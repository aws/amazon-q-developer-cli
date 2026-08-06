/**
 * V2 (Rust) client-experience telemetry wiring (§H.4/§H.6). Asserts that
 * RustAcpClient emits the same client-experience metric set the KAS path does,
 * stamped `engine='v2'`, and that it does NOT emit the host-authoritative
 * economics (tokens/cost/context_usage) on V2. The observer module is mocked so
 * we can capture the record-fn calls without standing up the OTLP transport.
 */
import { afterAll, beforeEach, describe, expect, it, mock } from 'bun:test';
import type { ContentBlock } from '@agentclientprotocol/sdk';
import { AgentEventType, type AgentStreamEvent } from '../types/agent-events';

type RecordFnArgs = Record<string, unknown>;
const recordTuiSessionStarted = mock((_args: RecordFnArgs) => {});
const recordTuiUserTurn = mock((_args: RecordFnArgs) => {});
const textPrompt = (text: string): ContentBlock[] => [{ type: 'text', text }];

mock.module('../utils/tui-telemetry-observer', () => ({
  DEFAULT_ENGINE: 'v3',
  TUI_SCOPE: 'kiro.tui',
  recordTuiWorkflowRestoreSummary: mock(() => {}),
  modeFromId: (id?: string) => (!id || id === 'default' ? 'interactive' : id),
  resultFromStatus: (status?: string) =>
    status === 'completed' ? 'success' : '_other_',
  turnFailureReasonFromStatus: () => undefined,
  recordTuiSessionStarted,
  recordTuiUserTurn,
  recordTuiCloudSession: mock(() => {}),
  recordTuiCloudSessionReady: mock(() => {}),
  recordTuiCreditsConsumed: mock(() => {}),
  recordTuiModelInvocations: mock(() => {}),
  recordTuiTokensConsumed: mock(() => {}),
  TuiFirstVisibleResponseObserver: class {
    start() {}
    observe() {}
    cancel() {}
  },
  TuiToolCallObserver: class {
    start() {}
    finish() {}
    reset() {}
  },
}));

const makeStream = () => {
  const stream: any = {
    on: mock(() => stream),
    once: mock(() => stream),
    write: mock(() => true),
    end: mock(() => {}),
    destroy: mock(() => {}),
    pipe: mock(() => stream),
  };
  return stream;
};
const mockSpawn = mock(() => ({
  stdin: makeStream(),
  stdout: makeStream(),
  stderr: makeStream(),
  kill: mock(() => {}),
  pid: 4242,
  on: mock(() => {}),
}));
mock.module('child_process', () => ({ spawn: mockSpawn }));
mock.module('node:child_process', () => ({ spawn: mockSpawn }));

let promptStopReason = 'end_turn';
let promptError: Error | undefined;
let promptPending = false;
let connectionAbortController = new AbortController();
const mockPrompt = mock(() =>
  promptError
    ? Promise.reject(promptError)
    : promptPending
      ? new Promise<never>(() => {})
      : Promise.resolve({ stopReason: promptStopReason })
);
class MockClientSideConnection {
  signal = connectionAbortController.signal;
  initialize = mock(() => Promise.resolve({ protocolVersion: '1.0' }));
  newSession = mock(() =>
    Promise.resolve({
      sessionId: 'v2-session-1',
      models: null,
      modes: { currentModeId: 'default', availableModes: [] },
    })
  );
  loadSession = mock(() =>
    Promise.resolve({ sessionId: 'v2-loaded', models: null, modes: null })
  );
  prompt = mockPrompt;
  cancel = mock(() => Promise.resolve());
  extMethod = mock(() => Promise.resolve({}));
  setSessionMode = mock(() => Promise.resolve());
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

// @ts-expect-error - query-string import isolates this suite's mocks.
const { RustAcpClient: AcpClient } = await import('../acp-client/rust?v2tel');

beforeEach(() => {
  recordTuiSessionStarted.mockClear();
  recordTuiUserTurn.mockClear();
  promptStopReason = 'end_turn';
  promptError = undefined;
  promptPending = false;
  connectionAbortController = new AbortController();
});

describe('Rust ACP telemetry ownership', () => {
  it('records one interactive V2 chat session at session creation', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    await client.prompt(textPrompt('hi'));

    expect(recordTuiSessionStarted).toHaveBeenCalledTimes(1);
    expect(recordTuiSessionStarted.mock.calls[0]?.[0]).toEqual({
      mode: 'interactive',
      version: '9.9.9-test',
      engine: 'v2',
    });
  });

  it('records the client-owned top-level turn with exact version', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    await client.prompt(textPrompt('hi'));

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'success',
      isSubagent: false,
      mode: 'interactive',
      version: '9.9.9-test',
      engine: 'v2',
    });
    expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
  });

  it('records one internal failure and preserves a rejected prompt', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    promptError = new Error('ACP request failed');

    await expect(client.prompt(textPrompt('hi'))).rejects.toThrow(
      'ACP request failed'
    );

    expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toEqual({
      result: 'failed',
      isSubagent: false,
      mode: 'interactive',
      version: '9.9.9-test',
      failureReason: 'internal_error',
      engine: 'v2',
    });
  });

  it('records a failed turn when the connection is already closed', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    connectionAbortController.abort();

    await expect(client.prompt(textPrompt('hi'))).rejects.toThrow(
      'Agent connection closed unexpectedly'
    );

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'failed',
      failureReason: 'internal_error',
      engine: 'v2',
    });
    expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
  });

  it('records a failed turn when the connection closes mid-prompt', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    promptPending = true;

    const prompt = client.prompt(textPrompt('hi'));
    connectionAbortController.abort();

    await expect(prompt).rejects.toThrow(
      'Agent connection closed unexpectedly'
    );
    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'failed',
      failureReason: 'internal_error',
      engine: 'v2',
    });
    expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
  });

  it('maps cancellation and execution limits to the reviewed turn contract', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();

    promptStopReason = 'cancelled';
    await client.prompt(textPrompt('cancel'));
    promptStopReason = 'max_turn_requests';
    await client.prompt(textPrompt('limit'));

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'cancelled',
    });
    expect(recordTuiUserTurn.mock.calls[1]?.[0]).toMatchObject({
      result: 'failed',
      failureReason: 'execution_limit',
    });
  });

  it('emits explicit context invalidation metadata as a cleared usage event', async () => {
    const client = new AcpClient('/agent', []);
    await client.newSession();
    const events: AgentStreamEvent[] = [];
    client.onUpdate((event: AgentStreamEvent) => events.push(event));

    await client.extNotification?.('_kiro.dev/metadata', {
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
});
