/**
 * Agent config migration orchestration: V2 (CLI) → V3 (KAS). Tool-name
 * mapping, V2/V3 detection, and field passthrough. The `toolsSettings →
 * permissions.rules` conversion lives in `permissions.ts`. Pure and
 * deterministic: same input → same output and warnings.
 */

import {
  convertAllowedTools,
  convertToolsSettings,
  type MigrationWarning,
  type V3PermissionRule,
  type V3Permissions,
} from './permissions.js';
import { V3_TAG_BY_V2_NAME, aliasesOfV2Name } from './tool-table.js';

// Re-export the permission/warning surface so consumers import from this barrel.
export type {
  MigrationWarning,
  V3PermissionRule,
  V3Permissions,
} from './permissions.js';
export { convertToolsSettings };

/** Result of migrating a single agent config. */
export interface MigrationResult {
  config: Record<string, unknown>;
  warnings: MigrationWarning[];
  /** True when the input was already V3 (no conversion needed). */
  isAlreadyV3: boolean;
  /** Tool-field transformation, for display. */
  toolsSummary?: { from: unknown; to: unknown };
}

/** Deprecated in V3, dropped silently. (`aws`/`use_aws` warn separately.) */
const DROPPED_TOOLS = new Set(['thinking', 'report_issue', 'report']);

/** V2 names that translate to a *different* V3 selector → signal a V2-authored config. */
const V2_MARKER_TOOL_NAMES = new Set<string>([
  ...[...V3_TAG_BY_V2_NAME]
    .filter(([name, tag]) => name !== tag)
    .map(([name]) => name),
  ...DROPPED_TOOLS,
]);

/** V2-only schema fields — a V2-detection signal, preserved on disk. */
const CLI_ONLY_FIELDS = [
  'keyboardShortcut',
  'toolsSettings',
  'allowedTools',
  'toolAliases',
];

/** V3-only trust fields. Mirrors the Rust loader (`load.rs`). */
const V3_TRUST_FIELDS = ['permissions', 'includePowers', 'excludedTools'];

/** True if a parsed config carries any V3 trust field. */
export function hasV3TrustField(config: Record<string, unknown>): boolean {
  return V3_TRUST_FIELDS.some((f) => f in config);
}

/**
 * True when a config shows a V2-authored signal: a `toolsSettings`/`allowedTools`
 * block, or `tools` entries spelled as CLI tool names that need translation.
 * (Broader than "trust" — a V2-spelled `tools` entry alone qualifies.)
 */
export function hasV2Signal(config: Record<string, unknown>): boolean {
  if ('toolsSettings' in config || 'allowedTools' in config) {
    return true;
  }
  const tools = config.tools;
  if (Array.isArray(tools)) {
    for (const t of tools) {
      if (typeof t === 'string' && V2_MARKER_TOOL_NAMES.has(t)) {
        return true;
      }
    }
  }
  return false;
}

/** Fields that carry over unchanged from V2 to V3. */
const PASSTHROUGH_FIELDS = [
  'name',
  'description',
  'model',
  'prompt',
  'resources',
  'hooks',
  'mcpServers',
  // V3-schema fields, so a hybrid config with V2 markers keeps trust the user
  // already set when migrated.
  'permissions',
  'welcomeMessage',
  'includeMcpJson',
  'includePowers',
  'excludedTools',
];

/** Detect V3 (KAS) format: no CLI-only fields and no CLI tool names. */
export function isAlreadyV3(config: Record<string, unknown>): boolean {
  for (const field of CLI_ONLY_FIELDS) {
    if (field in config) {
      return false;
    }
  }
  const tools = config.tools;
  if (Array.isArray(tools)) {
    for (const t of tools) {
      if (typeof t === 'string' && V2_MARKER_TOOL_NAMES.has(t)) {
        return false;
      }
    }
  }
  return true;
}

/** Convert a CLI `tools` value to KAS tags; unrecognized names pass through verbatim. */
function convertTools(
  cliTools: unknown,
  warnings: MigrationWarning[]
): string[] | '*' | undefined {
  if (cliTools === '*') {
    return '*';
  }
  if (!Array.isArray(cliTools)) {
    return undefined;
  }

  const kasTools = new Set<string>();
  for (const tool of cliTools) {
    if (typeof tool !== 'string') {
      continue;
    }
    const mappedTag = V3_TAG_BY_V2_NAME.get(tool);
    if (mappedTag !== undefined) {
      kasTools.add(mappedTag);
    } else if (tool === 'aws' || tool === 'use_aws') {
      kasTools.add(tool);
      warnings.push({
        kind: 'deprecated-aws-tool',
        detail: tool,
      });
    } else {
      // Not a known V2 tool: a V3 tag/id, MCP ref, glob, or custom name. V3
      // ignores names it doesn't recognize, so keep it verbatim for V2 — we
      // deliberately don't track the open-ended set of valid V3 selectors.
      kasTools.add(tool);
    }
  }
  return Array.from(kasTools).sort();
}

