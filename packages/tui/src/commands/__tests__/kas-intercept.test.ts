import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  mock,
} from 'bun:test';
import {
  __setListAllSessionsOverrideForTests,
  type ListAllSessionsResult,
} from '../../utils/list-all-sessions-cli';

// Stub listAllSessions so the KAS-mode /chat path can be exercised
// without spawning a real binary. The handler's dependency on the
// merged listing is the contract under test here; the spawn-and-parse
// contract is covered by `utils/__tests__/list-all-sessions-cli.test.ts`.
const mockListAllSessions = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: true, cwd: '/tmp/test', sessions: [] })
);

beforeEach(() => {
  __setListAllSessionsOverrideForTests(() => mockListAllSessions());
});

afterEach(() => {
  __setListAllSessionsOverrideForTests(undefined);
});

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
  meta: {
    inputType: 'selection',
    local: true,
    subcommands: ['new', 'save', 'load'],
  },
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
const PROMPTS_CMD: KasCommand = {
  name: KasCommandName.Prompts,
  description: 'x',
  meta: { inputType: 'selection' },
};

describe('dispatcher KAS intercept', () => {
  it("agentEngine='kas' + /chat: skips backend executeCommand and runs KAS handler", async () => {
    mockListAllSessions.mockClear();
    const exec = mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    );
    const telemetry = mock(() => undefined);
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
        sendChatSlashCommandTelemetry: telemetry,
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(CHAT_CMD, '', ctx);
    // KAS handler shells out to listAllSessions; rust backend not touched.
    expect(mockListAllSessions.mock.calls.length).toBe(1);
    expect((exec as any).mock.calls.length).toBe(0);
    expect(telemetry).toHaveBeenCalledWith({
      command: '/chat',
      success: true,
    });
  });

  it("agentEngine='v2' + /chat save: falls through to V2 backend executeCommand", async () => {
    const exec = mock(() =>
      Promise.resolve({
        success: true,
        message: 'saved',
        data: undefined,
      })
    );
    const telemetry = mock(() => undefined);
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
        sendChatSlashCommandTelemetry: telemetry,
      } as any,
    });
    ctx.agentEngine = 'v2';
    await dispatch(CHAT_CMD, 'save', ctx);
    expect((exec as any).mock.calls.length).toBe(1);
    expect((exec as any).mock.calls[0][0]).toEqual({
      command: 'chat',
      args: { value: 'save' },
    });
    expect(telemetry).not.toHaveBeenCalled();
  });

  it("agentEngine='v2' + /chat new: emits frontend-owned command usage", async () => {
    const telemetry = mock(() => undefined);
    const newSession = mock(() =>
      Promise.resolve({ sessionId: 'v2-session-1' })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        newSession,
        sendChatSlashCommandTelemetry: telemetry,
      } as any,
    });
    ctx.agentEngine = 'v2';
    await dispatch(CHAT_CMD, 'new hello', ctx);
    expect(telemetry).toHaveBeenCalledWith({
      command: '/chat',
      subcommand: 'new',
      success: true,
    });
  });

  it("agentEngine='kas' + non-handler command: falls through to existing dispatch", async () => {
    const MODEL_CMD: KasCommand = {
      name: KasCommandName.Model,
      description: 'x',
      meta: { inputType: 'selection' },
    };
    const ctx = createMockCommandContext({
      kasCommands: [MODEL_CMD],
    });
    ctx.agentEngine = 'kas';
    await dispatch(MODEL_CMD, 'some-model', ctx);
    expect(ctx.kiro.executeCommand).toHaveBeenCalled();
  });

  it("agentEngine='kas' + local /help command: emits frontend-owned command usage", async () => {
    const telemetry = mock(() => undefined);
    const HELP_CMD: KasCommand = {
      name: KasCommandName.Help,
      description: 'x',
      meta: { inputType: 'panel' },
    };
    const ctx = createMockCommandContext({
      kasCommands: [HELP_CMD],
      kiro: {
        sendChatSlashCommandTelemetry: telemetry,
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(HELP_CMD, '', ctx);
    expect(ctx._spies.setShowHelpPanel).toHaveBeenCalled();
    expect(telemetry).toHaveBeenCalledWith({
      command: '/help',
      success: true,
    });
  });

  it("agentEngine='kas' + /prompts skill selection: emits selected category once", async () => {
    const telemetry = mock(() => undefined);
    const ctx = createMockCommandContext({
      kasCommands: [PROMPTS_CMD],
      skills: [
        {
          name: 'review',
          description: 'review skill',
          source: { kind: 'workspace' },
        },
      ],
      kiro: {
        sendChatSlashCommandTelemetry: telemetry,
      } as any,
    });
    ctx.agentEngine = 'kas';

    await dispatch(PROMPTS_CMD, 'skill:review', ctx);

    expect(ctx._spies.sendMessage).toHaveBeenCalledWith('/review');
    expect(telemetry).toHaveBeenCalledTimes(1);
    expect(telemetry).toHaveBeenCalledWith({
      command: '/skill',
      success: true,
    });
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
    // Always round-trips now; the cached breakdown is the fallback used
    // here because the (stub) show response carries no fresh breakdown.
    expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(1);
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
