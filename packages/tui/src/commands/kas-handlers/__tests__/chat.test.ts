import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { ListAllSessionsResult } from '../../../utils/list-all-sessions-cli';

// Mock the session-archive-cli helper at module load. The handler under
// test calls these to shell out to `kiro-cli chat _ export-session` /
// `import-session`; the unit tests stub the module so no real spawn
// happens. The full wire-level coverage of the spawn-and-parse contract
// lives in `utils/__tests__/session-archive-cli.test.ts`, and the
// end-to-end coverage against the real binary lives in
// `acp_integ_tests/chat-command.test.ts`.
const mockExportSession = mock();
const mockImportSession = mock();
mock.module('../../../utils/session-archive-cli', () => ({
  exportSession: (...args: unknown[]) =>
    mockExportSession(...(args as Parameters<typeof mockExportSession>)),
  importSession: (...args: unknown[]) =>
    mockImportSession(...(args as Parameters<typeof mockImportSession>)),
}));

// Mock listAllSessions so the picker tests don't need a real binary.
// The merged listing is the contract the handler consumes; the
// spawn-and-parse contract is exercised by
// `utils/__tests__/list-all-sessions-cli.test.ts` and the real binary
// surface is exercised by `acp_integ_tests/chat-command.test.ts`.
const mockListAllSessions = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: false, error: 'not stubbed' })
);
mock.module('../../../utils/list-all-sessions-cli', () => ({
  listAllSessions: () => mockListAllSessions(),
}));

// Mock ensureSession so bare-id load tests don't spawn a real binary.
// The handler routes every load through ensure-session with
// `sourceFormat: 'auto'` (native ids resolve via fast filesystem
// probe; cross-engine ids trigger conversion). The spawn-and-parse
// contract is covered by `utils/__tests__/ensure-session-cli.test.ts`.
const mockEnsureSession = mock<
  (
    input: import('../../../utils/ensure-session-cli').EnsureSessionInput
  ) => Promise<import('../../../utils/ensure-session-cli').EnsureSessionResult>
>((input) =>
  Promise.resolve({
    ok: true,
    sessionId: input.sourceSessionId,
    converted: false,
  })
);
mock.module('../../../utils/ensure-session-cli', () => ({
  ensureSession: (input: unknown) =>
    mockEnsureSession(
      input as import('../../../utils/ensure-session-cli').EnsureSessionInput
    ),
}));

// Mock resolveAgentEngine so picker option encoding can be exercised
// in either active-engine direction without leaking process env state.
const mockResolveAgentEngine = mock<() => 'kas' | 'v2'>(() => 'kas');
mock.module('../../../agent-engine', () => ({
  resolveAgentEngine: () => mockResolveAgentEngine(),
}));

import { handleChat } from '../chat';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';

const CHAT_CMD: KasCommand = {
  name: KasCommandName.Chat,
  description: 'Load a previous session or start a new one',
  meta: { inputType: 'selection', local: true },
};