/** Migrate a parsed V2 config into V3. Pure; already-V3 input echoes back. */
export function migrateAgentConfig(
  config: Record<string, unknown>
): MigrationResult {
  if (isAlreadyV3(config)) {
    return { config, warnings: [], isAlreadyV3: true };
  }

  const warnings: MigrationWarning[] = [];
  const result: Record<string, unknown> = {};

  for (const field of PASSTHROUGH_FIELDS) {
    if (field in config) {
      result[field] = config[field];
    }
  }

  // V3 allows only relative file:// prompts — flag absolute/home-relative ones.
  if (
    typeof config.prompt === 'string' &&
    config.prompt.startsWith('file://')
  ) {
    const p = config.prompt.slice('file://'.length);
    if (p.startsWith('/') || p.startsWith('~')) {
      warnings.push({
        kind: 'file-prompt',
        detail: config.prompt,
      });
    }
  }

  // useLegacyMcpJson → includeMcpJson propagation for V3 compatibility.
  if ('useLegacyMcpJson' in config && !('includeMcpJson' in config)) {
    result.includeMcpJson = config.useLegacyMcpJson;
  }

  const cliTools = config.tools;
  let toolsSummary: { from: unknown; to: unknown } | undefined;
  if (cliTools !== undefined) {
    const converted = convertTools(cliTools, warnings);
    if (converted !== undefined) {
      result.tools = converted;
      toolsSummary = { from: cliTools, to: converted };
    }
  }

  // V2's `availableAgents` scopes invocable subagents; V3 expresses that via
  // per-subagent tags, so swap the broad `subagent` tag for `subagent/<name>`.
  const ts = config.toolsSettings as Record<string, any> | undefined;
  const availableAgents =
    ts?.subagent?.availableAgents ?? ts?.use_subagent?.availableAgents;
  if (
    Array.isArray(availableAgents) &&
    availableAgents.length > 0 &&
    Array.isArray(result.tools)
  ) {
    const tools = result.tools;
    const idx = tools.indexOf('subagent');
    if (idx >= 0) {
      const specific = availableAgents
        .filter((a): a is string => typeof a === 'string')
        .map((a) => `subagent/${a}`);
      const replaced = [
        ...tools.slice(0, idx),
        ...specific,
        ...tools.slice(idx + 1),
      ];
      result.tools = Array.from(new Set(replaced)).sort();
      toolsSummary = { from: cliTools, to: result.tools };
    }
  }

  const generatedRules: V3PermissionRule[] = [];
  const generatedPolicies: string[] = [];

  const toolsSettings = config.toolsSettings;
  if (toolsSettings && typeof toolsSettings === 'object') {
    const permissions = convertToolsSettings(
      toolsSettings as Record<string, any>,
      warnings
    );
    if (permissions) {
      generatedRules.push(...permissions.rules);
      if (permissions.policies) generatedPolicies.push(...permissions.policies);
    }
  }

  // allowedTools (V2 trusted-tools list) → capability-level allow rules.
  generatedRules.push(...convertAllowedTools(config.allowedTools, warnings));

  if (generatedRules.length > 0 || generatedPolicies.length > 0) {
    const existing = result.permissions as V3Permissions | undefined;
    if (existing && Array.isArray(existing.rules)) {
      existing.rules.push(...generatedRules);
      if (generatedPolicies.length > 0) {
        existing.policies = [
          ...(existing.policies ?? []),
          ...generatedPolicies,
        ];
      }
    } else {
      const perms: V3Permissions = { rules: generatedRules };
      if (generatedPolicies.length > 0) perms.policies = generatedPolicies;
      result.permissions = perms;
    }
  }

  return { config: result, warnings, isAlreadyV3: false, toolsSummary };
}

// ── Universal-config upgrade ──

/**
 * Upgrade-flow classification. `universal-in-sync` is also the no-trust
 * fallback; `v2-only` and `universal-out-of-sync` are the actionable states.
 */
export type AgentClassification =
  | 'v2-only'
  | 'universal-out-of-sync'
  | 'universal-in-sync'
  | 'v3-only';

/** Result of upgrading a single agent config. */
export interface UpgradeResult {
  /** V2 fields preserved verbatim, V3 fields added/updated. */
  config: Record<string, unknown>;
  classification: AgentClassification;
  warnings: MigrationWarning[];
  /** True when `config` differs from the input — the caller should write. */
  changed: boolean;
}

/** Drop V3 trust fields so a re-derivation depends only on V2 inputs. */
function stripV3TrustFields(
  config: Record<string, unknown>
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(config)) {
    if (!V3_TRUST_FIELDS.includes(k)) {
      out[k] = v;
    }
  }
  return out;
}

/** Order-independent structural equality of two JSON values. */
function deepEqualJson(a: unknown, b: unknown): boolean {
  return canonicalJson(a) === canonicalJson(b);
}

