/**
 * Shared harness for the KAS sticky agent/model/effort default tests.
 *
 * Models the v3/KAS wire contract for config-option changes faithfully so the
 * tests exercise the TUI's own apply/reconcile/persist logic (its outgoing
 * `set_config_option` requests + store state + cli.json), never KAS's internal
 * behavior:
 *
 *   - `session/set_config_option('model')` updates the server's model and
 *     CARRIES the current effort over when it is still valid for the new model
 *     (every model here shares {@link LEVELS}, so a carried value is always
 *     valid). It returns the updated `configOptions` in the RESPONSE and NEVER
 *     pushes a separate `config_option_update` notification.
 *   - `session/set_config_option('mode')` updates the active agent, and when
 *     the agent declares a model (see `modeModelMap`) it changes the model too
 *     — the same clientInitiated funnel a `/model` switch takes.
 *   - `session/set_config_option('effortLevel')` records a known level.
 *   - Everything else (autopilot, unknown) is acked with an empty body.
 */
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
  chmodSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { AcpTestCase } from './AcpTestCase';
import { defaultKasModes } from './default-agent';

export interface SelectEntry {
  value: string;
  name: string;
}

export interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

/** Full effort ladder; shared across all models so a carried value is always valid. */
export const LEVELS: SelectEntry[] = [
  { value: 'low', name: 'Low' },
  { value: 'medium', name: 'Medium' },
  { value: 'high', name: 'High' },
  { value: 'xhigh', name: 'xHigh' },
];

/**
 * Build the `category: 'model'` config option. Each model advertises
 * `_meta.kiro.effortSchemaPath: 'output_config'` (KAS's authoritative path for
 * Claude-family models) unless it appears in `noEffortSchema`, so an effort
 * default saved via `/effort set-current-as-default` lands at
 * `output_config.effort`, matching the cli.json shape these tests assert.
 * A model in `noEffortSchema` advertises no path.
 */
export function modelConfigOption(
  currentValue: string,
  models: SelectEntry[],
  noEffortSchema: ReadonlySet<string> = new Set()
) {
  return {
    type: 'select' as const,
    id: 'model',
    name: 'Model',
    category: 'model',
    currentValue,
    options: models.map((m) => ({
      value: m.value,
      name: m.name,
      _meta: noEffortSchema.has(m.value)
        ? {}
        : { kiro: { effortSchemaPath: 'output_config' } },
    })),
  };
}

/**
 * Build the `category: 'thought_level'` (effort) config option. Pass a subset
 * of {@link LEVELS} via `levels` to model a model whose ladder omits values, or
 * `[]` to model a model with no effort schema at all.
 */
export function effortConfigOption(
  currentValue: string,
  levels: SelectEntry[] = LEVELS
) {
  return {
    type: 'select' as const,
    id: 'effortLevel',
    name: 'Effort',
    category: 'thought_level',
    currentValue,
    options: levels,
  };
}

/**
 * Build the `category: 'mode'` (agent) config option. `agents[].value` is the
 * KAS wire id (e.g. `vibe` for the default agent, which the TUI normalizes to
 * `default`); `agents[].name` is its display label.
 */
export function modeConfigOption(currentValue: string, agents: SelectEntry[]) {
  return {
    type: 'select' as const,
    id: 'mode',
    name: 'Mode',
    category: 'mode',
    currentValue,
    options: agents,
  };
}

export interface KasState {
  model: string;
  effort: string;
  mode: string;
}

export interface StatefulKasOptions {
  sessionId: string;
  models: SelectEntry[];
  initialModel: string;
  initialEffort: string;
  /** Models whose `effortLevel` option is empty (no effort schema). */
  noEffortSchema?: ReadonlySet<string>;
  /**
   * Agents to surface as a `mode` config option. When omitted, no mode option
   * is emitted (model/effort-only tests keep the previous behavior).
   */
  agents?: SelectEntry[];
  /** Initial active agent wire id. Defaults to the first `agents` entry. */
  initialMode?: string;
  /** Agent wire id -> model id the agent switches the session to on select. */
  modeModelMap?: Record<string, string>;
}

/**
 * Install a stateful mock KAS. Returns the live state plus a `configOptions()`
 * builder so a test can re-sync the store (defeating the documented
 * session-start broadcast race) with a `config_option_update` carrying the
 * mock's CURRENT state — never a value the test invented.
 */
export function installStatefulKas(
  tc: AcpTestCase,
  opts: StatefulKasOptions
): { state: KasState; configOptions: () => unknown[] } {
  const levelValues = new Set(LEVELS.map((l) => l.value));
  const noEffortSchema = opts.noEffortSchema ?? new Set<string>();
  const state: KasState = {
    model: opts.initialModel,
    effort: opts.initialEffort,
    mode: opts.initialMode ?? opts.agents?.[0]?.value ?? 'vibe',
  };
  const configOptions = () => {
    const options: unknown[] = [];
    if (opts.agents) options.push(modeConfigOption(state.mode, opts.agents));
    options.push(modelConfigOption(state.model, opts.models, noEffortSchema));
    options.push(
      effortConfigOption(
        state.effort,
        noEffortSchema.has(state.model) ? [] : LEVELS
      )
    );
    return options;
  };

  tc.mock.on('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on('session/new', (params) => {
    // Honor the initial agent the TUI requests via `_meta.kiro.modeId`
    // (sourced from `--agent` / `chat.defaultAgent`) when a mode option exists.
    const requested = (params as { _meta?: { kiro?: { modeId?: string } } })
      ?._meta?.kiro?.modeId;
    if (opts.agents && typeof requested === 'string') state.mode = requested;
    return {
      sessionId: opts.sessionId,
      modes: defaultKasModes(),
      configOptions: configOptions(),
    };
  });
  tc.mock.on('session/load', () => ({
    modes: defaultKasModes(),
    configOptions: configOptions(),
  }));
  tc.mock.on('session/set_config_option', (params) => {
    const p = params as SetConfigOptionParams;
    if (p.configId === 'model' && typeof p.value === 'string') {
      state.model = p.value;
      return { configOptions: configOptions() };
    }
    if (p.configId === 'mode' && typeof p.value === 'string') {
      state.mode = p.value;
      const carried = opts.modeModelMap?.[p.value];
      if (carried) state.model = carried;
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
    return {};
  });

  return { state, configOptions };
}

