/**
 * Hooks migration: CLI object-form `hooks` (a map keyed by trigger) → KAS
 * array-of-documents form, so a migrated config loads under KAS's
 * `hooks: z.array(hookDocumentSchema)` profile schema (object-form `hooks`
 * fails whole-profile validation and drops the entire agent). Array-form input
 * is already universal and passes through unchanged, so re-migration is a
 * no-op. A hook KAS can't represent — a CLI tool hook (no `command`) or an
 * unknown trigger — is dropped with a warning rather than emitted as an invalid
 * document. Pure: warnings accumulate into a caller-supplied array.
 */

import type { MigrationWarning } from './permissions.js';

/**
 * Canonical camelCase hook triggers KAS understands (mirrors the Rust
 * `HookTrigger` enum). Object-form keys spelled any other way can't be mapped.
 */
const HOOK_TRIGGERS = new Set<string>([
  'agentSpawn',
  'userPromptSubmit',
  'preToolUse',
  'postToolUse',
  'stop',
]);

// CLI hook defaults shared by the V1 and V2 readers. These extras are emitted
// only when the authored value differs, keeping the common case minimal while
// round-tripping safely (both readers restore the same default). `timeout_ms`
// is handled separately — its default differs per reader.
const DEFAULT_MAX_OUTPUT_SIZE = 10 * 1024;
const DEFAULT_CACHE_TTL_SECONDS = 0;
// CLI absent-timeout default (milliseconds). KAS defaults an absent array-form
// timeout differently, so an omitted value would run at a different timeout per
// engine; we emit this explicitly instead.
const DEFAULT_TIMEOUT_MS = 10 * 1000;

/**
 * Convert a `hooks` value to the universal (KAS array) form. `undefined` when
 * there is nothing to write (absent or a non-object/array value); an empty
 * object yields an empty array (valid under KAS, unlike `{}`). Array-form input
 * passes through verbatim so re-running the migration changes nothing.
 */
export function convertHooks(
  hooks: unknown,
  warnings: MigrationWarning[]
): unknown | undefined {
  if (Array.isArray(hooks)) {
    // Already universal — pass through so re-migration is a no-op.
    return hooks;
  }
  if (isPlainObject(hooks)) {
    return objectFormToDocs(hooks, warnings);
  }
  return undefined;
}

/**
 * Flatten the trigger-keyed map into a flat list of KAS hook documents.
 * Triggers are visited in sorted order for deterministic output.
 */
function objectFormToDocs(
  map: Record<string, unknown>,
  warnings: MigrationWarning[]
): unknown[] {
  const docs: unknown[] = [];
  for (const trigger of Object.keys(map).sort()) {
    if (!HOOK_TRIGGERS.has(trigger)) {
      warnings.push(unconvertibleHookWarning(trigger));
      continue;
    }
    const entries = map[trigger];
    if (!Array.isArray(entries)) {
      continue;
    }
    entries.forEach((entry, idx) => {
      const doc = hookEntryToDoc(trigger, idx, entry, warnings);
      if (doc !== undefined) {
        docs.push(doc);
      }
    });
  }
  return docs;
}

/**
 * Project one CLI hook object into a KAS hook document. `undefined` (with a
 * warning) when the hook has no `command` — KAS's action is a discriminated
 * union with no equivalent for a CLI tool hook.
 */
function hookEntryToDoc(
  trigger: string,
  idx: number,
  entry: unknown,
  warnings: MigrationWarning[]
): Record<string, unknown> | undefined {
  if (!isPlainObject(entry)) {
    warnings.push(unconvertibleHookWarning(trigger));
    return undefined;
  }
  const command = entry.command;
  if (typeof command !== 'string') {
    warnings.push(unconvertibleHookWarning(trigger));
    return undefined;
  }

  const doc: Record<string, unknown> = {
    // KAS requires a non-empty `name`; synthesize a stable one from trigger +
    // position.
    name: `${trigger}-${idx}`,
    trigger,
  };
  if (typeof entry.matcher === 'string') {
    doc.matcher = entry.matcher;
  }
  doc.action = { type: 'command', command };

  // CLI stores milliseconds; KAS's `timeout` is seconds. Always emitted (using
  // the CLI default when the source omitted it): the CLI and KAS readers apply
  // *different* absent-timeout defaults, so an omitted value would run at a
  // different timeout depending on which engine reads it. Clamp to a floor of
  // 1s for any authored value: KAS's timeout is an integer >= 1s (0 means
  // "disabled", not "instant"), so an authored sub-second — or literal 0 — maps
  // to the tightest bound KAS can express rather than inverting the user's
  // intent into no-timeout.
  const timeoutMs = asU64(entry.timeout_ms);
  doc.timeout =
    timeoutMs !== undefined
      ? Math.max(divCeil(timeoutMs, 1000), 1)
      : DEFAULT_TIMEOUT_MS / 1000;

  // CLI-only extras; KAS silently ignores unknown keys, and the CLI reader
  // restores them. Both readers share one default for these, so omitting at the
  // default round-trips safely.
  const size = nonDefaultU64(entry.max_output_size, DEFAULT_MAX_OUTPUT_SIZE);
  if (size !== undefined) {
    doc.maxOutputSize = size;
  }
  const ttl = nonDefaultU64(entry.cache_ttl_seconds, DEFAULT_CACHE_TTL_SECONDS);
  if (ttl !== undefined) {
    doc.cacheTtlSeconds = ttl;
  }
  return doc;
}

/** A non-negative integer value, mirroring serde's `Value::as_u64`. */
function asU64(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0
    ? value
    : undefined;
}

/** A numeric field read only when present and set to a non-default value. */
function nonDefaultU64(
  value: unknown,
  defaultValue: number
): number | undefined {
  const n = asU64(value);
  return n !== undefined && n !== defaultValue ? n : undefined;
}

/** Integer ceiling division (`n / d` rounded up), matching Rust's `div_ceil`. */
function divCeil(n: number, d: number): number {
  return Math.ceil(n / d);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function unconvertibleHookWarning(detail: string): MigrationWarning {
  return {
    kind: 'unconvertible-hook',
    detail,
    attribute: 'hooks',
  };
}
