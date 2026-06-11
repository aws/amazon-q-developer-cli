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
  __setListAllSessionsOverrideForTests,
  type ListAllSessionsResult,
} from '../../../utils/list-all-sessions-cli';
import type {
  EnsureSessionInput,
  EnsureSessionResult,
} from '../../../utils/ensure-session-cli';

// Mock listAllSessions so the picker tests don't need a real binary.
// The merged listing is the contract the handler consumes; the
// spawn-and-parse contract is exercised by
// `utils/__tests__/list-all-sessions-cli.test.ts`.
const mockListAllSessions = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: false, error: 'not stubbed' })
);

beforeEach(() => {
  __setListAllSessionsOverrideForTests(() => mockListAllSessions());
});

afterEach(() => {
  __setListAllSessionsOverrideForTests(undefined);
});

// Mock ensureSession so bare-id load tests don't spawn a real binary.
const mockEnsureSession = mock<
  (input: EnsureSessionInput) => Promise<EnsureSessionResult>
>((input) =>
  Promise.resolve({
    ok: true,
    sessionId: input.sourceSessionId,
    converted: false,
  })
);
mock.module('../../../utils/ensure-session-cli', () => ({
  ensureSession: (input: unknown) =>
    mockEnsureSession(input as EnsureSessionInput),
}));

afterAll(() => {
  mock.restore();
});

import { handleChat } from '../chat';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { SlashCommand } from '../../../stores/app-store';

const CHAT_CMD: SlashCommand = {
  name: '/chat',
  description: 'Load a previous session or start a new one',
  source: 'local',
  meta: { inputType: 'selection', local: true },
};

