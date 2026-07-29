/**
 * V2 (Rust) client-experience telemetry wiring (§H.4/§H.6). Asserts that
 * RustAcpClient emits the same client-experience metric set the KAS path does,
 * stamped `engine='v2'`, and that it does NOT emit the host-authoritative
 * economics (tokens/cost/context_usage) on V2. The observer module is mocked so
 * we can capture the record-fn calls without standing up the OTLP transport.
 */
import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import type { TuiToolCallStart } from '../utils/tui-telemetry-observer';
import { AgentEventType, type AgentStreamEvent } from '../types/agent-events';

type ToolFinishArgs = {
  outcome: 'success' | 'error' | 'cancelled' | 'denied';
  model: string;
};
type RecordFnArgs = Record<string, unknown>;

// --- Capture the observer record-fn calls ---
const recordTuiSessionStarted = mock((_a: RecordFnArgs) => {});
const recordTuiModeActive = mock((_a: RecordFnArgs) => {});
const recordTuiUserTurn = mock((_a: RecordFnArgs) => {});
const recordTuiModelInvocation = mock((_a: RecordFnArgs) => {});
const recordTuiTurnOutcome = mock((_a: RecordFnArgs) => {});
const recordTuiTokensConsumed = mock((_a: RecordFnArgs) => {});
const recordTuiContextUsage = mock((_a: RecordFnArgs) => {});

// TuiToolCallObserver is exercised for real (it just forwards to the record fns,
// which we replace below), but we need to capture the engine it is built with.
const observerEngines: Array<string | undefined> = [];
const toolStartCalls: Array<{ id: string; info: TuiToolCallStart }> = [];
const toolFinishCalls: Array<{ id: string; args: ToolFinishArgs }> = [];

mock.module('../utils/tui-telemetry-observer', () => ({
  DEFAULT_ENGINE: 'v3',
  TUI_SCOPE: 'kiro.tui',
  recordTuiSessionStarted,
  recordTuiModeActive,
  recordTuiUserTurn,
  recordTuiModelInvocation,
  recordTuiTurnOutcome,
  recordTuiTokensConsumed,
  recordTuiContextUsage,
  recordTuiCloudSession: mock(() => {}),
  // This suite only ever feeds 'default' (→ 'interactive'); the full
  // normalization is covered in tui-telemetry-observer.test.ts.
  modeFromId: (id?: string) => (!id || id === 'default' ? 'interactive' : id),
  resultFromStatus: (s?: string) => (s === 'completed' ? 'success' : '_other_'),
  TuiToolCallObserver: class {
    constructor(_deps: unknown, engine?: string) {
      observerEngines.push(engine);
    }
    start(id: string, info: TuiToolCallStart) {
      toolStartCalls.push({ id, info });
    }
    finish(id: string, args: ToolFinishArgs) {
      toolFinishCalls.push({ id, args });
    }
    reset() {}
  },
}));

// --- Mock child_process spawn ---
const makeStream = () => {
  const s: any = {
    on: mock(() => s),
    once: mock(() => s),
    write: mock(() => true),
    end: mock(() => {}),
    destroy: mock(() => {}),
    pipe: mock(() => s),
  };
  return s;
};
let mockProcess: any;
const mockSpawn = mock(() => {
  mockProcess = {
    stdin: makeStream(),
    stdout: makeStream(),
    stderr: makeStream(),
    kill: mock(() => {}),
    pid: 4242,
    on: mock(() => {}),
  };
  return mockProcess;
});
mock.module('child_process', () => ({ spawn: mockSpawn }));
mock.module('node:child_process', () => ({ spawn: mockSpawn }));

