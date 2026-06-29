/**
 * End-to-end wire-level tests for the KAS sticky model/effort defaults
 * (`chat.defaultModel` + `chat.modelDefaults`, opt-out via
 * `chat.disableAutoDefaultModel` / `chat.disableAutoDefaultEffort`).
 *
 * These exercise the SAME paths a real KAS server drives, which is exactly
 * what the unit-level mocks could not: the v3/KAS contract is that a
 * client-initiated `session/set_config_option('model')` (a) carries the
 * current effort across the switch when it is still valid for the new model
 * and (b) returns the updated `configOptions` ONLY in the RESPONSE — it never
 * pushes a separate `config_option_update` notification for that change. A
 * prior unit test got a FALSE POSITIVE on "effort restored on /model switch"
 * because it manually fired a `config_option_update` that real KAS does not
 * send; the apply was actually being driven by that synthetic notification,
 * not by the swap itself. The stateful mock here models the no-push behavior
 * faithfully (see {@link installStatefulKas}) so the only thing that can
 * restore a model's saved per-model effort is `executeModelSwap`'s own
 * `maybeApplySavedEffortDefault({ fromModelSwitch: true })` call.
 *
 * Scenarios:
 *   1. Effort RESTORED on model switch-back: A(saved!=native) -> B -> A shows
 *      A's saved effort, not the carried value. Also: switching to a model
 *      with NO saved default forces nothing (keeps KAS's carried effort).
 *   2. PERSIST + REAPPLY across processes: set /model + /effort in one session,
 *      assert cli.json, then a fresh process on the same KIRO_HOME applies the
 *      saved model + per-model effort on startup.
 *   3. OPT-OUT: `chat.disableAutoDefaultEffort` suppresses the WRITE on /effort
 *      but a pre-existing saved default is STILL applied (write-suppressed,
 *      apply-not-suppressed). Same for `chat.disableAutoDefaultModel`.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

// ── Fixtures ────────────────────────────────────────────────────────────────

interface SelectEntry {
  value: string;
  name: string;
}
interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

/** Full effort ladder; shared across all models so a carried value is always valid. */
const LEVELS: SelectEntry[] = [
  { value: 'low', name: 'Low' },
  { value: 'medium', name: 'Medium' },
  { value: 'high', name: 'High' },
  { value: 'xhigh', name: 'xHigh' },
];

/**
 * Build the `category: 'model'` config option. Each model advertises
 * `_meta.kiro.effortSchemaPath: 'output_config'` (KAS's authoritative path for
 * Claude-family models), so a persisted effort default lands at
 * `output_config.effort` — matching the cli.json shape these tests assert.
 */
function modelConfigOption(currentValue: string, models: SelectEntry[]) {
  return {
    type: 'select' as const,
    id: 'model',
    name: 'Model',
    category: 'model',
    currentValue,
    options: models.map((m) => ({
      value: m.value,
      name: m.name,
      _meta: { kiro: { effortSchemaPath: 'output_config' } },
    })),
  };
}

function effortConfigOption(currentValue: string) {
  return {
    type: 'select' as const,
    id: 'effortLevel',
    name: 'Effort',
    category: 'thought_level',
    currentValue,
    options: LEVELS,
  };
}

interface KasState {
  model: string;
  effort: string;
}

/**
 * Install a stateful mock KAS that faithfully models the v3 contract for
 * client-initiated config changes:
 *
 *   - `session/set_config_option('model')` updates the server's model and
 *     CARRIES the current effort over (it is always valid here since every
 *     model shares {@link LEVELS}, mirroring how real KAS keeps a still-valid
 *     carried effort). It returns the updated `configOptions` in the RESPONSE
 *     and NEVER pushes a `config_option_update` notification.
 *   - `session/set_config_option('effortLevel')` records the level when it is a
 *     known one, and likewise replies with the full state, no push.
 *   - `autopilot` (and anything else) is acked with an empty body.
 *
 * Returns the live state plus a `configOptions()` builder so a test can, when
 * it needs to defeat the documented session-start broadcast race, re-sync the
 * store with a `config_option_update` carrying the mock's CURRENT state (the
 * value the product's own logic produced, never a value the test invented).
 */
