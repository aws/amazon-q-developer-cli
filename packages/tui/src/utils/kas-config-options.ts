import { KAS_DEFAULT_AGENT_ID } from '../constants/agents';
import { features, Feature } from '../features';
import type {
  KiroModelOptionMeta,
  EffortSchemaPath,
} from '@kiro/acp-type-covenant';
import type { SessionOrigin } from '../stores/app-store';

/** A model the user can switch to, normalized across engines. */
export interface ModelEntry {
  id: string;
  name: string;
  description?: string;
  rateMultiplier?: number;
  rateUnit?: string;
  /**
   * Request-schema location of this model's `effort` field, advertised by KAS
   * via `_meta.kiro.effortSchemaPath`. Present only for effort-capable models;
   * drives where a persisted effort default is written (`output_config.effort`
   * vs `reasoning.effort`). Absent when KAS does not advertise it.
   */
  effortSchemaPath?: EffortSchemaPath;
}

/** A reasoning effort level advertised by the active model. */
export interface EffortEntry {
  value: string;
  name: string;
}

/**
 * An agent (KAS "mode") the user can switch to, normalized across engines.
 * `source` (`bundled`/`global`/`workspace`) drives `/agent` menu grouping and
 * bundled-agent filtering; `welcomeMessage` drives the agent chip banner.
 */
export interface AgentEntry {
  id: string;
  name: string;
  description?: string;
  source?: string;
  welcomeMessage?: string;
}

export interface ParsedModels {
  models: ModelEntry[];
  currentModelId?: string;
}

export interface ParsedEfforts {
  efforts: EffortEntry[];
  currentLevel: string | null;
}

export interface ParsedAgents {
  agents: AgentEntry[];
  currentAgentId?: string;
}

/** Where a KAS `KasModelConfigUpdate` originated, identified by the emitting call site. */
export type KasConfigOrigin =
  | 'newSession'
  | 'loadSession'
  | 'clientInitiated'
  | 'serverPush';

/** Map TUI-facing mode names to KAS wire names. */
export function toKasModeId(tuiModeId: string): string {
  // The TUI surfaces the planner under the internal name `kiro_planner`; the
  // agent's read-only planner builtin mode is wire id `plan`.
  if (tuiModeId === 'kiro_planner') return 'plan';
  // KAS still emits/accepts `vibe` as the wire id for the default mode.
  if (tuiModeId === 'default') return 'vibe';
  return tuiModeId;
}

/** Map KAS wire mode names back to TUI-facing names. */
export function fromKasModeId(kasModeId: string): string {
  if (kasModeId === 'plan') return 'kiro_planner';
  if (kasModeId === 'vibe') return 'default';
  return kasModeId;
}

/** Parse the `category: 'model'` select into normalized model entries. */
export function parseModelsFromConfigOptions(
  configOptions: unknown
): ParsedModels | undefined {
  const select = findSelect(configOptions, (o) => o.category === 'model');
  if (!select) return undefined;
  const models = validEntries(select.options).map((o): ModelEntry => {
    // KAS attaches per-model rate info under `_meta.kiro`; read defensively.
    const kiro = (o._meta as { kiro?: KiroModelOptionMeta } | undefined)?.kiro;
    return {
      id: o.value as string,
      name: o.name as string,
      description:
        typeof o.description === 'string' ? o.description : undefined,
      rateMultiplier:
        typeof kiro?.rateMultiplier === 'number'
          ? kiro.rateMultiplier
          : undefined,
      rateUnit: typeof kiro?.rateUnit === 'string' ? kiro.rateUnit : undefined,
      effortSchemaPath:
        kiro?.effortSchemaPath === 'output_config' ||
        kiro?.effortSchemaPath === 'reasoning'
          ? kiro.effortSchemaPath
          : undefined,
    };
  });
  return { models, currentModelId: currentValueOf(select) };
}

/** Parse the `id: 'effortLevel'` select into normalized effort entries. */
export function parseEffortsFromConfigOptions(
  configOptions: unknown
): ParsedEfforts | undefined {
  const select = findSelect(configOptions, (o) => o.id === 'effortLevel');
  if (!select) return undefined;
  const efforts = validEntries(select.options).map(
    (o): EffortEntry => ({ value: o.value as string, name: o.name as string })
  );
  return { efforts, currentLevel: currentValueOf(select) ?? null };
}

