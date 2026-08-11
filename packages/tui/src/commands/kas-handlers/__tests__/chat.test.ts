import {
  describe,
  it,
  expect,
  mock,
  beforeEach,
  afterEach,
  afterAll,
} from 'bun:test';
import {
  noteCloudScrollbackRepaint,
  cancelCloudScrollbackReconcile,
  isCloudScrollbackReconcileArmed,
} from '../../cloud-scrollback-reconcile';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  AgentEventType,
  type AgentStreamEvent,
} from '../../../types/agent-events';
import {
  __setListAllSessionsOverrideForTests,
  type ListAllSessionsResult,
} from '../../../utils/list-all-sessions-cli';

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

// Track calls to cancelCloudClearRewipes so tests can verify session switches
// cancel pending /clear re-wipe timers without leaking into the next session.
const mockCancelCloudClearRewipes = mock(() => {});
mock.module('../../effects', () => ({
  cancelCloudClearRewipes: () => mockCancelCloudClearRewipes(),
}));

// Mock listAllSessions so the picker tests don't need a real binary.
// The merged listing is the contract the handler consumes; the
// spawn-and-parse contract is exercised by
// `utils/__tests__/list-all-sessions-cli.test.ts` and the real binary
// surface is exercised by `acp_integ_tests/chat-command.test.ts`.
const mockListAllSessions = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: false, error: 'not stubbed' })
);

beforeEach(() => {
  __setListAllSessionsOverrideForTests(() => mockListAllSessions());
  // The columnar picker + live-client overlay are dark-shipped behind the
  // remote-sandbox feature; these tests exercise the feature-ON flow.
  process.env.KIRO_ENABLED_FEATURES = JSON.stringify(['remote_sandbox']);
  features._resetForTests();
});

afterEach(() => {
  __setListAllSessionsOverrideForTests(undefined);
  delete process.env.KIRO_ENABLED_FEATURES;
  features._resetForTests();
});

afterAll(() => {
  mock.restore();
});

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