/** Stable, recursive JSON serialization with sorted keys. */
function canonicalJson(value: unknown): string {
  const norm = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(norm);
    if (v && typeof v === 'object') {
      const o = v as Record<string, unknown>;
      return Object.keys(o)
        .sort()
        .reduce<Record<string, unknown>>((acc, k) => {
          acc[k] = norm(o[k]);
          return acc;
        }, {});
    }
    return v;
  };
  return JSON.stringify(norm(value));
}

/**
 * Build the universal `tools` array: start from the V3-derived tags, then add
 * back each original V2 name UNLESS one of its aliases is already present. This
 * keeps distinct V2 tools V3 folds into a tag (e.g. `grep`/`web_fetch`, whose
 * name isn't their tag, so V2 needs them listed) while dropping redundant alias
 * spellings (`fs_read`/`execute_bash` when `read`/`shell` are already there).
 * Scoped subagent discovery drops the broad subagent aliases; `'*'` stays `'*'`.
 */
function mergeToolsForUniversalConfig(
  original: unknown,
  derived: unknown
): string[] | '*' {
  if (original === '*' || derived === '*') {
    return '*';
  }
  const originalNames = Array.isArray(original)
    ? original.filter((t): t is string => typeof t === 'string')
    : [];
  const derivedTags = Array.isArray(derived)
    ? derived.filter((t): t is string => typeof t === 'string')
    : [];

  const result = new Set<string>(derivedTags);
  for (const name of originalNames) {
    if (!aliasesOfV2Name(name).some((a) => result.has(a))) {
      result.add(name);
    }
  }

  // Scoped subagents present → drop the broad subagent aliases so V3 stays scoped.
  const hasScopedSubagent = [...result].some((t) => t.startsWith('subagent/'));
  if (hasScopedSubagent) {
    for (const a of aliasesOfV2Name('subagent')) result.delete(a);
  }

  return Array.from(result).sort();
}

/**
 * Universal-config upgrade: preserve V2 fields, overlay V3-derived `tools`
 * (union) and `permissions`. Pure. Re-deriving overwrites V3, so writers back
 * up first (see `upgradeAgentFile`).
 */
export function upgradeAgentConfig(
  config: Record<string, unknown>
): UpgradeResult {
  const hasV2 = hasV2Signal(config);
  const hasV3 = hasV3TrustField(config);

  if (!hasV2 && hasV3) {
    return { config, classification: 'v3-only', warnings: [], changed: false };
  }
  if (!hasV2 && !hasV3) {
    return {
      config,
      classification: 'universal-in-sync',
      warnings: [],
      changed: false,
    };
  }

  const stripped = stripV3TrustFields(config);
  const derived = migrateAgentConfig(stripped);

  const enriched: Record<string, unknown> = { ...config };
  if ('tools' in derived.config) {
    enriched.tools = mergeToolsForUniversalConfig(
      config.tools,
      derived.config.tools
    );
  }
  if ('permissions' in derived.config) {
    enriched.permissions = derived.config.permissions;
  } else if ('toolsSettings' in enriched || 'allowedTools' in enriched) {
    // KAS skips a config with CLI-only fields but no `permissions` marker (it
    // reads as CLI-exclusive), so attach an empty one when the derivation
    // produced no rules (e.g. allowedTools: ['aws']).
    enriched.permissions = { rules: [] };
  }
  // else: nothing was derivable from V2 and there is no CLI-only permission
  // source (`toolsSettings`/`allowedTools`) — so any `permissions` present is
  // hand-authored V3. Preserve it verbatim; deleting it here would silently
  // drop the user's trust (the config was only flagged via tool-name spelling).

  if (!hasV3) {
    // A config whose only V2 signal is tool-name spelling converges after the
    // union is written once (the V2 name stays alongside its V3 tag, e.g.
    // `web_fetch` + `web`). Re-running then yields an identical config, so
    // derive `changed` structurally: an already-migrated config settles to
    // `universal-in-sync` instead of being re-listed and rewritten (with a
    // fresh .bak) on every run.
    const changed = !deepEqualJson(enriched, config);
    return {
      config: enriched,
      classification: changed ? 'v2-only' : 'universal-in-sync',
      warnings: derived.warnings,
      changed,
    };
  }

  // Both present: in-sync iff the derived tools/permissions equal the input's.
  const inSync =
    deepEqualJson(config.permissions, enriched.permissions) &&
    deepEqualJson(config.tools, enriched.tools);
  return {
    config: enriched,
    classification: inSync ? 'universal-in-sync' : 'universal-out-of-sync',
    warnings: derived.warnings,
    changed: !inSync,
  };
}

/** True if a classification is one the user can act on. */
export const ACTIONABLE_CLASSIFICATIONS: readonly AgentClassification[] = [
  'v2-only',
  'universal-out-of-sync',
];

export function isActionable(c: AgentClassification): boolean {
  return ACTIONABLE_CLASSIFICATIONS.includes(c);
}