/**
 * Parse the `id: 'mode'` (category `mode`) select into normalized agent
 * entries. Ids are mapped via {@link fromKasModeId}; bundled agents not on the
 * allowlist are filtered out. `welcomeMessage` reads `_meta.kiro.welcomeMessage`
 * and falls back to top-level `_meta.welcomeMessage` for older KAS builds.
 */
export function parseAgentsFromConfigOptions(
  configOptions: unknown
): ParsedAgents | undefined {
  const select = findSelect(configOptions, (o) => o.id === 'mode');
  if (!select) return undefined;
  const agents = validEntries(select.options)
    .map((o): AgentEntry => {
      return {
        id: fromKasModeId(o.value as string),
        name: o.name as string,
        description:
          typeof o.description === 'string' ? o.description : undefined,
        source: getMetaSource(o._meta),
        welcomeMessage: readWelcomeMessage(o._meta),
      };
    })
    .filter((a) => !isAgentHidden(a.id, a.source));
  const currentValue = currentValueOf(select);
  return {
    agents,
    currentAgentId: currentValue ? fromKasModeId(currentValue) : undefined,
  };
}

/**
 * Welcome banner for the *current* mode, resolved from the raw `mode` option
 * matching `currentValue`. Unlike {@link parseAgentsFromConfigOptions} this does
 * NOT apply the display allowlist, so a hidden/bundled agent that becomes the
 * active mode (e.g. an autonomous handoff) still surfaces its banner.
 */
export function currentModeWelcomeMessage(
  configOptions: unknown
): string | undefined {
  const select = findSelect(configOptions, (o) => o.id === 'mode');
  const current = select ? currentValueOf(select) : undefined;
  if (!select || !current) return undefined;
  const opt = validEntries(select.options).find((o) => o.value === current);
  return readWelcomeMessage(opt?._meta);
}

/**
 * Resolve the current model + agent selections (for a `session/new` or
 * `session/load` result) from a configOptions payload. The agent chip is
 * shown whenever the mode select advertises a current value, even if that
 * agent is filtered from the menu (e.g. a hidden bundled agent). The welcome
 * banner is resolved from the raw current-mode option (see
 * {@link currentModeWelcomeMessage}) so a hidden current agent still surfaces
 * one; the session-result consumer decides whether to show it (suppressed on
 * load).
 */
export function deriveCurrentSelections(configOptions: unknown): {
  currentModel?: { id: string; name: string };
  currentAgent?: { name: string; welcomeMessage?: string };
} {
  const models = parseModelsFromConfigOptions(configOptions);
  const model = models?.currentModelId
    ? models.models.find((m) => m.id === models.currentModelId)
    : undefined;
  const agents = parseAgentsFromConfigOptions(configOptions);
  return {
    currentModel: model ? { id: model.id, name: model.name } : undefined,
    currentAgent: agents?.currentAgentId
      ? {
          name: agents.currentAgentId,
          welcomeMessage: currentModeWelcomeMessage(configOptions),
        }
      : undefined,
  };
}

/**
 * Resolve the model to apply at session start. An explicit `--model` flag wins
 * over a saved `chat.defaultModel`; absent both, the engine's own default is
 * kept (returns null). Applied only on a new session, never on resume.
 */
export function resolveInitialModel(params: {
  flagModel?: string | null;
  savedDefaultModel?: string | null;
}): string | null {
  return params.flagModel || params.savedDefaultModel || null;
}

/**
 * Decide which effort level to apply for the current model, or null for "leave
 * it alone". `shouldApply` carries the store's session/origin decision
 * (see {@link shouldApplyEffortDefault}), keeping this function origin-agnostic
 * and purely about validity/idempotency: skip when not eligible, when there is
 * no current model, when the model has no saved default, when that default is
 * not an available level, or when it equals the current level.
 */
export function resolveEffortToApply(params: {
  currentModelId: string | null | undefined;
  availableEfforts: string[];
  currentEffort: string | null;
  savedEffortForModel: string | null;
  shouldApply: boolean;
}): string | null {
  const {
    currentModelId,
    availableEfforts,
    currentEffort,
    savedEffortForModel,
    shouldApply,
  } = params;
  if (!shouldApply) return null;
  if (!currentModelId) return null;
  if (!savedEffortForModel) return null;
  if (!availableEfforts.includes(savedEffortForModel)) return null;
  if (currentEffort === savedEffortForModel) return null;
  return savedEffortForModel;
}

