/**
 * Wire-level contract for `--agent <name>` under the KAS engine.
 *
 * `--agent` is applied at session creation: the TUI sends the agent as
 * `_meta.kiro.modeId` on `session/new`, NOT as a post-creation
 * `session/set_config_option({ configId: 'mode' })` round-trip.
 *
 * Creating the session directly in the requested mode is what lets KAS
 * resolve that agent's configured `model` (KAS only applies a profile's
 * `model` at session-creation time, never on a mid-session mode switch) and
 * return it in the `session/new` `configOptions`. The model chip therefore
 * reflects the agent's model instead of the backend default.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../src/constants/agents';

interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

/** Shape of the `_meta.kiro` envelope the TUI attaches to session/new. */
interface NewSessionKiroMeta {
  _meta?: { kiro?: { modeId?: string } };
}

const MODEL_OPTIONS = [
  { value: 'auto', name: 'Auto' },
  { value: 'claude-opus-4', name: 'Claude Opus 4' },
];

function modelConfigOption(currentValue: string) {
  return {
    type: 'select' as const,
    id: 'model',
    name: 'Model',
    category: 'model',
    currentValue,
    options: MODEL_OPTIONS,
  };
}

function onInitialize(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
}

function modeIdOf(req: { params: unknown }): string | undefined {
  return (req.params as NewSessionKiroMeta)._meta?.kiro?.modeId;
}

describe('--agent CLI flag → KAS session/new modeId', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('sends --agent as _meta.kiro.modeId on session/new (no mode round-trip)', async () => {
    /**
     * GIVEN  the CLI is launched with `--agent kiro_planner`
     * WHEN   the TUI creates the session
     * THEN   session/new carries `_meta.kiro.modeId === 'plan'` and no
     *        separate session/set_config_option({configId:'mode'}) is sent
     */
    tc = new AcpTestCase({
      testName: 'acp-agent-flag',
      args: ['--agent', 'kiro_planner'],
    });
    onInitialize(tc);

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-1',
      modes: {
        currentModeId: 'plan',
        availableModes: [
          { id: KAS_DEFAULT_AGENT_ID, name: KAS_DEFAULT_AGENT_NAME },
          { id: 'plan', name: 'Planner' },
        ],
      },
      configOptions: [modelConfigOption('auto')],
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.isInitialized === true, 8000);

    const newReqs = tc.mock.receivedRequests('session/new');
    expect(newReqs).toHaveLength(1);
    expect(modeIdOf(newReqs[0]!)).toBe('plan');

    const modeReqs = tc.mock
      .receivedRequests('session/set_config_option')
      .filter((r) => (r.params as SetConfigOptionParams).configId === 'mode');
    expect(modeReqs).toHaveLength(0);
  }, 15000);

  it('omits modeId on session/new when --agent is absent', async () => {
    /**
     * GIVEN  no `--agent` flag (and KIRO_MODE unset)
     * WHEN   the TUI creates the session
     * THEN   session/new carries no `_meta.kiro.modeId`
     */
    // Sandbox settings so an ambient chat.defaultAgent in the runner's real
    // ~/.kiro/settings/cli.json can't make the TUI send a modeId on
    // session/new and break the "absent" assertion.
    tc = new AcpTestCase({ testName: 'acp-agent-flag-absent', settings: {} });
    onInitialize(tc);

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-2',
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          { id: KAS_DEFAULT_AGENT_ID, name: KAS_DEFAULT_AGENT_NAME },
        ],
      },
      configOptions: [modelConfigOption('auto')],
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.isInitialized === true, 8000);

    const newReqs = tc.mock.receivedRequests('session/new');
    expect(newReqs).toHaveLength(1);
    expect(modeIdOf(newReqs[0]!)).toBeUndefined();
  }, 15000);

  it('reads the agent-resolved model from session/new configOptions exactly', async () => {
    /**
     * GIVEN  `--agent kiro_planner`, and KAS resolves that agent's
     *        configured model only when the session is created in that mode
     * WHEN   session/new is created with `_meta.kiro.modeId === 'plan'`
     * THEN   KAS returns the agent's model as the model config option's
     *        currentValue, and the chip reads exactly that model
     */
    tc = new AcpTestCase({
      testName: 'acp-agent-flag-model',
      args: ['--agent', 'kiro_planner'],
    });
    onInitialize(tc);

    // Mirror KAS: a profile's `model` is applied at session creation. With the
    // mode set on session/new, the agent's model surfaces as currentValue;
    // without it (the old post-creation switch), only the default resolves.
    tc.mock.on<NewSessionRequest, NewSessionResponse>(
      'session/new',
      (params) => {
        const modeId = (params as NewSessionKiroMeta)._meta?.kiro?.modeId;
        const currentModel = modeId === 'plan' ? 'claude-opus-4' : 'auto';
        return {
          sessionId: 'test-session-3',
          modes: {
            currentModeId: modeId ?? KAS_DEFAULT_AGENT_ID,
            availableModes: [
              { id: KAS_DEFAULT_AGENT_ID, name: KAS_DEFAULT_AGENT_NAME },
              { id: 'plan', name: 'Planner' },
            ],
          },
          configOptions: [modelConfigOption(currentModel)],
        };
      }
    );
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();

    const store = await tc.waitForStore(
      (s) => s.currentModel?.id === 'claude-opus-4',
      8000
    );
    expect(store.currentModel).toEqual({
      id: 'claude-opus-4',
      name: 'Claude Opus 4',
    });
  }, 15000);
});
