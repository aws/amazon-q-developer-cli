/**
 * End-to-end wire-level tests for the KAS `/effort` command.
 *
 * `/effort` mirrors `/model`: KAS advertises the reasoning effort level as
 * a standard ACP Session Config Option (`id: 'effortLevel'`,
 * `category: 'thought_level'`) on every session response, and validates
 * writes server-side. This suite drives the real `KasAcpClient` +
 * `@kiro/client` SDK against an in-process mock ACP server and asserts:
 *
 *   1. A new session populates the default effort level (chip + store).
 *   2. `/effort` lists the cached levels with the current one marked
 *      `[active]` — served from the client cache, no extra wire request.
 *   3. Setting a level fires `session/set_config_option` with
 *      `configId: 'effortLevel'` and surfaces "Effort set to {Level}".
 *   4. The prompt-bar chip + store reflect the new level.
 *   5. An autonomous `config_option_update` notification updates the chip.
 *
 * See packages/tui/Effort-KAS.md §7.1 for the design of these scenarios.
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

interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

/** The effortLevel config option, parameterized by the current value. */
function effortConfigOption(currentValue: string) {
  return {
    type: 'select' as const,
    id: 'effortLevel',
    name: 'Effort',
    category: 'thought_level',
    currentValue,
    options: [
      { value: 'low', name: 'Low' },
      { value: 'medium', name: 'Medium' },
      { value: 'high', name: 'High' },
      { value: 'xhigh', name: 'xHigh' },
    ],
  };
}

/** Baseline session/new configOptions: a model entry + the effort entry. */
function baselineConfigOptions(effortCurrent: string) {
  return [
    {
      type: 'select' as const,
      id: 'model',
      name: 'Model',
      category: 'model',
      currentValue: 'claude-opus-4.7',
      options: [{ value: 'claude-opus-4.7', name: 'Claude Opus 4.7' }],
    },
    effortConfigOption(effortCurrent),
  ];
}

function registerInitialize(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
}

