/**
 * V2 (Rust) client-experience telemetry wiring (§H.4/§H.6). Asserts that
 * RustAcpClient emits the same client-experience metric set the KAS path does,
 * stamped `engine='v2'`, and that it does NOT emit the host-authoritative
 * economics (tokens/cost/context_usage) on V2. The observer module is mocked so
 * we can capture the record-fn calls without standing up the OTLP transport.
 */
import { afterAll, beforeEach, describe, expect, it, mock } from 'bun:test';
import type { ContentBlock } from '@agentclientprotocol/sdk';
import { createCipheriv } from 'node:crypto';
import { AgentEventType, type AgentStreamEvent } from '../types/agent-events';
import { TurnFailureReason } from '../types/generated/telemetry';

type RecordFnArgs = Record<string, unknown>;
const recordTuiSessionStarted = mock((_args: RecordFnArgs) => {});
const recordTuiUserTurn = mock((_args: RecordFnArgs) => {});
const setTelemetryUserId = mock((_userId: string | undefined) => {});
const textPrompt = (text: string): ContentBlock[] => [{ type: 'text', text }];
const telemetryIdentityKey = Buffer.alloc(32, 7);
const pseudonymousTelemetryUserId =
  'v1:WvTeO69_0uZMOLh0_HyQuoA87GQaiEZxqtJwK8EmlIg';

function encryptedIdentityParams(
  userId: string,
  aad = '_kiro.dev/telemetry/identityChanged'
) {
  const nonce = Buffer.alloc(12, 9);
  const cipher = createCipheriv('aes-256-gcm', telemetryIdentityKey, nonce);
  cipher.setAAD(Buffer.from(aad));
  const ciphertext = Buffer.concat([
    cipher.update(userId, 'utf8'),
    cipher.final(),
    cipher.getAuthTag(),
  ]);
  return {
    nonce: nonce.toString('base64url'),
    ciphertext: ciphertext.toString('base64url'),
  };
}

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../utils/tui-telemetry-observer',
  'child_process',
  'node:child_process',
  '@agentclientprotocol/sdk',
  '../utils/logger',
]);