function installStatefulKas(
  tc: AcpTestCase,
  opts: {
    sessionId: string;
    models: SelectEntry[];
    initialModel: string;
    initialEffort: string;
  }
): { state: KasState; configOptions: () => unknown[] } {
  const levelValues = new Set(LEVELS.map((l) => l.value));
  const state: KasState = {
    model: opts.initialModel,
    effort: opts.initialEffort,
  };
  const configOptions = () => [
    modelConfigOption(state.model, opts.models),
    effortConfigOption(state.effort),
  ];

  tc.mock.on('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on('session/new', () => ({
    sessionId: opts.sessionId,
    modes: defaultKasModes(),
    configOptions: configOptions(),
  }));
  tc.mock.on('session/set_config_option', (params) => {
    const p = params as SetConfigOptionParams;
    if (p.configId === 'model' && typeof p.value === 'string') {
      state.model = p.value;
      // Carry the current effort over the switch (always valid here). NO
      // config_option_update is pushed — the response is the only signal.
      return { configOptions: configOptions() };
    }
    if (
      p.configId === 'effortLevel' &&
      typeof p.value === 'string' &&
      levelValues.has(p.value)
    ) {
      state.effort = p.value;
      return { configOptions: configOptions() };
    }
    // autopilot + unknown writes: ack without state.
    return {};
  });

  return { state, configOptions };
}

// ── KIRO_HOME / cli.json helpers ─────────────────────────────────────────────
//
// AcpTestCase always sandboxes KIRO_HOME, but it picks a private random dir and
// exposes no path, so a test cannot read the persisted cli.json or reuse it for
// a second process. We instead create our own KIRO_HOME, seed cli.json there,
// and override via `extraEnv.KIRO_HOME` (which wins over the harness sandbox —
// TestCase spreads `extraEnv` last). This is the same on-disk shape the harness
// `settings` option produces; it just hands us the path for read/reuse.

const createdHomes: string[] = [];

function makeKiroHome(seed?: Record<string, unknown>): string {
  const home = mkdtempSync(join(tmpdir(), 'kiro-sticky-'));
  createdHomes.push(home);
  if (seed) {
    const p = join(home, 'settings', 'cli.json');
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, JSON.stringify(seed, null, 2), 'utf-8');
  }
  return home;
}

function readCliJson(home: string): Record<string, unknown> {
  const p = join(home, 'settings', 'cli.json');
  if (!existsSync(p)) return {};
  return JSON.parse(readFileSync(p, 'utf-8')) as Record<string, unknown>;
}

/** Read a saved per-model effort default out of a parsed cli.json. */
function savedEffort(cli: Record<string, unknown>, modelId: string): unknown {
  const defaults = cli['chat.modelDefaults'] as Record<string, any> | undefined;
  return defaults?.[modelId]?.output_config?.effort;
}

