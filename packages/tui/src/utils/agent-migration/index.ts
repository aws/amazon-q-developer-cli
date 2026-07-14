/**
 * Agent-config migration module: V2 (CLI) → universal (V2 + V3/KAS) upgrade.
 *
 * Public API surface. Internal layering:
 *  - `regex-to-glob.ts` — pure V2 regex → V3 glob(s) translation
 *  - `permissions.ts` — pure `toolsSettings`/`allowedTools` → V3 rules conversion
 *  - `hooks.ts`       — pure object-form `hooks` → KAS array-form conversion
 *  - `migrate.ts`     — pure derivation (`migrateAgentConfig`) + universal upgrade
 *                       + classification (`upgradeAgentConfig`)
 *  - `scan.ts`        — scan/classify/analyze agent dirs (read-only)
 *  - `io.ts`          — filesystem read/backup/write (`upgradeAgentFile`)
 *
 * Prefer importing from this barrel rather than the individual files.
 */
export * from './regex-to-glob.js';
export * from './permissions.js';
export * from './hooks.js';
export * from './migrate.js';
export * from './scan.js';
export * from './io.js';