/**
 * Whether a `KasModelConfigUpdate` should auto-apply the active model's saved
 * effort default. Pure policy, no state.
 *
 * Applies only when the model changed AND either the user explicitly switched
 * (`clientInitiated`) or this is the first model resolution of a new session
 * without an explicit launch `--effort`. Resumed sessions and later autonomous
 * `serverPush` changes never stomp the session's effort (v2 parity).
 */
export function shouldApplyEffortDefault(args: {
  origin: KasConfigOrigin;
  sessionOrigin: SessionOrigin;
  modelChanged: boolean;
  hasExplicitEffort: boolean;
  hadPriorModel: boolean;
}): boolean {
  return (
    args.modelChanged &&
    (args.origin === 'clientInitiated' ||
      (args.sessionOrigin === 'new' &&
        !args.hadPriorModel &&
        !args.hasExplicitEffort))
  );
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * The only KAS *bundled* agents that may surface in the `/agent` menu, keyed by
 * their `fromKasModeId`-normalized id. This is an allowlist rather than a
 * denylist: KAS ships a growing set of bundled modes (semantic_reviewer,
 * autonomous, quick-spec, bug-fix, …), most internal or non-conversational, so
 * any bundled mode that isn't one of these is hidden by default. User/workspace
 * agents are always shown — a config the user opted into is never dropped.
 */
const BUILTIN_AGENT_ALLOWLIST = new Set<string>([
  KAS_DEFAULT_AGENT_ID,
  'kiro_planner',
  'spec',
  // Explore (C2S) gated to internal nightly via Feature::C2s rollout.
  ...(features.isEnabled(Feature.C2s) ? ['Explore'] : []),
]);

type RawSelect = {
  currentValue?: unknown;
  options?: unknown;
};

/** Read `_meta.kiro.source` from a config option entry, if present. */
function getMetaSource(meta: unknown): string | undefined {
  const kiro = (meta as { kiro?: { source?: unknown } } | undefined)?.kiro;
  return typeof kiro?.source === 'string' ? kiro.source : undefined;
}

/**
 * Whether an agent entry should be hidden from listings. Targets KAS *bundled*
 * agents only: a user/workspace agent (or one with no source metadata) is
 * always shown; a bundled agent is hidden unless its normalized id is on
 * {@link BUILTIN_AGENT_ALLOWLIST}.
 */
function isAgentHidden(id: string, source: string | undefined): boolean {
  if (source !== 'bundled') return false;
  return !BUILTIN_AGENT_ALLOWLIST.has(id);
}

/** Locate a `type: 'select'` entry by `category` or `id`. */
function findSelect(
  configOptions: unknown,
  match: (opt: Record<string, unknown>) => boolean
): RawSelect | undefined {
  if (!Array.isArray(configOptions)) return undefined;
  for (const opt of configOptions as Array<Record<string, unknown>>) {
    if (opt.type !== 'select') continue;
    if (match(opt)) return opt as RawSelect;
  }
  return undefined;
}

/** Narrow a raw options list to the well-formed `{ value, name, ... }` entries. */
function validEntries(options: unknown): Array<Record<string, unknown>> {
  const raw = Array.isArray(options) ? options : [];
  return raw.filter(
    (o): o is Record<string, unknown> =>
      typeof o === 'object' &&
      o !== null &&
      typeof (o as Record<string, unknown>).value === 'string' &&
      typeof (o as Record<string, unknown>).name === 'string'
  );
}

function currentValueOf(select: RawSelect): string | undefined {
  return typeof select.currentValue === 'string'
    ? select.currentValue
    : undefined;
}

/**
 * Read a mode/agent welcome banner from an option's `_meta`. KAS namespaces it
 * under `_meta.kiro.welcomeMessage`; older payloads put it at the top level.
 */
function readWelcomeMessage(meta: unknown): string | undefined {
  const m = meta as
    | { welcomeMessage?: unknown; kiro?: { welcomeMessage?: unknown } }
    | undefined;
  return typeof m?.kiro?.welcomeMessage === 'string'
    ? m.kiro.welcomeMessage
    : typeof m?.welcomeMessage === 'string'
      ? m.welcomeMessage
      : undefined;
}
