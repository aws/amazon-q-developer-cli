/**
 * Predicates and mappings used by the `--resume` boot path and the
 * `/chat` picker handlers when deciding whether a merged-listing
 * entry belongs to the active engine, can be resumed at all, and
 * which `chat _ ensure-session` source-format token it maps to.
 *
 * No string-prefix protocol lives here anymore: cross-engine
 * conversion is performed by callers passing structured
 * `SessionEntry` data straight to `ensureSession`.
 */

import type { SourceFormat } from './ensure-session-cli';
import type { SessionSource } from './list-all-sessions-cli';

/**
 * `true` when the merged-listing source belongs to the engine the
 * TUI booted into. KAS owns `v3`; the Rust V2 engine owns `v2`
 * and (via `LegacySessionExporter`) `classic`.
 */
export function isActiveEngineSource(
  source: SessionSource,
  activeIsKas: boolean
): boolean {
  return activeIsKas
    ? source === 'v3'
    : source === 'v2' || source === 'classic';
}

/**
 * `true` when the active engine has an implemented import path from
 * `source`. KAS accepts every source via the converter; the rust V2
 * engine accepts `v2` and `classic` but `chat _ ensure-session`
 * returns `KAS source -> V2 target not supported` for `v3`. Pickers
 * and `--resume` selection use this to skip rows that would fail at
 * conversion time.
 */
export function isResumableSource(
  source: SessionSource,
  activeIsKas: boolean
): boolean {
  return activeIsKas || source !== 'v3';
}

/**
 * Map a merged-listing `SessionSource` to the `SourceFormat` accepted
 * by `chat _ ensure-session`. The wire token is `v3` for the KAS
 * engine's storage; the converter calls it `kas`.
 */
export function sourceFormatFor(source: SessionSource): SourceFormat {
  return source === 'v3' ? 'kas' : source;
}
