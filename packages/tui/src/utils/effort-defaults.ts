/**
 * Per-model reasoning-effort defaults, persisted to cli.json under
 * `chat.modelDefaults` for cross-engine parity with v2 (Rust).
 *
 * v2 stores effort as a NESTED object keyed by model id, where the leaf path
 * depends on the model family's schema:
 *
 *   {
 *     "chat.modelDefaults": {
 *       "claude-opus-4.7": { "output_config": { "effort": "low" } },
 *       "gpt-5.1":         { "reasoning":     { "effort": "medium" } }
 *     }
 *   }
 *
 * We read/write the SAME key and shape so a default set in either engine
 * transfers to the other. Source of truth for the shape:
 *   crates/chat-cli-v2/src/agent/acp/commands/effort.rs (write)
 *   crates/chat-cli-v2/src/agent/rts/mod.rs (read/apply)
 */

import { readCliSettings, updateCliSettingWith } from './cli-settings.js';
import { Settings } from '../constants/settings.js';

/** cli.json key holding per-model defaults (shared with v2). */
export const MODEL_DEFAULTS_SETTING = Settings.CHAT_MODEL_DEFAULTS;

/**
 * Known schema paths that hold the effort field, by model family. Claude/qwen
 * expose effort under `output_config.effort`; GPT/openai under
 * `reasoning.effort`.
 *
 * The authoritative path is now resolved at the call site from KAS's
 * `_meta.kiro.effortSchemaPath` (advertised per model option) and passed into
 * {@link persistEffortDefault}. This list remains the source of truth for (a)
 * the family heuristic fallback used when KAS omits the path (older servers),
 * and (b) the single-leaf invariant — writing one path prunes the other so a
 * model never holds both.
 */
export const KNOWN_EFFORT_PATHS = [
  'output_config.effort',
  'reasoning.effort',
] as const;

/**
 * Resolve the schema path the given model uses for effort. Heuristic mirror of
 * v2's family split, used as a FALLBACK only when KAS does not advertise the
 * authoritative `effortSchemaPath` for the model (see KNOWN_EFFORT_PATHS).
 */
export function effortPathForModel(modelId: string): string {
  const id = modelId.toLowerCase();
  return id.includes('gpt') || id.includes('openai')
    ? 'reasoning.effort'
    : 'output_config.effort';
}

/** Read a dotted path out of a plain object, returning the leaf or undefined. */
function getByPath(obj: Record<string, unknown>, path: string): unknown {
  let cur: unknown = obj;
  for (const seg of path.split('.')) {
    if (!cur || typeof cur !== 'object' || Array.isArray(cur)) return undefined;
    cur = (cur as Record<string, unknown>)[seg];
  }
  return cur;
}

/** Build a nested object for a dotted path with `level` at the leaf. */
function buildNested(path: string, level: string): Record<string, unknown> {
  const segs = path.split('.');
  return segs.reduceRight<Record<string, unknown>>(
    (node, seg) => ({ [seg]: node }),
    level as unknown as Record<string, unknown>
  );
}

/**
 * Remove the leaf at dotted `path` from `obj` (in place), then prune any
 * ancestor objects left empty by the removal. No-op if the path is absent.
 * Used to guarantee a single effort leaf per model: writing one schema path
 * deletes the other so a stale value at the unused path can never be read back.
 */
function deleteByPath(obj: Record<string, unknown>, path: string): void {
  const segs = path.split('.');
  // Collect the [container, key] chain down to the leaf; bail if any
  // intermediate segment is missing or not a plain object.
  const chain: Array<[Record<string, unknown>, string]> = [];
  let cur: Record<string, unknown> = obj;
  for (let i = 0; i < segs.length; i++) {
    const seg = segs[i]!;
    chain.push([cur, seg]);
    if (i === segs.length - 1) break;
    const next = cur[seg];
    if (!next || typeof next !== 'object' || Array.isArray(next)) return;
    cur = next as Record<string, unknown>;
  }
  // Delete the leaf, then walk back up removing now-empty parent objects.
  for (let i = chain.length - 1; i >= 0; i--) {
    const [container, key] = chain[i]!;
    if (i === chain.length - 1) {
      delete container[key];
      continue;
    }
    const child = container[key];
    if (
      child &&
      typeof child === 'object' &&
      !Array.isArray(child) &&
      Object.keys(child as Record<string, unknown>).length === 0
    ) {
      delete container[key];
    }
  }
}

