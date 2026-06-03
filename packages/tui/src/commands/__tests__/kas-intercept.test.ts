import { afterAll, describe, expect, it, mock } from 'bun:test';
import type { ListAllSessionsResult } from '../../utils/list-all-sessions-cli';

// Stub listAllSessions so the KAS-mode /chat path can be exercised
// without spawning a real binary. The handler's dependency on the
// merged listing is the contract under test here; the spawn-and-parse
// contract is covered by `utils/__tests__/list-all-sessions-cli.test.ts`.
// `afterAll(() => mock.restore())` prevents this stub from leaking
// into sibling test files when bun loads them in the same process.
const mockListAllSessions = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: true, cwd: '/tmp/test', sessions: [] })
);
mock.module('../../utils/list-all-sessions-cli', () => ({
  listAllSessions: () => mockListAllSessions(),
}));

afterAll(() => {
  mock.restore();
});

import { dispatch } from '../dispatcher';
import { createMockCommandContext } from '../__tests__/test-helpers';
import type { SlashCommand } from '../../stores/app-store';
import type { KasCommand } from '../../kas-commands';
import { KasCommandName } from '../../kas-commands';

const CHAT_CMD: SlashCommand = {
  name: '/chat',
  description: 'x',
  source: 'backend',
  meta: { inputType: 'selection', local: true },
};
const HELP_CMD: KasCommand = {
  name: KasCommandName.Help,
  description: 'x',
  meta: { inputType: 'panel' },
};
const CONTEXT_CMD: SlashCommand = {
  name: '/context',
  description: 'x',
  source: 'backend',
  meta: {
    inputType: 'panel',
    subcommands: ['show', 'add', 'remove', 'clear'],
  },
};

describe('dispatcher KAS intercept', () => {
  it("agentEngine='kas' + /chat: skips backend executeCommand and runs KAS handler", async () => {
    mockListAllSessions.mockClear();
    const exec = mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(CHAT_CMD, '', ctx);
    // KAS handler shells out to listAllSessions; rust backend not touched.
    expect(mockListAllSessions.mock.calls.length).toBe(1);
    expect((exec as any).mock.calls.length).toBe(0);
  });

  it("agentEngine='v2' + /chat save: falls through to V2 backend executeCommand", async () => {
    const exec = mock(() =>
      Promise.resolve({
        success: true,
        message: 'saved',
        data: undefined,
      })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
      } as any,
    });
    ctx.agentEngine = 'v2';
    await dispatch(CHAT_CMD, 'save', ctx);
    expect((exec as any).mock.calls.length).toBe(1);
    expect((exec as any).mock.calls[0][0]).toEqual({
      command: 'chat',
      args: { value: 'save' },
    });
  });

  it("agentEngine='kas' + non-handler command: falls through to existing dispatch", async () => {
    const ctx = createMockCommandContext({
      kasCommands: [HELP_CMD],
    });
    ctx.agentEngine = 'kas';
    await dispatch(HELP_CMD, '', ctx);
    expect(ctx._spies.setActiveCommand).toHaveBeenCalled();
  });

  it("agentEngine='kas' + /context add: handler is invoked and forwards args via typed contextAdd", async () => {
    const contextAdd = mock(() =>
      Promise.resolve({
        success: true,
        message: "Added 'foo.ts' to context",
      })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CONTEXT_CMD],
      kiro: {
        sessionId: 'cur',
        contextAdd,
        getCachedContextBreakdown: mock(() => null),
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(CONTEXT_CMD, 'add foo.ts', ctx);

    // Handler dispatches directly to the typed client method — no
    // executeCommand round-trip.
    expect((contextAdd as any).mock.calls.length).toBe(1);
    expect((contextAdd as any).mock.calls[0]).toEqual([
      'foo.ts',
      { force: false },
    ]);
    // Mutation result surfaces as a success alert, not a panel.
    expect(ctx._spies.showAlert).toHaveBeenCalled();
    expect(ctx._spies.setShowContextBreakdown).not.toHaveBeenCalled();
  });

  it("agentEngine='kas' + bare /context: handler opens the panel from the cached breakdown", async () => {
    const cached = { contextFiles: { tokens: 100, percent: 5 } };
    const ctx = createMockCommandContext({
      slashCommands: [CONTEXT_CMD],
      kiro: {
        sessionId: 'cur',
        getCachedContextBreakdown: mock(() => cached),
        contextShow: mock(() => Promise.resolve({ entries: [] })),
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(CONTEXT_CMD, '', ctx);

    expect(ctx._spies.setShowContextBreakdown).toHaveBeenCalled();
    // No agent round-trip when the breakdown is cached.
    expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(0);
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it("agentEngine='v2' + /context add: V2 dispatcher pipeline runs (no kas-handler intercept)", async () => {
    const exec = mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CONTEXT_CMD],
      kiro: { sessionId: 'cur', executeCommand: exec } as any,
    });
    ctx.agentEngine = 'v2';
    await dispatch(CONTEXT_CMD, 'add foo.ts', ctx);

    // V2 dispatcher path: still ends up calling executeCommand, but via
    // the standard pipeline rather than the kas-handler. Asserting it was
    // called proves the intercept correctly DID NOT short-circuit.
    expect((exec as any).mock.calls.length).toBe(1);
  });
});