// --- Mock the ACP SDK connection ---
let promptStopReason = 'end_turn';
const mockPrompt = mock((_p: any) =>
  Promise.resolve({ stopReason: promptStopReason })
);
const mockNewSession = mock((_p: any) =>
  Promise.resolve({
    sessionId: 'v2-session-1',
    models: {
      currentModelId: 'claude-sonnet-4',
      availableModels: [{ modelId: 'claude-sonnet-4', name: 'Sonnet' }],
    },
    modes: {
      currentModeId: 'default',
      availableModes: [{ id: 'default' }],
    },
  })
);
class MockClientSideConnection {
  signal = { aborted: false, addEventListener: () => {} };
  initialize = mock(() => Promise.resolve({ protocolVersion: '1.0' }));
  newSession = mockNewSession;
  loadSession = mock(() =>
    Promise.resolve({ sessionId: 'v2-loaded', models: null, modes: null })
  );
  prompt = mockPrompt;
  cancel = mock(() => Promise.resolve());
  extMethod = mock(() => Promise.resolve({}));
  setSessionMode = mock(() => Promise.resolve());
  constructor(_f: any, _s: any) {}
}
mock.module('@agentclientprotocol/sdk', () => ({
  ndJsonStream: () => ({
    readable: new ReadableStream(),
    writable: new WritableStream(),
  }),
  ClientSideConnection: MockClientSideConnection,
  PROTOCOL_VERSION: '1.0',
}));
mock.module('../utils/logger', () => ({
  logger: { debug: () => {}, error: () => {}, warn: () => {}, info: () => {} },
}));

afterAll(() => mock.restore());

// @ts-expect-error — bun query-string import to bypass stale mocks
const { AcpClient } = await import('../acp-client?v2tel');

function clearAll() {
  recordTuiSessionStarted.mockClear();
  recordTuiModeActive.mockClear();
  recordTuiUserTurn.mockClear();
  recordTuiModelInvocation.mockClear();
  recordTuiTurnOutcome.mockClear();
  recordTuiTokensConsumed.mockClear();
  recordTuiContextUsage.mockClear();
  toolStartCalls.length = 0;
  toolFinishCalls.length = 0;
}