/** Deep-merge plain objects (source wins at leaves), preserving sibling keys. */
function deepMerge(
  target: Record<string, unknown>,
  source: Record<string, unknown>
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...target };
  for (const [k, v] of Object.entries(source)) {
    const existing = out[k];
    if (
      v &&
      typeof v === 'object' &&
      !Array.isArray(v) &&
      existing &&
      typeof existing === 'object' &&
      !Array.isArray(existing)
    ) {
      out[k] = deepMerge(
        existing as Record<string, unknown>,
        v as Record<string, unknown>
      );
    } else {
      out[k] = v;
    }
  }
  return out;
}

/**
 * Read the saved effort default for `modelId` from cli.json, tolerating either
 * known schema path (`output_config.effort` or `reasoning.effort`). Returns the
 * level string, or undefined when absent. Robust regardless of model family.
 */
export function readSavedEffortDefault(modelId: string): string | undefined {
  const defaults = readCliSettings()[MODEL_DEFAULTS_SETTING];
  if (!defaults || typeof defaults !== 'object' || Array.isArray(defaults)) {
    return undefined;
  }
  const node = (defaults as Record<string, unknown>)[modelId];
  if (!node || typeof node !== 'object' || Array.isArray(node))
    return undefined;
  for (const path of KNOWN_EFFORT_PATHS) {
    const leaf = getByPath(node as Record<string, unknown>, path);
    if (typeof leaf === 'string' && leaf !== '') return leaf;
  }
  return undefined;
}

/**
 * Persist `level` as the per-model effort default for `modelId`, writing the
 * nested v2-compatible shape at the model's effort schema path. When the caller
 * has resolved the authoritative path from KAS's `_meta.kiro.effortSchemaPath`
 * (e.g. `"reasoning.effort"`), pass it as `resolvedPath`; otherwise the family
 * name heuristic ({@link effortPathForModel}) is used as a fallback. The entire
 * read-merge-write runs inside cli-settings' serialized write queue (via
 * {@link updateCliSettingWith}) so a concurrent in-process writer to
 * `chat.modelDefaults` cannot cause a lost update. Deep-merges into the
 * existing value so other models (and other fields on the same model) are
 * preserved, then deletes the OTHER known effort path under this model so a
 * model never holds both `output_config.effort` and `reasoning.effort` — a
 * stale leaf at the unused path could otherwise be read back by the
 * family-tolerant {@link readSavedEffortDefault}.
 */
export async function persistEffortDefault(
  modelId: string,
  level: string,
  resolvedPath?: string
): Promise<void> {
  const path = resolvedPath ?? effortPathForModel(modelId);
  const node = buildNested(path, level);
  await updateCliSettingWith(MODEL_DEFAULTS_SETTING, (existing) => {
    const base =
      existing && typeof existing === 'object' && !Array.isArray(existing)
        ? (existing as Record<string, unknown>)
        : {};
    const prevModel = base[modelId];
    const mergedModel = deepMerge(
      prevModel && typeof prevModel === 'object' && !Array.isArray(prevModel)
        ? (prevModel as Record<string, unknown>)
        : {},
      node
    );
    // Enforce a single effort leaf: drop any value at the other known path so
    // switching a model's family path can't leave a contradictory stale leaf.
    for (const other of KNOWN_EFFORT_PATHS) {
      if (other !== path) deleteByPath(mergedModel, other);
    }
    return { ...base, [modelId]: mergedModel };
  });
}
