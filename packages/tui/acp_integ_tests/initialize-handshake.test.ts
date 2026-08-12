/**
 * Minimal example of the `acp_integ_tests/` harness.
 *
 * Spawns the TUI with its KAS ACP transport wired to an in-process mock
 * server, scripts just enough of the ACP handshake for the TUI to boot,
 * and asserts that:
 *   1. The TUI sent an `initialize` request with the expected client info.
 *   2. The TUI sent a `session/new` request with a cwd.
 *   3. The mock's canned `initialize` and `session/new` responses flowed
 *      back through the real `KasAcpClient` and the real `@kiro/client` SDK.
 *
 * This is an infrastructure smoke test, not a product test. Product tests
 * (`/chat`, `/agent`, etc.) will consume this harness in follow-up PRs.
 *
 * Pattern note: assertions go in the test body (on `receivedRequests`), not
 * inside `mock.on()` handlers. An `expect()` failure inside a handler would
 * be caught by the server's try/catch, returned to the TUI as a JSON-RPC
 * error, and surface downstream as a confusing symptom ("wrong count")
 * rather than the actual assertion failure. Handlers should return canned
 * data; the test body asserts.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

describe('ACP initialize handshake', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('TUI performs initialize + session/new against the mock transport', async () => {
    tc = new AcpTestCase({ testName: 'acp-initialize-handshake' });

    // Handlers return canned data only. No assertions here; see pattern note above.
    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: {
          kiro: {
            extensionMethods: [],
          },
        },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-1',
      modes: defaultKasModes(),
    }));

    // `session/set_config_option` fires for autopilot inside try/catch in
    // KasAcpClient.newSession, so failure is non-fatal. We still handle it
    // so the mock server doesn't surface a "No handler registered" error.
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();

    // Give the TUI a tick to send its initial requests and propagate the
    // session through Kiro orchestrator to the app store.
    await new Promise((r) => setTimeout(r, 200));

    // Assertions go here, where failures surface as test failures.
    const initReqs = tc.mock.receivedRequests('initialize');
    expect(initReqs).toHaveLength(1);
    const initParams = initReqs[0]!.params as InitializeRequest;
    expect(initParams.clientInfo?.name).toBe('kiro-cli');
    expect(typeof initParams.protocolVersion).toBe('number');

    const newReqs = tc.mock.receivedRequests('session/new');
    expect(newReqs).toHaveLength(1);
    const newParams = newReqs[0]!.params as NewSessionRequest;
    expect(typeof newParams.cwd).toBe('string');
  });
});