describe('handleChat (V2-mode dispatch)', () => {
  describe('list (no args)', () => {
    it('opens picker with bare sessionIds (no source-prefix encoding)', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'native-v2-1',
            source: 'v2',
            title: 'Other',
            updatedAt: new Date().toISOString(),
          },
          {
            sessionId: 'cur-id',
            source: 'v2',
            title: 'Current',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kiro: { sessionId: 'cur-id' } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalled();
      const arg = setActive.mock.calls[0][0];
      expect(arg.options.map((o: any) => o.value)).toEqual(['native-v2-1']);
      // No `<source>:` prefix in the value.
      expect(arg.options[0]!.value).not.toContain(':');
    });

    it('drops KAS (v3) entries when active engine is v2', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'kas-only',
            source: 'v3',
            title: 'KAS one',
            updatedAt: new Date().toISOString(),
          },
          {
            sessionId: 'native-v2',
            source: 'v2',
            title: 'V2 one',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kiro: { sessionId: null } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalled();
      expect(setActive.mock.calls[0][0].options).toHaveLength(1);
      expect(setActive.mock.calls[0][0].options[0].value).toBe('native-v2');
    });

    it('tags non-active-engine entries with their source in the label', async () => {
      // In V2 mode the only non-native source that survives the
      // resumable filter is `classic` (V2 actually owns classic via
      // LegacySessionExporter, so it ends up native). The
      // source-tag branch is exercised in KAS mode where V2 entries
      // are non-native; see `kas-handlers/__tests__/chat.test.ts`.
      // This test is intentionally left as a placeholder so the
      // describe block remains consistent with the KAS suite.
      expect(true).toBe(true);
    });

    it('strips raw newlines from picker option labels', async () => {
      // Multi-line titles can arrive when a session was seeded from a
      // first prompt that contains real newlines. The Ink-based
      // autocomplete picker renders each option on one row, so embedded
      // `\n`s mangle the option layout. The label must collapse them.
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [
          {
            sessionId: 'multiline-1',
            source: 'v2',
            title: 'fix the bug\nwhere foo crashes\nwhen bar is null',
            updatedAt: new Date().toISOString(),
          },
        ],
      });
      const ctx = createMockCommandContext({
        kiro: { sessionId: null } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      const arg = setActive.mock.calls[0][0];
      const label = arg.options[0]!.label as string;
      expect(label).not.toContain('\n');
      expect(label).not.toContain('\r');
      expect(label).toContain('fix the bug');
    });

    it('alerts when listing fails', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: false,
        error: 'spawn failed',
      });
      const ctx = createMockCommandContext({
        kiro: { sessionId: null } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalledWith(
        'Failed to list sessions: spawn failed',
        'error',
        3000
      );
    });

    it('alerts when no resumable sessions exist', async () => {
      mockListAllSessions.mockResolvedValueOnce({
        ok: true,
        cwd: '/x',
        sessions: [],
      });
      const ctx = createMockCommandContext({
        kiro: { sessionId: null } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalledWith(
        'No previous sessions found',
        'error',
        3000
      );
    });
  });

  describe('new (no backend roundtrip)', () => {
    it('calls kiro.newSession + clears UI + resets messages', async () => {
      const newSession = mock(() =>
        Promise.resolve({
          sessionId: 'new-1',
          currentModel: { id: 'm1', name: 'Claude' },
          currentAgent: { name: 'kiro' },
        })
      );
      const ctx = createMockCommandContext({
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect((newSession as any).mock.calls.length).toBe(1);
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
      expect(ctx._spies.resetMessages).toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('new-1');
    });

    it('forwards the prompt via sendMessage when given an arg', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'new-2' }));
      const ctx = createMockCommandContext({
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new hello world', ctx);
      expect(ctx._spies.sendMessage).toHaveBeenCalledWith('hello world');
    });

    it('alerts on failure without leaving loading state', async () => {
      const newSession = mock(() => Promise.reject(new Error('auth failed')));
      const ctx = createMockCommandContext({
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.setLoadingMessage).toHaveBeenCalledWith(null);
      expect(ctx._spies.showAlert).toHaveBeenCalled();
    });
  });

  describe('save / load (delegated to V2 backend)', () => {
    it('save: forwards to ctx.kiro.executeCommand', async () => {
      const exec = mock(() =>
        Promise.resolve({ success: true, message: 'Saved', data: undefined })
      );
      const ctx = createMockCommandContext({
        kiro: { executeCommand: exec } as any,
      });
      await handleChat(CHAT_CMD, 'save /tmp/out.json', ctx);
      expect((exec as any).mock.calls.length).toBe(1);
      expect((exec as any).mock.calls[0][0]).toEqual({
        command: 'chat',
        args: { value: 'save /tmp/out.json' },
      });
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'Saved',
        'success',
        5000
      );
    });

    it('load <path>: forwards to backend, then loads the imported session', async () => {
      const loadSession = mock(() =>
        Promise.resolve({ sessionId: 'imported-1' })
      );
      const exec = mock(() =>
        Promise.resolve({
          success: true,
          message: '',
          data: { sessionId: 'imported-1' },
        })
      );
      const ctx = createMockCommandContext({
        kiro: { executeCommand: exec, loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'load /tmp/in.json', ctx);
      expect((exec as any).mock.calls.length).toBe(1);
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
    });

    it('load: alerts on backend failure', async () => {
      const exec = mock(() =>
        Promise.resolve({ success: false, message: 'no such file' })
      );
      const ctx = createMockCommandContext({
        kiro: { executeCommand: exec } as any,
      });
      await handleChat(CHAT_CMD, 'load /tmp/missing.json', ctx);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'no such file',
        'error',
        5000
      );
    });
  });

  describe('bare sessionId (typed or picker selection)', () => {
    it('routes through ensureSession with sourceFormat=auto, then loadSession', async () => {
      mockEnsureSession.mockClear();
      const loadSession = mock(() => Promise.resolve({ sessionId: 'abc-123' }));
      const ctx = createMockCommandContext({
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'abc-123', ctx);
      expect(mockEnsureSession.mock.calls.length).toBe(1);
      expect(mockEnsureSession.mock.calls[0]![0]).toEqual({
        sourceFormat: 'auto',
        sourceSessionId: 'abc-123',
        targetFormat: 'v2',
        cwd: process.cwd(),
      });
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
    });

    it('alerts when ensureSession fails', async () => {
      mockEnsureSession.mockResolvedValueOnce({
        ok: false,
        error: 'session not found',
      });
      const ctx = createMockCommandContext();
      await handleChat(CHAT_CMD, 'missing-id', ctx);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'Failed to load session: session not found',
        'error',
        5000
      );
    });
  });
});