describe('RustAcpClient v2 telemetry wiring (§H.4/§H.6)', () => {
  beforeEach(() => {
    clearAll();
    promptStopReason = 'end_turn';
  });

  it('constructs its TuiToolCallObserver with engine="v2"', () => {
    new AcpClient('/agent', []);
    expect(observerEngines).toContain('v2');
  });

  it('newSession emits chat_session_started + mode_active stamped engine=v2', async () => {
    const c = new AcpClient('/agent', [], '9.9.9-test');
    await c.newSession();
    expect(recordTuiSessionStarted).toHaveBeenCalledTimes(1);
    // The raw Rust mode id 'default' is normalized to the catalog `mode` enum
    // member 'interactive' (modeFromId) before it is stamped on the metric.
    expect(recordTuiSessionStarted.mock.calls[0]![0]).toMatchObject({
      mode: 'interactive',
      version: '9.9.9-test',
      engine: 'v2',
    });
    expect(recordTuiModeActive).toHaveBeenCalledWith(
      expect.objectContaining({ mode: 'interactive', engine: 'v2' })
    );
  });

  it('session-started fires once per session id (dedup)', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    await c.prompt([{ type: 'text', text: 'hi' } as any]);
    await c.prompt([{ type: 'text', text: 'again' } as any]);
    // newSession emits start once; prompts must not re-emit it.
    expect(recordTuiSessionStarted).toHaveBeenCalledTimes(1);
  });

  it('prompt() end_turn emits user-turn(success) + model-invocation, NO turn-outcome', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    clearAll();
    await c.prompt([{ type: 'text', text: 'hi' } as any]);
    expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
    const turn = recordTuiUserTurn.mock.calls[0]![0];
    expect(turn).toMatchObject({
      engine: 'v2',
      result: 'success',
      isSubagent: false,
      model: 'claude-sonnet-4',
    });
    expect(typeof turn.durationSeconds).toBe('number');
    expect(recordTuiModelInvocation).toHaveBeenCalledWith(
      expect.objectContaining({ engine: 'v2', model: 'claude-sonnet-4' })
    );
    // end_turn is a success: the v2 path forwards status=undefined, which
    // recordTuiTurnOutcome suppresses (no counter emitted). recordTuiTurnOutcome
    // is mocked here, so we assert the forwarded status; the actual
    // success-suppression is locked in tui-telemetry-observer.test.ts.
    expect(recordTuiTurnOutcome).toHaveBeenCalledTimes(1);
    expect(recordTuiTurnOutcome.mock.calls[0]![0].status).toBeUndefined();
  });

  it('prompt() cancelled maps to result=cancelled + a turn-outcome', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    clearAll();
    promptStopReason = 'cancelled';
    await c.prompt([{ type: 'text', text: 'hi' } as any]);
    expect(recordTuiUserTurn.mock.calls[0]![0].result).toBe('cancelled');
    expect(recordTuiTurnOutcome.mock.calls[0]![0]).toMatchObject({
      engine: 'v2',
      status: 'cancelled',
    });
  });

  it('NEVER emits economics on v2 (tokens/context_usage) — §H.6', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    await c.prompt([{ type: 'text', text: 'hi' } as any]);
    expect(recordTuiTokensConsumed).not.toHaveBeenCalled();
    expect(recordTuiContextUsage).not.toHaveBeenCalled();
  });

  it('emits explicit context invalidation metadata as a cleared usage event', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    const events: AgentStreamEvent[] = [];
    c.onUpdate((event: AgentStreamEvent) => events.push(event));

    await c.extNotification?.('_kiro.dev/metadata', {
      sessionId: 'v2-session-1',
      contextUsageInvalidated: true,
    });

    expect(
      events.some(
        (event) =>
          event.type === AgentEventType.ContextUsage && event.percent === null
      )
    ).toBe(true);
  });

  it('main-session tool_call → observer.start, tool_call_update → observer.finish', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    clearAll();
    await c.sessionUpdate({
      sessionId: 'v2-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-1',
        title: 'fs_read',
        status: 'pending',
      },
    } as any);
    await c.sessionUpdate({
      sessionId: 'v2-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-1',
        status: 'completed',
      },
    } as any);
    expect(toolStartCalls).toHaveLength(1);
    expect(toolStartCalls[0]!.id).toBe('tc-1');
    expect(toolStartCalls[0]!.info.toolOrigin).toBe('builtin');
    expect(toolFinishCalls).toHaveLength(1);
    expect(toolFinishCalls[0]!.args.outcome).toBe('success');
  });

  it('main-session tool_call preserves host-stamped MCP server identity', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    clearAll();

    await c.sessionUpdate({
      sessionId: 'v2-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-mcp',
        title: 'query_db',
        status: 'pending',
        _meta: { kiro: { mcpServerName: 'local-server' } },
      },
    } as any);

    expect(toolStartCalls).toHaveLength(1);
    expect(toolStartCalls[0]!.info).toEqual({
      name: 'query_db',
      toolOrigin: 'mcp',
      mcpServerName: 'local-server',
    });
  });

  it('failed-before-exec synthesized MCP tool_call feeds observer.start before finish', async () => {
    const c = new AcpClient('/agent', []);
    await c.newSession();
    clearAll();

    await c.sessionUpdate({
      sessionId: 'v2-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-mcp-failed',
        title: 'query_db',
        status: 'failed',
        rawInput: { sql: 'select 1' },
        _meta: {
          kiro: { toolName: 'query_db', mcpServerName: 'local-server' },
        },
      },
    } as any);

    expect(toolStartCalls).toHaveLength(1);
    expect(toolStartCalls[0]!.info).toEqual({
      name: 'query_db',
      toolOrigin: 'mcp',
      mcpServerName: 'local-server',
    });
    expect(toolFinishCalls).toHaveLength(1);
    expect(toolFinishCalls[0]!.id).toBe('tc-mcp-failed');
    expect(toolFinishCalls[0]!.args.outcome).toBe('error');
  });
});