// ── KIRO_HOME / cli.json helpers ─────────────────────────────────────────────
//
// TODO: fold this into the AcpTestCase/TestCase harness. TestCase already
// creates a sandboxed KIRO_HOME but neither exposes its path nor lets a test
// seed and reuse it across processes. Add a stateful option (expose the
// sandbox path via TestPaths, allow seeding cli.json) so tests can read the
// persisted cli.json directly instead of creating a parallel KIRO_HOME and
// overriding via `extraEnv.KIRO_HOME`. Deferred here: it needs changes to
// TestCase/TestPaths beyond this PR's scope.
//
// Until then: create our own KIRO_HOME, seed cli.json, and override via
// `extraEnv.KIRO_HOME` (which wins because TestCase spreads `extraEnv` last).

const createdHomes: string[] = [];
const extraCleanups: Array<() => void> = [];

export function makeKiroHome(seed?: Record<string, unknown>): string {
  const home = mkdtempSync(join(tmpdir(), 'kiro-sticky-'));
  createdHomes.push(home);
  if (seed) {
    const p = join(home, 'settings', 'cli.json');
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, JSON.stringify(seed, null, 2), 'utf-8');
  }
  return home;
}

export function readCliJson(home: string): Record<string, unknown> {
  const p = join(home, 'settings', 'cli.json');
  if (!existsSync(p)) return {};
  return JSON.parse(readFileSync(p, 'utf-8')) as Record<string, unknown>;
}

/** Read a saved per-model effort default out of a parsed cli.json. */
export function savedEffort(
  cli: Record<string, unknown>,
  modelId: string
): unknown {
  const defaults = cli['chat.modelDefaults'] as
    | Record<string, { output_config?: { effort?: unknown } }>
    | undefined;
  return defaults?.[modelId]?.output_config?.effort;
}

export async function waitForCliJson(
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

/** Register a best-effort cleanup to run in {@link cleanupStickyDefaults}. */
export function registerCleanup(fn: () => void): void {
  extraCleanups.push(fn);
}

/** Remove every KIRO_HOME + registered resource created this test. */
export function cleanupStickyDefaults(): void {
  for (const home of createdHomes.splice(0)) {
    try {
      rmSync(home, { recursive: true, force: true });
    } catch {
      /* best-effort */
    }
  }
  for (const cleanup of extraCleanups.splice(0)) {
    try {
      cleanup();
    } catch {
      /* best-effort */
    }
  }
}

// ── set_config_option observation ────────────────────────────────────────────

/** All values written for a given configId, in order. */
export function configSetValues(tc: AcpTestCase, configId: string): unknown[] {
  return tc.mock
    .receivedRequests('session/set_config_option')
    .filter((r) => (r.params as SetConfigOptionParams).configId === configId)
    .map((r) => (r.params as SetConfigOptionParams).value);
}

/** All effortLevel values written so far, in order. */
export function effortSetValues(tc: AcpTestCase): unknown[] {
  return configSetValues(tc, 'effortLevel');
}

/** Poll until a `set_config_option` with the given configId+value is observed. */
export async function waitForConfigSet(
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

// ── Resume (--resume) plumbing ───────────────────────────────────────────────
//
// A scripted `chat_cli` stub emits a `--list-sessions` envelope so the TUI picks
// a winner and drives `session/load` (resume) instead of `session/new`.

export interface ScriptedBinary {
  binPath: string;
  cleanup: () => void;
}

export function writeResumeStub(listSessionsJson: string): ScriptedBinary {
  const dir = mkdtempSync(join(tmpdir(), 'kiro-sticky-stub-'));
  const binPath = join(dir, 'chat_cli');
  const listingPath = join(dir, 'list-sessions.json');
  writeFileSync(listingPath, listSessionsJson);
  const script =
    `#!/usr/bin/env bash\n` +
    `for a in "$@"; do [[ "$a" == "--list-sessions" ]] && { cat "${listingPath}"; exit 0; }; done\n` +
    `exit 0\n`;
  writeFileSync(binPath, script);
  chmodSync(binPath, 0o755);
  return {
    binPath,
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}

/** A listing envelope with a single most-recent native KAS (v3) winner. */
export function v3WinnerListing(cwd: string, winnerId: string): string {
  return JSON.stringify([
    {
      cwd,
      sessions: [
        {
          sessionId: winnerId,
          source: 'v3',
          title: 'Resumed session',
          updatedAt: '2026-06-01T12:00:00.000Z',
        },
      ],
    },
  ]);
}