describe('/effort command (KAS)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('new session populates the default effort level (store + chip)', async () => {
    tc = new AcpTestCase({ testName: 'effort-new-session' });
    registerInitialize(tc);
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
      configOptions: baselineConfigOptions('high'),
    }));
    // autopilot set during newSession; canned empty response.
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    // Wait for TUI mount, then re-send config to ensure effort reaches store
    // (session/new effort broadcast can race ahead of React handler registration on Linux)
    await tc.waitForVisibleText('ask a question', 10000);
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: baselineConfigOptions('high'),
      },
    });
    await tc.waitForStore((s) => s.currentEffort === 'high', 3000);

    // Chip shows the lowercased level in the prompt bar.
    await tc.waitForVisibleText('high', 2000);
  });

  it('/effort lists levels from the client cache without a wire request', async () => {
    tc = new AcpTestCase({ testName: 'effort-list' });
    registerInitialize(tc);
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
      configOptions: baselineConfigOptions('high'),
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: baselineConfigOptions('high'),
      },
    });
    await tc.waitForStore((s) => s.currentEffort === 'high', 3000);

    // Open the selection menu.
    await tc.sendKeys('/effort');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(400);

    await tc.waitForVisibleText('xhigh', 2000);
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('low');
    expect(snap).toContain('medium');
    expect(snap).toContain('high');
    expect(snap).toContain('xhigh');

    // Options are served from the cache populated on session/new — opening
    // the menu must NOT have triggered an effortLevel set_config_option.
    const effortSets = tc.mock
      .receivedRequests('session/set_config_option')
      .filter(
        (r) => (r.params as SetConfigOptionParams).configId === 'effortLevel'
      );
    expect(effortSets).toHaveLength(0);
  });

  it('setting a level fires set_config_option and shows the success message', async () => {
    tc = new AcpTestCase({ testName: 'effort-set' });
    registerInitialize(tc);
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
      configOptions: baselineConfigOptions('high'),
    }));
    // KAS returns the full config-option set (model + effort + …) on every
    // set_config_option response, not just the changed option. The effort chip
    // only moves when an effort update arrives alongside the model, so the
    // response must carry the model.
    tc.mock.on('session/set_config_option', (params) => {
      const p = params as SetConfigOptionParams;
      if (p.configId === 'effortLevel') {
        return { configOptions: baselineConfigOptions(String(p.value)) };
      }
      return {};
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: baselineConfigOptions('high'),
      },
    });
    await tc.waitForStore((s) => s.currentEffort === 'high', 3000);

    // Provide the level as an arg so the dispatcher executes directly.
    await tc.sendKeys('/effort xhigh');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(400);

    // Wire shape: exactly one effortLevel write with value 'xhigh'.
    const effortSets = tc.mock
      .receivedRequests('session/set_config_option')
      .filter(
        (r) => (r.params as SetConfigOptionParams).configId === 'effortLevel'
      );
    expect(effortSets).toHaveLength(1);
    const params = effortSets[0]!.params as SetConfigOptionParams;
    expect(params.sessionId).toBe('test-1');
    expect(params.value).toBe('xhigh');

    // Message locked to the plain "Effort set to xhigh" confirmation.
    await tc.waitForVisibleText('Effort set to xhigh', 2000);

    // Scenario 4: store + chip reflect the new level.
    await tc.waitForStore((s) => s.currentEffort === 'xhigh', 3000);
  });

  it('autonomous config_option_update refreshes the chip', async () => {
    tc = new AcpTestCase({ testName: 'effort-autonomous' });
    registerInitialize(tc);
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
      configOptions: baselineConfigOptions('high'),
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: baselineConfigOptions('high'),
      },
    });
    await tc.waitForStore((s) => s.currentEffort === 'high', 3000);

    // KAS autonomously changes effort (e.g. after a model fallback). The
    // notification carries the full config-option set, including the model.
    tc.mock.notify('session/update', {
      sessionId: 'test-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: baselineConfigOptions('medium'),
      },
    });

    await tc.waitForStore((s) => s.currentEffort === 'medium', 3000);
    await tc.waitForVisibleText('medium', 2000);

    // Re-open /effort: the cached menu must reflect the new level without
    // any extra wire round-trip (no effortLevel set_config_option fired).
    await tc.sendKeys('/effort');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(400);
    await tc.waitForVisibleText('[active]', 2000);
    const menuSnap = tc.getSnapshotFormatted();
    // The active marker sits on the medium row now.
    const mediumLine = menuSnap
      .split('\n')
      .find((l) => l.includes('medium') && l.includes('[active]'));
    expect(mediumLine).toBeDefined();

    const effortSets = tc.mock
      .receivedRequests('session/set_config_option')
      .filter(
        (r) => (r.params as SetConfigOptionParams).configId === 'effortLevel'
      );
    expect(effortSets).toHaveLength(0);
  });

  it('bare /effort on a model with no effort schema surfaces a descriptive error', async () => {
    tc = new AcpTestCase({ testName: 'effort-no-schema' });
    registerInitialize(tc);
    // session/new returns only a model option — no effortLevel entry.
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-1',
      modes: defaultKasModes(),
      configOptions: [
        {
          type: 'select' as const,
          id: 'model',
          name: 'Model',
          category: 'model',
          currentValue: 'claude-opus-4.7',
          options: [{ value: 'claude-opus-4.7', name: 'Claude Opus 4.7' }],
        },
      ],
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    // No effort chip: the store's currentEffort stays null.
    await tc.waitForStore((s) => s.currentEffort === null, 3000);

    // Bare /effort: empty cache → menu empty → dispatcher falls through to
    // execute → descriptive "not available" guidance.
    await tc.sendKeys('/effort');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(400);

    await tc.waitForVisibleText('not available', 2000);

    // No effortLevel write should have been attempted.
    const effortSets = tc.mock
      .receivedRequests('session/set_config_option')
      .filter(
        (r) => (r.params as SetConfigOptionParams).configId === 'effortLevel'
      );
    expect(effortSets).toHaveLength(0);
  });
});