describe('handleChat (KAS-mode dispatch)', () => {
  describe('list (no args)', () => {
    it('opens picker with sessions excluding the current one', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'aaaa1111',
            source: 'v3',
            title: 'Other',
            updatedAt: new Date().toISOString(),
          },
          {
            sessionId: 'bbbb2222',
            source: 'v3',
            title: 'Current',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'bbbb2222' } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalled();
      const arg = setActive.mock.calls[0][0];
      expect(arg.options.map((o: any) => o.value)).toEqual(['aaaa1111']);
    });

    it('alerts when no other sessions exist', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'cur' } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalledWith(
        'No previous sessions found',
        'error',
        3000
      );
    });

    it('alerts on listAllSessions failure', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: false,
        error: 'boom',
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'cur' } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(showAlert.mock.calls[0][0]).toContain('boom');
      expect(showAlert.mock.calls[0][1]).toBe('error');
    });

    it('strips raw newlines from picker option labels', async () => {
      // Multi-line titles arrive when KAS seeds the title from a first
      // prompt that contains real newlines. The Ink-based autocomplete
      // picker renders each option on one row, so embedded `\n`s
      // mangle the option layout. The label must collapse them.
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'multiline-1',
            source: 'v3',
            title: 'fix the bug\nwhere foo crashes\nwhen bar is null',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'cur-id' } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      const arg = setActive.mock.calls[0][0];
      const label = arg.options[0]!.label as string;
      expect(label).not.toContain('\n');
      expect(label).not.toContain('\r');
      expect(label).toContain('fix the bug');
    });
  });

  describe('save (shells out to chat _ export-session)', () => {
    beforeEach(() => {
      mockExportSession.mockReset();
      mockImportSession.mockReset();
    });

    it('invokes exportSession with sessionId, cwd, and out path', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/tmp/x.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-current' } as any,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      expect(mockExportSession).toHaveBeenCalledTimes(1);
      const call = mockExportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.sessionId).toBe('sess-current');
      expect(call.out).toBe('/tmp/x.zip');
      expect(call.force).toBe(false);
      expect(call.cwd).toBe(process.cwd());
    });

    it('passes force: true when --force is provided', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/tmp/x.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess' } as any,
      });
      await handleChat(CHAT_CMD, 'save --force /tmp/x.zip', ctx);
      const call = mockExportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.force).toBe(true);
      expect(call.out).toBe('/tmp/x.zip');
    });

    it('shows a success alert with the returned path', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/abs/x.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 's' } as any,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      const showAlert = ctx._spies.showAlert as any;
      const successCall = showAlert.mock.calls.find(
        (c: any[]) => c[1] === 'success'
      );
      expect(successCall).toBeDefined();
      expect(String(successCall![0])).toContain('/abs/x.zip');
    });

    it('shows an error alert when exportSession reports failure', async () => {
      mockExportSession.mockReturnValue({
        ok: false,
        error: 'session not found',
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 's' } as any,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some(
          (c: any[]) => c[0] === 'session not found' && c[1] === 'error'
        )
      ).toBe(true);
    });

    it('alerts and skips the spawn when no path is provided', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 's' } as any,
      });
      await handleChat(CHAT_CMD, 'save', ctx);
      expect(mockExportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain('Usage');
    });

    it('alerts and skips the spawn when there is no active session', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: null } as any,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      expect(mockExportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain(
        'No active session'
      );
    });
  });

  describe('load (shells out to chat _ import-session)', () => {
    let tmpDir: string;
    let archivePath: string;

    beforeEach(() => {
      mockExportSession.mockReset();
      mockImportSession.mockReset();
      // Real on-disk file the handler can stat; the spawn itself is
      // mocked, so the contents don't matter - only existence + isFile.
      tmpDir = mkdtempSync(join(tmpdir(), 'kiro-chat-load-'));
      archivePath = join(tmpDir, 'session.zip');
      writeFileSync(archivePath, 'not a real zip');
    });

    afterEach(() => {
      rmSync(tmpDir, { recursive: true, force: true });
    });

    it('invokes importSession with archivePath and cwd', async () => {
      mockImportSession.mockReturnValue({
        ok: true,
        path: '/sessions/abc/sess_imported-1',
      });
      const loadSession = mock(() =>
        Promise.resolve({ sessionId: 'sess_imported-1' })
      );
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      expect(mockImportSession).toHaveBeenCalledTimes(1);
      const call = mockImportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.archivePath).toBe(archivePath);
      expect(call.cwd).toBe(process.cwd());
    });

    it('calls kiro.loadSession with the basename of the imported path', async () => {
      mockImportSession.mockReturnValue({
        ok: true,
        path: '/sessions/abc/sess_imported-1',
      });
      const loadSession = mock(() =>
        Promise.resolve({ sessionId: 'sess_imported-1' })
      );
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      expect((loadSession as any).mock.calls.length).toBe(1);
      expect((loadSession as any).mock.calls[0][0]).toBe('sess_imported-1');
    });

    it('displays "Loaded session from <path>" on success', async () => {
      mockImportSession.mockReturnValue({
        ok: true,
        path: '/sessions/abc/sess_imported-1',
      });
      const loadSession = mock(() =>
        Promise.resolve({ sessionId: 'sess_imported-1' })
      );
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      const addSystemMessage = ctx._spies.addSystemMessage as any;
      expect(
        addSystemMessage.mock.calls.some(
          (c: any[]) => c[0] === `Loaded session from ${archivePath}`
        )
      ).toBe(true);
    });

    it('shows error alert and skips loadSession when import fails', async () => {
      mockImportSession.mockReturnValue({
        ok: false,
        error: 'archive is not a zip',
      });
      const loadSession = mock(() => Promise.resolve({}));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      expect(loadSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some(
          (c: any[]) => c[0] === 'archive is not a zip' && c[1] === 'error'
        )
      ).toBe(true);
    });

    it('alerts and skips the spawn when no path is provided', async () => {
      const ctx = createMockCommandContext({ kasCommands: [CHAT_CMD] });
      await handleChat(CHAT_CMD, 'load', ctx);
      expect(mockImportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain('Usage');
    });

    it('alerts "No such file" and skips the spawn when path does not exist', async () => {
      const missing = join(tmpDir, 'does-not-exist.zip');
      const ctx = createMockCommandContext({ kasCommands: [CHAT_CMD] });
      await handleChat(CHAT_CMD, `load ${missing}`, ctx);
      expect(mockImportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain(
        'No such file'
      );
    });

    it('alerts "Not a file" and skips the spawn when path is a directory', async () => {
      const dirPath = join(tmpDir, 'a-directory');
      mkdirSync(dirPath);
      const ctx = createMockCommandContext({ kasCommands: [CHAT_CMD] });
      await handleChat(CHAT_CMD, `load ${dirPath}`, ctx);
      expect(mockImportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain('Not a file');
    });

    it('ends with the loading message cleared on success', async () => {
      mockImportSession.mockReturnValue({
        ok: true,
        path: '/sessions/abc/sess_imported-1',
      });
      const loadSession = mock(() =>
        Promise.resolve({ sessionId: 'sess_imported-1' })
      );
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      const setLoading = ctx._spies.setLoadingMessage as any;
      expect(setLoading.mock.calls.at(-1)?.[0]).toBe(null);
    });

    it('does not touch setLoadingMessage on import failure', async () => {
      mockImportSession.mockReturnValue({ ok: false, error: 'boom' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession: mock(() => Promise.resolve({})) } as any,
      });
      await handleChat(CHAT_CMD, `load ${archivePath}`, ctx);
      expect(ctx._spies.setLoadingMessage).not.toHaveBeenCalled();
    });
  });

  describe('unknown subcommand (typed directly)', () => {
    it('alerts and does not call loadSession', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      // No `argIsSynthetic` flag - simulates the user typing `/chat foo`
      // directly rather than picking from the picker.
      await handleChat(CHAT_CMD, 'foo', ctx);
      expect((loadSession as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls[0][0])).toContain(
        'Unknown /chat subcommand'
      );
    });
  });

  describe('new / switch (self-contained, calls kiro directly)', () => {
    it('new: calls kiro.newSession + clears UI + resets messages', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect((newSession as any).mock.calls.length).toBe(1);
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
      expect(ctx._spies.resetMessages).toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('newSID');
    });

    it('new <prompt>: forwards the prompt via sendMessage', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new hello there', ctx);
      expect(ctx._spies.sendMessage).toHaveBeenCalledWith('hello there');
    });

    it('new: alerts on failure without leaving loading state', async () => {
      const newSession = mock(() => Promise.reject(new Error('boom')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.setLoadingMessage).toHaveBeenLastCalledWith(null);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls.at(-1)?.[1]).toBe('error');
    });

    it('bare sessionId: calls kiro.loadSession + clears UI', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect((loadSession as any).mock.calls.length).toBe(1);
      expect((loadSession as any).mock.calls[0][0]).toBe('sid');
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('sid');
    });

    it('bare sessionId: alerts on loadSession failure', async () => {
      const loadSession = mock(() => Promise.reject(new Error('nope')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.setLoadingMessage).toHaveBeenLastCalledWith(null);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls.at(-1)?.[1]).toBe('error');
    });
  });
});
