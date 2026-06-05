/**
 * Verifies that passing `--agent <name>` on the CLI causes the TUI to send
 * a `session/set_config_option` request with `configId: 'mode'` after
 * `session/new`.
 *
 * This is the wire-level contract that makes `--agent` actually take
 * effect under the KAS engine: KAS has no equivalent CLI flag, so the
 * TUI must apply the agent via the ACP `setSessionConfigOption('mode')`
 * call once the session exists. Without this test, a regression in
 * `KasAcpClient.newSession` plumbing would silently drop the flag.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

describe('--agent CLI flag → KAS setSessionConfigOption(mode)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('forwards --agent value as the mode config option on new session', async () => {
    tc = new AcpTestCase({
      testName: 'acp-agent-flag',
      args: ['--agent', 'kiro_planner'],
    });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-1',
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          { id: 'vibe', name: 'Default' },
          { id: 'kiro_planner', name: 'Planner' },
        ],
      },
    }));

    // Both autopilot and mode flow through this method; canned empty response.
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();

    // Allow the TUI to complete its initialize → session/new → set_config_option chain.
    await new Promise((r) => setTimeout(r, 300));

    const setConfigReqs = tc.mock.receivedRequests('session/set_config_option');
    const modeReqs = setConfigReqs.filter(
      (r) => (r.params as SetConfigOptionParams).configId === 'mode'
    );
    expect(modeReqs).toHaveLength(1);
    const params = modeReqs[0]!.params as SetConfigOptionParams;
    expect(params.sessionId).toBe('test-session-1');
    expect(params.value).toBe('quick-plan');
  });

  it('does not send mode config option when --agent is absent', async () => {
    tc = new AcpTestCase({ testName: 'acp-agent-flag-absent' });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-2',
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Default' }],
      },
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();

    await new Promise((r) => setTimeout(r, 300));

    const setConfigReqs = tc.mock.receivedRequests('session/set_config_option');
    const modeReqs = setConfigReqs.filter(
      (r) => (r.params as SetConfigOptionParams).configId === 'mode'
    );
    expect(modeReqs).toHaveLength(0);
  });
});