mock.module('../utils/tui-telemetry-observer', () => ({
  DEFAULT_ENGINE: 'v3',
  TUI_SCOPE: 'kiro.tui',
  recordTuiWorkflowRestoreSummary: mock(() => {}),
  modeFromId: (id?: string) => (!id || id === 'default' ? 'interactive' : id),
  resultFromStatus: (status?: string) =>
    status === 'completed' ? 'success' : '_other_',
  turnFailureReasonFromStatus: () => undefined,
  recordTuiSessionStarted,
  recordTuiSlashCommand: mock(() => {}),
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
import * as realChildProcess from 'child_process';
// A module mock is process-wide, so the real exports are carried over rather
// than dropped: a suite loaded later that imports a different export would
// otherwise resolve against a module that no longer provides it.
mock.module('child_process', () => ({
  ...realChildProcess,
  spawn: mockSpawn,
}));
mock.module('node:child_process', () => ({
  ...realChildProcess,
  spawn: mockSpawn,
}));

let promptStopReason = 'end_turn';
let promptMeta: Record<string, unknown> | undefined;
let promptError: Error | undefined;
let promptPending = false;
let connectionAbortController = new AbortController();
const mockPrompt = mock(() =>
  promptError
    ? Promise.reject(promptError)
    : promptPending
      ? new Promise<never>(() => {})
      : Promise.resolve({ stopReason: promptStopReason, _meta: promptMeta })
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
  setTelemetryUserId.mockClear();
  promptStopReason = 'end_turn';
  promptMeta = undefined;
  promptError = undefined;
  promptPending = false;
  connectionAbortController = new AbortController();
});

describe('Rust ACP telemetry ownership', () => {
  it('applies only pseudonymous private identity notifications before returning', async () => {
    const client = new AcpClient(
      '/agent',
      [],
      '9.9.9-test',
      telemetryIdentityKey,
      setTelemetryUserId
    );

    await client.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      encryptedIdentityParams(pseudonymousTelemetryUserId)
    );

    expect(setTelemetryUserId).toHaveBeenCalledTimes(1);
    expect(setTelemetryUserId).toHaveBeenCalledWith(
      pseudonymousTelemetryUserId
    );
    expect(setTelemetryUserId).not.toHaveBeenCalledWith('private-user-id');
  });

  it('rejects malformed, unauthenticated, and wrong-key identity payloads', async () => {
    const client = new AcpClient(
      '/agent',
      [],
      '9.9.9-test',
      telemetryIdentityKey,
      setTelemetryUserId
    );
    const modifiedTag = encryptedIdentityParams(pseudonymousTelemetryUserId);
    const sealed = Buffer.from(modifiedTag.ciphertext, 'base64url');
    sealed[sealed.length - 1] = sealed[sealed.length - 1]! ^ 1;
    modifiedTag.ciphertext = sealed.toString('base64url');

    await client.extNotification('_kiro.dev/telemetry/identityChanged', {
      nonce: 'not-valid-base64url!',
      ciphertext: 'also-invalid!',
    });
    await client.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      modifiedTag
    );
    await client.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      encryptedIdentityParams(
        pseudonymousTelemetryUserId,
        '_kiro.dev/telemetry/differentMethod'
      )
    );

    const wrongKeyClient = new AcpClient(
      '/agent',
      [],
      '9.9.9-test',
      Buffer.alloc(32, 8),
      setTelemetryUserId
    );
    await wrongKeyClient.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      encryptedIdentityParams(pseudonymousTelemetryUserId)
    );

    expect(setTelemetryUserId).not.toHaveBeenCalled();
    client.close();
    wrongKeyClient.close();
  });

  it('applies clear and new-account notifications synchronously in wire order', async () => {
    const client = new AcpClient(
      '/agent',
      [],
      '9.9.9-test',
      telemetryIdentityKey,
      setTelemetryUserId
    );

    await client.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      encryptedIdentityParams('')
    );
    await client.extNotification(
      '_kiro.dev/telemetry/identityChanged',
      encryptedIdentityParams(pseudonymousTelemetryUserId)
    );

    expect(setTelemetryUserId.mock.calls.map((call) => call[0])).toEqual([
      undefined,
      pseudonymousTelemetryUserId,
    ]);
  });

  it('records one interactive V2 chat session at session creation', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    await client.prompt(textPrompt('hi'));

    expect(recordTuiSessionStarted).toHaveBeenCalledTimes(1);
    expect(recordTuiSessionStarted.mock.calls[0]?.[0]).toEqual({
      mode: 'interactive',
      version: '9.9.9-test',
      engine: 'v2',
      trustPosture: 'prompt_on_demand',
    });
  });

  it('records trust-all posture from the launched agent arguments', async () => {
    const client = new AcpClient('/agent', ['--trust-all-tools'], '9.9.9-test');
    await client.newSession();

    expect(recordTuiSessionStarted.mock.calls[0]?.[0]).toMatchObject({
      engine: 'v2',
      trustPosture: 'trust_all_tools',
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

  for (const failureReason of Object.values(TurnFailureReason)) {
    it(`records generated turn failure reason ${failureReason}`, async () => {
      const client = new AcpClient('/agent', [], '9.9.9-test');
      await client.newSession();
      recordTuiUserTurn.mockClear();
      promptMeta = { kiro: { turnFailureReason: failureReason } };

      await client.prompt(textPrompt('failed turn'));

      expect(recordTuiUserTurn).toHaveBeenCalledTimes(1);
      expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
        result: 'failed',
        failureReason,
        engine: 'v2',
      });
    });
  }

  it('keeps cancellation authoritative over failure metadata', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    promptStopReason = 'cancelled';
    promptMeta = { kiro: { turnFailureReason: 'model_error' } };

    await client.prompt(textPrompt('cancelled after refusal'));

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'cancelled',
      engine: 'v2',
    });
    expect(recordTuiUserTurn.mock.calls[0]?.[0]).not.toHaveProperty(
      'failureReason'
    );
  });

  it('ignores an unknown turn failure reason', async () => {
    const client = new AcpClient('/agent', [], '9.9.9-test');
    await client.newSession();
    recordTuiUserTurn.mockClear();
    promptMeta = { kiro: { turnFailureReason: 'future_reason' } };

    await client.prompt(textPrompt('future metadata'));

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'success',
      engine: 'v2',
    });
    expect(recordTuiUserTurn.mock.calls[0]?.[0]).not.toHaveProperty(
      'failureReason'
    );
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
    promptStopReason = 'refusal';
    await client.prompt(textPrompt('reject tool'));
    promptStopReason = 'max_turn_requests';
    await client.prompt(textPrompt('limit'));

    expect(recordTuiUserTurn.mock.calls[0]?.[0]).toMatchObject({
      result: 'cancelled',
    });
    expect(recordTuiUserTurn.mock.calls[1]?.[0]).toMatchObject({
      result: 'cancelled',
    });
    expect(recordTuiUserTurn.mock.calls[2]?.[0]).toMatchObject({
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