import { handleChat, loadExistingSession } from '../chat';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';
import { features } from '../../../features';

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
      const showPicker = ctx._spies.setShowSessionPicker as any;
      expect(showPicker).toHaveBeenCalled();
      const rows = showPicker.mock.calls[0][1];
      expect(rows.map((r: any) => r.sessionId)).toEqual(['aaaa1111']);
    });

    it('excludes the current KAS session under its sess_ alias', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'active',
            source: 'v3',
            title: 'Current alias',
            updatedAt: new Date().toISOString(),
          },
          {
            sessionId: 'other',
            source: 'v3',
            title: 'Other',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'sess_active',
          listSessions: async () => ({ sessions: [] }),
        } as any,
      });

      await handleChat(CHAT_CMD, '', ctx);

      const showPicker = ctx._spies.setShowSessionPicker as any;
      const rows = showPicker.mock.calls[0][1];
      expect(rows.map((row: any) => row.sessionId)).toEqual(['other']);
    });

    it('dark-ship: feature OFF keeps the legacy selection menu (no columnar panel, no live overlay)', async () => {
      delete process.env.KIRO_ENABLED_FEATURES;
      features._resetForTests();
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
        ],
      });
      const listSessions = mock(() => Promise.resolve({ sessions: [] }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'bbbb2222', listSessions } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      // Released path: selection menu via setActiveCommand, not the panel.
      expect(ctx._spies.setShowSessionPicker as any).not.toHaveBeenCalled();
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalled();
      expect(setActive.mock.calls[0][0].options[0].value).toBe('aaaa1111');
      // And the live-client overlay call never fires (no new RPC on released builds).
      expect(listSessions).not.toHaveBeenCalled();
    });

    it('resets the per-session cloud scope when loading a different session', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          loadSession: async () => ({ sessionId: 'other' }),
        } as any,
      });
      await handleChat(CHAT_CMD, 'other', ctx, { argIsSynthetic: true });
      expect(ctx._spies.resetCloudSessionScope).toHaveBeenCalled();
    });

    it('passes the selected storage engine to local session conversion', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          loadSession: async () => ({ sessionId: 'classic-session' }),
        } as any,
      });

      const loaded = await loadExistingSession(ctx, 'classic-session', {
        source: 'local',
        sourceFormat: 'classic',
      });

      expect(loaded).toBe(true);
      expect(mockEnsureSession).toHaveBeenLastCalledWith(
        expect.objectContaining({
          sourceSessionId: 'classic-session',
          sourceFormat: 'classic',
          targetFormat: 'kas',
        })
      );
    });

    it('surfaces cloud rows from the live client that the shell-out omitted', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'local1',
            source: 'v3',
            title: 'Local one',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          listSessions: async () => ({
            sessions: [
              {
                sessionId: 'cloud1',
                cwd: '/x',
                title: 'Cloud task',
                updatedAt: new Date().toISOString(),
                executionTarget: { kind: 'cloud-sandbox' },
                status: 'in_progress',
              },
            ],
          }),
        } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showPicker = ctx._spies.setShowSessionPicker as any;
      expect(showPicker).toHaveBeenCalled();
      const rows = showPicker.mock.calls[0][1];
      const cloud = rows.find((r: any) => r.sessionId === 'cloud1');
      expect(cloud).toBeDefined();
      expect(cloud.environment).toBe('cloud');
      expect(cloud.status).toBe('working');
      // The local shell-out row is still present.
      expect(rows.some((r: any) => r.sessionId === 'local1')).toBe(true);
    });

    it('sorts merged local + cloud rows most-recent-first (mixed UTC-Z timestamps)', async () => {
      // Both sources emit UTC ISO-8601 (Z) timestamps, so lexicographic
      // localeCompare equals chronological order -- a cloud row newer than the
      // local rows must sort to the top even when the shell-out lists it last.
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'localOld',
            source: 'v3',
            title: 'Oldest local',
            updatedAt: '2026-01-01T00:00:00.000Z',
          },
          {
            sessionId: 'localMid',
            source: 'v3',
            title: 'Middle local',
            updatedAt: '2026-06-15T12:30:00.000Z',
          },
        ],
      });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          listSessions: async () => ({
            sessions: [
              {
                sessionId: 'cloudNew',
                cwd: '/x',
                title: 'Newest cloud',
                updatedAt: '2026-07-01T09:00:00.000Z',
                executionTarget: { kind: 'cloud-sandbox' },
                status: 'idle',
              },
            ],
          }),
        } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showPicker = ctx._spies.setShowSessionPicker as any;
      const rows = showPicker.mock.calls[0][1];
      expect(rows.map((r: any) => r.sessionId)).toEqual([
        'cloudNew',
        'localMid',
        'localOld',
      ]);
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

    it('strips raw newlines from picker row titles', async () => {
      // Multi-line titles arrive when KAS seeds the title from a first
      // prompt that contains real newlines. The columnar picker renders each
      // row on one line, so embedded `\n`s mangle the layout — the title must
      // collapse them.
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
      const showPicker = ctx._spies.setShowSessionPicker as any;
      const rows = showPicker.mock.calls[0][1];
      const title = rows[0]!.title as string;
      expect(title).not.toContain('\n');
      expect(title).not.toContain('\r');
      expect(title).toContain('fix the bug');
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

    it('accepts a double-quoted path containing spaces', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/out.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess' } as any,
      });
      await handleChat(CHAT_CMD, 'save "/Users/me/test space/test.json"', ctx);
      expect(mockExportSession).toHaveBeenCalledTimes(1);
      const call = mockExportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.out).toBe('/Users/me/test space/test.json');
    });

    it('accepts a single-quoted path with spaces alongside --force', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/out.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess' } as any,
      });
      await handleChat(
        CHAT_CMD,
        "save --force '/Users/me/test space/test.json'",
        ctx
      );
      const call = mockExportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.force).toBe(true);
      expect(call.out).toBe('/Users/me/test space/test.json');
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
        message: 'session not found',
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

    it('accepts a quoted archive path containing spaces', async () => {
      const spacedDir = join(tmpDir, 'test space');
      mkdirSync(spacedDir);
      const spacedArchive = join(spacedDir, 'test.zip');
      writeFileSync(spacedArchive, 'not a real zip');
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
      await handleChat(CHAT_CMD, `load "${spacedArchive}"`, ctx);
      expect(mockImportSession).toHaveBeenCalledTimes(1);
      const call = mockImportSession.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(call.archivePath).toBe(spacedArchive);
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
        message: 'archive is not a zip',
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

    it('new (cloud): arms the event-driven scrollback reconcile so slow sandbox startup cannot leave pre-new rows', async () => {
      try {
        const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
        const ctx = createMockCommandContext({
          kasCommands: [CHAT_CMD],
          kiro: {
            newSession,
            isCloudSessionActive: () => true,
            getSessionRepositories: () => null,
          } as any,
        });
        await handleChat(CHAT_CMD, 'new', ctx);
        // Armed → wiped once now, and re-wipes on each later cloud repaint (a
        // slow sandbox can't outlast a fixed timer because there is none).
        expect(isCloudScrollbackReconcileArmed()).toBe(true);
        const before = (ctx._spies.bumpLiteScrollbackClear as any).mock.calls
          .length;
        noteCloudScrollbackRepaint();
        expect(
          (ctx._spies.bumpLiteScrollbackClear as any).mock.calls.length
        ).toBe(before + 1);
      } finally {
        cancelCloudScrollbackReconcile();
      }
    });

    it('new (local): arms no reconcile — the single reset wipe lands last', async () => {
      try {
        const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
        const ctx = createMockCommandContext({
          kasCommands: [CHAT_CMD],
          kiro: {
            newSession,
            isCloudSessionActive: () => false,
            getSessionRepositories: () => null,
          } as any,
        });
        await handleChat(CHAT_CMD, 'new', ctx);
        expect(isCloudScrollbackReconcileArmed()).toBe(false);
      } finally {
        cancelCloudScrollbackReconcile();
      }
    });

    it('new (cloud): clears the post-create checklist during create, then arms it on success', async () => {
      try {
        const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
        const ctx = createMockCommandContext({
          kasCommands: [CHAT_CMD],
          kiro: {
            newSession,
            isCloudSessionActive: () => true,
            getSessionRepositories: () => null,
          } as any,
        });
        await handleChat(CHAT_CMD, 'new', ctx);
        const calls = (ctx._spies.setCloudNewSessionChecklist as any).mock
          .calls;
        // First cleared (loader shows alone during create), then armed on success.
        expect(calls.map((c: unknown[]) => c[0])).toEqual([false, true]);
      } finally {
        cancelCloudScrollbackReconcile();
      }
    });

    it('new (local): never arms the post-create cloud checklist', async () => {
      try {
        const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
        const ctx = createMockCommandContext({
          kasCommands: [CHAT_CMD],
          kiro: {
            newSession,
            isCloudSessionActive: () => false,
            getSessionRepositories: () => null,
          } as any,
        });
        await handleChat(CHAT_CMD, 'new', ctx);
        const calls = (ctx._spies.setCloudNewSessionChecklist as any).mock
          .calls;
        // Cleared up front, but never armed (no `true`) off cloud.
        expect(calls.some((c: unknown[]) => c[0] === true)).toBe(false);
      } finally {
        cancelCloudScrollbackReconcile();
      }
    });

    it('new: drops the client display caches so panels cannot show the previous session', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.resetClientDisplayCaches).toHaveBeenCalled();
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
      // The post-/chat new checklist describes the outgoing session; a resume
      // must clear it so it can't linger under the loaded transcript.
      expect(ctx._spies.setCloudNewSessionChecklist).toHaveBeenCalledWith(
        false
      );
      // Session switches APPEND each load's replay to the transcript (matching
      // local-session behavior in released builds) — the previous history must
      // NOT be reset. Replayed user rows with already-rendered persisted ids
      // are handled by the cloud-replay dedupe-skip in the stream handler.
      expect(ctx._spies.resetMessages).not.toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('sid');
    });

    it('bare sessionId: drops the client display caches so panels cannot show the previous session', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.resetClientDisplayCaches).toHaveBeenCalled();
    });

    it('bare sessionId: replays history through the persistent renderer', async () => {
      const event = { type: AgentEventType.TurnStart } as AgentStreamEvent;
      const loadSession = mock(
        async (
          _sessionId: string,
          onHistoryEvent: (e: AgentStreamEvent) => void
        ) => {
          onHistoryEvent(event);
          return { sessionId: 'sid' };
        }
      );
      const replayHistory = mock(() => true);
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, replayHistory } as any,
      });

      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });

      expect(replayHistory).toHaveBeenCalledWith([event]);
      expect(ctx._spies.createStreamEventHandler).not.toHaveBeenCalled();
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

    it('new: a rejected newSession restores the previous session cloud scope', async () => {
      const newSession = mock(() => Promise.reject(new Error('boom')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession, sessionId: 'prev-sid' } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      // The scope was stashed under the previous session's id before the RPC;
      // the failure must bring it back — that session is still the active one.
      expect(ctx._spies.stashCloudSessionScope).toHaveBeenCalledWith(
        'prev-sid'
      );
      expect(ctx._spies.restoreCloudSessionScope).toHaveBeenCalledWith(
        'prev-sid'
      );
    });

    it('bare sessionId: a rejected loadSession restores the previous session cloud scope', async () => {
      const loadSession = mock(() => Promise.reject(new Error('nope')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, sessionId: 'prev-sid' } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.stashCloudSessionScope).toHaveBeenCalledWith(
        'prev-sid'
      );
      expect(ctx._spies.restoreCloudSessionScope).toHaveBeenCalledWith(
        'prev-sid'
      );
    });

    it('new: a rejected newSession restores the display caches from the pre-reset snapshot', async () => {
      const newSession = mock(() => Promise.reject(new Error('boom')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession, sessionId: 'prev-sid' } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      const snapshot = (ctx._spies.resetClientDisplayCaches as any).mock
        .results[0]?.value;
      expect(ctx._spies.restoreClientDisplayCaches).toHaveBeenCalledWith(
        snapshot
      );
    });

    it('new: a resolved newSession does NOT restore the display caches', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.restoreClientDisplayCaches).not.toHaveBeenCalled();
    });

    it('bare sessionId: a rejected loadSession restores the display caches from the pre-reset snapshot', async () => {
      const loadSession = mock(() => Promise.reject(new Error('nope')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, sessionId: 'prev-sid' } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      const snapshot = (ctx._spies.resetClientDisplayCaches as any).mock
        .results[0]?.value;
      expect(ctx._spies.restoreClientDisplayCaches).toHaveBeenCalledWith(
        snapshot
      );
    });

    it('bare sessionId: a resolved loadSession does NOT restore the display caches', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.restoreClientDisplayCaches).not.toHaveBeenCalled();
    });

    it('loading a cloud session from a local one announces "Connected"', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, isCloudSessionActive: () => true } as any,
      });
      // Pre-load state is local; the message must reflect the LOADED session.
      ctx.cloudSessionActive = false;
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      const addSystemMessage = ctx._spies.addSystemMessage as any;
      expect(
        addSystemMessage.mock.calls.some(
          (c: any[]) => c[0] === 'Connected to session sid'
        )
      ).toBe(true);
    });

    it('loading a local session from a cloud one announces "Loaded"', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, isCloudSessionActive: () => false } as any,
      });
      ctx.cloudSessionActive = true;
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      const addSystemMessage = ctx._spies.addSystemMessage as any;
      expect(
        addSystemMessage.mock.calls.some(
          (c: any[]) => c[0] === 'Loaded session sid'
        )
      ).toBe(true);
    });

    it('new: cancels pending cloud /clear re-wipe timers', async () => {
      mockCancelCloudClearRewipes.mockClear();
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(mockCancelCloudClearRewipes).toHaveBeenCalled();
    });

    it('bare sessionId: cancels pending cloud /clear re-wipe timers', async () => {
      mockCancelCloudClearRewipes.mockClear();
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession, isCloudSessionActive: () => false } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(mockCancelCloudClearRewipes).toHaveBeenCalled();
    });

    it('new: hydrates footer from getSessionRepositories when cloud', async () => {
      const repos = [
        { name: 'acme/foo', branch: 'main' },
        { name: 'acme/bar' },
      ];
      const newSession = mock(() => Promise.resolve({ sessionId: 'csid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          newSession,
          isCloudSessionActive: () => true,
          getSessionRepositories: () => repos,
        } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.applyRepoFooter).toHaveBeenCalledWith(
        ['acme/foo', 'acme/bar'],
        'main'
      );
    });

    it('bare sessionId: hydrates footer from boundRepos when cloud', async () => {
      const repos = [{ name: 'org/repo', branch: 'dev' }];
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          loadSession,
          isCloudSessionActive: () => true,
          getSessionRepositories: () => repos,
        } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.applyRepoFooter).toHaveBeenCalledWith(
        ['org/repo'],
        'dev'
      );
      // Should NOT fall through to restoreCloudSessionScope
      expect(ctx._spies.restoreCloudSessionScope).not.toHaveBeenCalledWith(
        'sid'
      );
    });

    it('bare sessionId: falls back to stash when repos not reported', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          loadSession,
          isCloudSessionActive: () => true,
          getSessionRepositories: () => null,
        } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.applyRepoFooter).not.toHaveBeenCalled();
      expect(ctx._spies.restoreCloudSessionScope).toHaveBeenCalledWith('sid');
    });
  });

  describe('cloud-session gate (save/load refuse; local untouched)', () => {
    beforeEach(() => {
      mockExportSession.mockReset();
      mockImportSession.mockReset();
    });

    it('cloud: save refuses with the exact message and never spawns', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-cloud' } as any,
        cloudSessionActive: true,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      expect(mockExportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        ['/chat save is not available for a cloud session yet.', 'error', 5000],
      ]);
    });

    it('cloud: load refuses with the exact message and never spawns', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-cloud' } as any,
        cloudSessionActive: true,
      });
      await handleChat(CHAT_CMD, 'load /tmp/x.zip', ctx);
      expect(mockImportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        ['/chat load is not available for a cloud session yet.', 'error', 5000],
      ]);
    });

    it('cloud: the /sessions alias refuses under its own name', async () => {
      // /sessions routes to this same handler; the refusal must name the
      // command the user actually typed, not /chat.
      const SESSIONS_CMD: KasCommand = {
        ...CHAT_CMD,
        name: KasCommandName.Sessions,
      };
      const ctx = createMockCommandContext({
        kasCommands: [SESSIONS_CMD],
        kiro: { sessionId: 'sess-cloud' } as any,
        cloudSessionActive: true,
      });
      await handleChat(SESSIONS_CMD, 'load /tmp/x.zip', ctx);
      expect(mockImportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        [
          '/sessions load is not available for a cloud session yet.',
          'error',
          5000,
        ],
      ]);
    });

    it('cloud: bare "save" (no path) still refuses before the usage check', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-cloud' } as any,
        cloudSessionActive: true,
      });
      await handleChat(CHAT_CMD, 'save', ctx);
      expect(mockExportSession).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toBe(
        '/chat save is not available for a cloud session yet.'
      );
    });

    it('local: save is untouched by the gate (export still runs, no refusal)', async () => {
      mockExportSession.mockReturnValue({ ok: true, path: '/abs/x.zip' });
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-local' } as any,
        cloudSessionActive: false,
      });
      await handleChat(CHAT_CMD, 'save /tmp/x.zip', ctx);
      expect(mockExportSession).toHaveBeenCalledTimes(1);
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some((c: any[]) =>
          String(c[0]).includes('not available for a cloud session')
        )
      ).toBe(false);
    });

    it('local: load is untouched by the gate (usage error, no refusal)', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { sessionId: 'sess-local' } as any,
        cloudSessionActive: false,
      });
      await handleChat(CHAT_CMD, 'load', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls.at(-1)?.[0])).toContain('Usage');
      expect(
        showAlert.mock.calls.some((c: any[]) =>
          String(c[0]).includes('not available for a cloud session')
        )
      ).toBe(false);
    });
  });
});