async function waitForCliJson(
  home: string,
  predicate: (cli: Record<string, unknown>) => boolean,
  timeoutMs = 6000
): Promise<Record<string, unknown>> {
  const start = Date.now();
  let last: Record<string, unknown> = {};
  while (Date.now() - start < timeoutMs) {
    last = readCliJson(home);
    if (predicate(last)) return last;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(
    `cli.json did not satisfy predicate within ${timeoutMs}ms; last=${JSON.stringify(last)}`
  );
}

/** All effortLevel values written so far, in order. */
function effortSetValues(tc: AcpTestCase): unknown[] {
  return tc.mock
    .receivedRequests('session/set_config_option')
    .filter(
      (r) => (r.params as SetConfigOptionParams).configId === 'effortLevel'
    )
    .map((r) => (r.params as SetConfigOptionParams).value);
}

/** Poll until a `set_config_option` with the given configId+value is observed. */
async function waitForConfigSet(
  tc: AcpTestCase,
  configId: string,
  value: string,
  timeoutMs = 6000
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const hit = tc.mock
      .receivedRequests('session/set_config_option')
      .some((r) => {
        const p = r.params as SetConfigOptionParams;
        return p.configId === configId && p.value === value;
      });
    if (hit) return;
    await tc.sleepMs(100);
  }
  throw new Error(
    `no set_config_option ${configId}=${value} within ${timeoutMs}ms`
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────

describe('sticky model/effort defaults (KAS)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    for (const home of createdHomes.splice(0)) {
      try {
        rmSync(home, { recursive: true, force: true });
      } catch {
        /* best-effort */
      }
    }
  });

  it('restores a model\u2019s saved effort on switch-back, and forces nothing for a model with no saved default', async () => {
    // A's saved effort (xhigh) == its native here; B's saved (medium) differs
    // from the value carried in from A (xhigh); C has no saved default at all.
    const A = 'claude-opus-4.8';
    const B = 'claude-opus-4.7';
    const C = 'claude-sonnet-4';
    const home = makeKiroHome({
      'chat.modelDefaults': {
        [A]: { output_config: { effort: 'xhigh' } },
        [B]: { output_config: { effort: 'medium' } },
      },
    });
    tc = new AcpTestCase({
      testName: 'sticky-switch-back',
      extraEnv: { KIRO_HOME: home },
    });
    const kas = installStatefulKas(tc, {
      sessionId: 'sticky-session-1',
      models: [
        { value: A, name: 'Claude Opus 4.8' },
        { value: B, name: 'Claude Opus 4.7' },
        { value: C, name: 'Claude Sonnet 4' },
      ],
      initialModel: A,
      initialEffort: 'xhigh',
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Defeat the session-start broadcast race: re-sync the store from the
    // mock's current state (A @ xhigh). The guard was already armed on startup
    // (A's saved == native), so this is a no-op for the apply path.
    tc.mock.notify('session/update', {
      sessionId: 'sticky-session-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: kas.configOptions(),
      },
    });
    await tc.waitForStore(
      (s) => s.currentModel?.id === A && s.currentEffort === 'xhigh',
      5000
    );
    // No effortLevel write yet: A starts at its saved level.
    expect(effortSetValues(tc)).toEqual([]);

    // A -> B. KAS carries xhigh; the swap itself applies B's saved 'medium'.
    await tc.sendKeys(`/model ${B}`);
    await tc.pressEnter();
    await tc.waitForStore(
      (s) => s.currentModel?.id === B && s.currentEffort === 'medium',
      6000
    );
    await waitForConfigSet(tc, 'effortLevel', 'medium');
    expect(effortSetValues(tc)).toEqual(['medium']);

    // B -> A (switch BACK). KAS carries 'medium' in, but the swap restores A's
    // saved 'xhigh' — the exact behavior the false-positive unit test missed.
    await tc.sendKeys(`/model ${A}`);
    await tc.pressEnter();
    await tc.waitForStore(
      (s) => s.currentModel?.id === A && s.currentEffort === 'xhigh',
      6000
    );
    await waitForConfigSet(tc, 'effortLevel', 'xhigh');
    expect(effortSetValues(tc)).toEqual(['medium', 'xhigh']);

    // A -> C (no saved default). KAS carries 'xhigh'; the swap must force
    // nothing — no new effortLevel write, effort stays at the carried value.
    const beforeC = effortSetValues(tc).length;
    await tc.sendKeys(`/model ${C}`);
    await tc.pressEnter();
    await tc.waitForStore(
      (s) => s.currentModel?.id === C && s.currentEffort === 'xhigh',
      6000
    );
    // Give any (erroneous) apply a chance to fire before asserting absence.
    await tc.sleepMs(400);
    expect(effortSetValues(tc).length).toBe(beforeC);
    expect(kas.state.effort).toBe('xhigh');
  });

  it('persists /model + /effort to cli.json and reapplies them on a fresh session', async () => {
    const A = 'claude-opus-4.8';
    const B = 'claude-opus-4.7';
    const home = makeKiroHome(); // empty: stickiness on, nothing saved yet

    // ── Session 1: set a model + effort, assert they land in cli.json. ──
    tc = new AcpTestCase({
      testName: 'sticky-persist-write',
      extraEnv: { KIRO_HOME: home },
    });
    installStatefulKas(tc, {
      sessionId: 'sticky-session-1',
      models: [
        { value: A, name: 'Claude Opus 4.8' },
        { value: B, name: 'Claude Opus 4.7' },
      ],
      initialModel: A,
      initialEffort: 'high',
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys(`/model ${B}`);
    await tc.pressEnter();
    await tc.waitForStore((s) => s.currentModel?.id === B, 6000);

    await tc.sendKeys('/effort low');
    await tc.pressEnter();
    await tc.waitForStore((s) => s.currentEffort === 'low', 6000);

    const cli1 = await waitForCliJson(
      home,
      (c) => c['chat.defaultModel'] === B && savedEffort(c, B) === 'low'
    );
    expect(cli1['chat.defaultModel']).toBe(B);
    expect(savedEffort(cli1, B)).toBe('low');

    await tc.cleanup();
    tc = null;

    // ── Session 2: fresh process, SAME KIRO_HOME. Startup must apply the
    // saved model (B) and B's saved per-model effort (low). The mock's
    // session/new still reports the server default A @ high; the product's
    // startup logic is what drives the switch to B and the effort restore. ──
    tc = new AcpTestCase({
      testName: 'sticky-persist-reapply',
      extraEnv: { KIRO_HOME: home },
    });
    const kas2 = installStatefulKas(tc, {
      sessionId: 'sticky-session-2',
      models: [
        { value: A, name: 'Claude Opus 4.8' },
        { value: B, name: 'Claude Opus 4.7' },
      ],
      initialModel: A,
      initialEffort: 'high',
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Non-vacuous proof: the product issued both writes on startup off the
    // saved settings — the saved model AND the saved per-model effort.
    await waitForConfigSet(tc, 'model', B);
    await waitForConfigSet(tc, 'effortLevel', 'low');

    // Re-sync the store from the mock's resulting state and confirm the chip.
    tc.mock.notify('session/update', {
      sessionId: 'sticky-session-2',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: kas2.configOptions(),
      },
    });
    await tc.waitForStore(
      (s) => s.currentModel?.id === B && s.currentEffort === 'low',
      6000
    );
    expect(kas2.state.model).toBe(B);
    expect(kas2.state.effort).toBe('low');
  });

  it('opt-out (disableAutoDefaultEffort): suppresses the /effort write but still applies a pre-existing default', async () => {
    const A = 'claude-opus-4.8';
    // Pre-existing saved default (low) differs from the model's native (high),
    // so the startup apply is observable; the opt-out only gates WRITES.
    const home = makeKiroHome({
      'chat.disableAutoDefaultEffort': true,
      'chat.modelDefaults': { [A]: { output_config: { effort: 'low' } } },
    });
    tc = new AcpTestCase({
      testName: 'sticky-optout-effort',
      extraEnv: { KIRO_HOME: home },
    });
    const kas = installStatefulKas(tc, {
      sessionId: 'sticky-session-1',
      models: [{ value: A, name: 'Claude Opus 4.8' }],
      initialModel: A,
      initialEffort: 'high',
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // apply-NOT-suppressed: the pre-existing default (low) is applied at
    // startup even though auto-default-effort is disabled.
    await waitForConfigSet(tc, 'effortLevel', 'low');
    tc.mock.notify('session/update', {
      sessionId: 'sticky-session-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: kas.configOptions(),
      },
    });
    await tc.waitForStore((s) => s.currentEffort === 'low', 6000);

    // write-SUPPRESSED: changing effort takes effect for the session but must
    // NOT persist a new value (no "(saved for ...)" suffix, cli.json unchanged).
    await tc.sendKeys('/effort high');
    await tc.pressEnter();
    await tc.waitForStore((s) => s.currentEffort === 'high', 6000);
    await tc.waitForVisibleText('Effort set to High', 3000);

    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('Effort set to High');
    expect(snap).not.toContain('saved for');

    // Let any (erroneous) async write flush, then confirm the on-disk default
    // is still the pre-existing 'low' — the /effort high did not write.
    await tc.sleepMs(500);
    const cli = readCliJson(home);
    expect(savedEffort(cli, A)).toBe('low');
  });

  it('opt-out (disableAutoDefaultModel): suppresses the /model write but still applies the saved default', async () => {
    const A = 'claude-opus-4.8';
    const B = 'claude-opus-4.7';
    // Pre-existing saved default model is B; server default (session/new) is A,
    // so the startup apply (switch to B) is observable.
    const home = makeKiroHome({
      'chat.disableAutoDefaultModel': true,
      'chat.defaultModel': B,
    });
    tc = new AcpTestCase({
      testName: 'sticky-optout-model',
      extraEnv: { KIRO_HOME: home },
    });
    installStatefulKas(tc, {
      sessionId: 'sticky-session-1',
      models: [
        { value: A, name: 'Claude Opus 4.8' },
        { value: B, name: 'Claude Opus 4.7' },
      ],
      initialModel: A,
      initialEffort: 'high',
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // apply-NOT-suppressed: saved default model B is applied at startup.
    await waitForConfigSet(tc, 'model', B);
    await tc.waitForStore((s) => s.currentModel?.id === B, 6000);

    // write-SUPPRESSED: switching to A must take effect but must NOT overwrite
    // the saved default (no "(saved as default)" suffix, cli.json unchanged).
    await tc.sendKeys(`/model ${A}`);
    await tc.pressEnter();
    await tc.waitForStore((s) => s.currentModel?.id === A, 6000);
    await tc.waitForVisibleText('Switched to Claude Opus 4.8', 3000);

    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('Switched to Claude Opus 4.8');
    expect(snap).not.toContain('saved as default');

    await tc.sleepMs(500);
    const cli = readCliJson(home);
    expect(cli['chat.defaultModel']).toBe(B);
  });
});
