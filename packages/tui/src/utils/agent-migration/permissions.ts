/**
 * Permission conversion: CLI `toolsSettings`/`allowedTools` → V3 (KAS)
 * `permissions.rules`. The hard case is shell patterns — the CLI allows regex,
 * KAS matches tokenized globs, so some translate only best-effort (flagged with
 * a warning). Pure: warnings accumulate into a caller-supplied array.
 */

import { regexToGlob } from './regex-to-glob.js';
import { V3_CAP_BY_V2_NAME } from './tool-table.js';

/** A single permission rule in the V3 (KAS) format. */
export interface V3PermissionRule {
  capability: string;
  match?: string[];
  exclude?: string[];
  effect: 'allow' | 'deny' | 'ask';
}

/** The V3 (KAS) permissions block. */
export interface V3Permissions {
  rules: V3PermissionRule[];
  policies?: string[];
}

/** A warning about a lossy or ambiguous conversion. */
export interface MigrationWarning {
  kind:
    | 'regex-shell-pattern'
    | 'regex-web-pattern'
    | 'unconvertible-pattern'
    | 'unmapped-allowed-tool'
    | 'deprecated-aws-tool'
    // `denyByDefault` + `autoAllowReadonly` can't coexist in V3 — read-only
    // auto-approval was dropped.
    | 'deny-by-default-readonly'
    | 'file-prompt'
    // An object-form hook KAS can't represent: a CLI tool hook (no `command`)
    // or an unknown trigger. Dropped rather than emitted as an invalid doc.
    | 'unconvertible-hook';
  detail?: string;
  /** Source config field, e.g. `toolsSettings.shell.allowedCommands`. */
  attribute?: string;
  /** Emitted glob(s) for regex conversions (empty if unconvertible). */
  converted?: string[];
  /** Rule effect — drives the allow/deny label and deny-all status in diagnostics. */
  effect?: 'allow' | 'deny';
}

/**
 * Convert CLI command/path patterns into V3 `match` globs, classified by
 * {@link regexToGlob}: lossless → emitted; lossy → emitted + `regex-*-pattern`
 * warning; unconvertible → *not* emitted (a fabricated glob would be
 * dangerously broad) + `unconvertible-pattern` warning. Filesystem paths are
 * already globs and pass through; a regex may fan out to several globs.
 */
export function convertPatterns(
  patterns: string[],
  capability: string,
  warnings: MigrationWarning[],
  effect: 'allow' | 'deny' = 'allow'
): string[] {
  const isRegexCapability =
    capability === 'shell' || capability === 'web_fetch';
  if (!isRegexCapability) {
    return patterns.slice();
  }

  const regexKind =
    capability === 'shell' ? 'regex-shell-pattern' : 'regex-web-pattern';
  const attribute =
    capability === 'shell'
      ? `toolsSettings.shell.${effect === 'deny' ? 'deniedCommands' : 'allowedCommands'}`
      : `toolsSettings.web_fetch.${effect === 'deny' ? 'blocked' : 'trusted'}`;
  const out: string[] = [];
  const seen = new Set<string>();
  for (const p of patterns) {
    const { globs, fidelity } = regexToGlob(p, {
      dropChaining: capability === 'shell',
    });
    if (fidelity === 'unconvertible') {
      if (effect === 'deny') {
        // Fail closed: a deny we can't translate must never silently vanish
        // (that would re-grant a command the author explicitly blocked). Deny
        // the whole capability instead; the warning tells the user to narrow it.
        if (!seen.has('*')) {
          seen.add('*');
          out.push('*');
        }
        warnings.push({
          kind: 'unconvertible-pattern',
          detail: p,
          attribute,
          converted: ['*'],
          effect,
        });
      } else {
        warnings.push({
          kind: 'unconvertible-pattern',
          detail: p,
          attribute,
          converted: [],
          effect,
        });
      }
      continue;
    }
    if (fidelity === 'lossy') {
      warnings.push({
        kind: regexKind,
        detail: p,
        attribute,
        converted: globs.slice(),
        effect,
      });
    }
    for (const g of globs) {
      if (!seen.has(g)) {
        seen.add(g);
        out.push(g);
      }
    }
  }
  return out;
}

/**
 * `exclude` list for a `denyByDefault` catch-all shell deny. Each allow glob is
 * excluded; a `<cmd> *` glob also mirrors its bare `<cmd>` form because KAS
 * expands allow matches (`git log *` also matches `git log`) but not excludes —
 * without the bare form the catch-all deny would still block `git log`.
 */
function shellDenyExcludeFor(allowGlobs: string[]): string[] {
  const exclude = new Set<string>();
  for (const g of allowGlobs) {
    exclude.add(g);
    if (g.endsWith(' *')) {
      const bare = g.slice(0, -2);
      if (bare.length > 0 && !/[*?]/.test(bare)) {
        exclude.add(bare);
      }
    }
  }
  return Array.from(exclude).sort();
}

/** Convert a CLI `toolsSettings` block into V3 `permissions.rules`. */
// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'convertToolsSettings' has a complexity of 41. Maximum allowed is 30.; refactor before extending
// LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 36 to the 30 allowed.; refactor before extending
// eslint-disable-next-line complexity, sonarjs/cognitive-complexity
export function convertToolsSettings(
  toolsSettings: Record<string, any>,
  warnings: MigrationWarning[]
): V3Permissions | undefined {
  const rules: V3PermissionRule[] = [];
  const policies: string[] = [];

  // Accept all CLI aliases for the shell settings key.
  const shell =
    toolsSettings.execute_bash ??
    toolsSettings.executeBash ??
    toolsSettings.shell ??
    {};
  const denyByDefault = Boolean(shell.denyByDefault);
  if (shell.autoAllowReadonly) {
    if (denyByDefault) {
      // No V3 runtime read-only detection, and a denyByDefault catch-all
      // (deny > allow) would override the read-only-shell preset — so keep
      // deny-by-default (stronger intent), drop auto-approval, and flag it.
      warnings.push({
        kind: 'deny-by-default-readonly',
        attribute: 'toolsSettings.shell',
      });
    } else {
      policies.push('read-only-shell');
    }
  }
  let allowedShellGlobs: string[] = [];
  if (
    Array.isArray(shell.allowedCommands) &&
    shell.allowedCommands.length > 0
  ) {
    allowedShellGlobs = convertPatterns(
      shell.allowedCommands,
      'shell',
      warnings
    );
    if (allowedShellGlobs.length > 0) {
      rules.push({
        capability: 'shell',
        match: allowedShellGlobs,
        effect: 'allow',
      });
    }
  }
  if (Array.isArray(shell.deniedCommands) && shell.deniedCommands.length > 0) {
    const match = convertPatterns(
      shell.deniedCommands,
      'shell',
      warnings,
      'deny'
    );
    if (match.length > 0) {
      rules.push({ capability: 'shell', match, effect: 'deny' });
    }
  }
  if (denyByDefault) {
    // Catch-all deny whose `exclude` carves out the allow list (deny outranks
    // allow, so an un-excluded allow glob would be clobbered).
    const exclude = shellDenyExcludeFor(allowedShellGlobs);
    const denyAll: V3PermissionRule = {
      capability: 'shell',
      match: ['*'],
      effect: 'deny',
    };
    if (exclude.length > 0) denyAll.exclude = exclude;
    rules.push(denyAll);
  }

  const fsRead = toolsSettings.fs_read ?? toolsSettings.read ?? {};
  if (Array.isArray(fsRead.allowedPaths) && fsRead.allowedPaths.length > 0) {
    rules.push({
      capability: 'fs_read',
      match: convertPatterns(fsRead.allowedPaths, 'fs_read', warnings),
      effect: 'allow',
    });
  }
  if (Array.isArray(fsRead.deniedPaths) && fsRead.deniedPaths.length > 0) {
    rules.push({
      capability: 'fs_read',
      match: convertPatterns(fsRead.deniedPaths, 'fs_read', warnings),
      effect: 'deny',
    });
  }

  const fsWrite = toolsSettings.fs_write ?? toolsSettings.write ?? {};
  if (Array.isArray(fsWrite.allowedPaths) && fsWrite.allowedPaths.length > 0) {
    rules.push({
      capability: 'fs_write',
      match: convertPatterns(fsWrite.allowedPaths, 'fs_write', warnings),
      effect: 'allow',
    });
  }
  if (Array.isArray(fsWrite.deniedPaths) && fsWrite.deniedPaths.length > 0) {
    rules.push({
      capability: 'fs_write',
      match: convertPatterns(fsWrite.deniedPaths, 'fs_write', warnings),
      effect: 'deny',
    });
  }

  const webFetch = toolsSettings.web_fetch ?? toolsSettings.webFetch ?? {};
  if (Array.isArray(webFetch.trusted) && webFetch.trusted.length > 0) {
    const match = convertPatterns(webFetch.trusted, 'web_fetch', warnings);
    if (match.length > 0) {
      rules.push({ capability: 'web_fetch', match, effect: 'allow' });
    }
  }
  if (Array.isArray(webFetch.blocked) && webFetch.blocked.length > 0) {
    const match = convertPatterns(
      webFetch.blocked,
      'web_fetch',
      warnings,
      'deny'
    );
    if (match.length > 0) {
      rules.push({ capability: 'web_fetch', match, effect: 'deny' });
    }
  }

  // `trustedAgents` (prompt-free allowlist) → a `subagent` allow rule matched
  // by name. `availableAgents` (discovery scoping) is handled via `tools` tags
  // in the orchestrator; this module only emits the `trustedAgents` rule.
  const subagent = toolsSettings.subagent ?? toolsSettings.use_subagent ?? {};
  if (
    Array.isArray(subagent.trustedAgents) &&
    subagent.trustedAgents.length > 0
  ) {
    rules.push({
      capability: 'subagent',
      match: subagent.trustedAgents.slice().sort(),
      effect: 'allow',
    });
  }

  if (rules.length === 0 && policies.length === 0) return undefined;
  const result: V3Permissions = { rules };
  if (policies.length > 0) result.policies = policies;
  return result;
}

/** Tools KAS handles without policy checks — silently skip in allowedTools. */
const NO_POLICY_TOOLS = new Set([
  'knowledge',
  'todo',
  'todo_list',
  'task_list',
]);

/** Deprecated V2 tools with specific migration messages. */
const DEPRECATED_TOOLS = new Set(['aws', 'use_aws']);

/**
 * Convert a CLI `allowedTools` list into capability-level `allow` rules (V2
 * trusts a tool individually; KAS allows its whole capability). `*` → one `all`
 * allow; `@server[/tool]` → a merged `mcp` rule; built-ins → capability rules
 * deduped by capability; NO_POLICY_TOOLS skipped; aws/unmapped flagged.
 */
export function convertAllowedTools(
  allowedTools: unknown,
  warnings: MigrationWarning[]
): V3PermissionRule[] {
  if (
    allowedTools === '*' ||
    (Array.isArray(allowedTools) && allowedTools.includes('*'))
  ) {
    return [{ capability: 'all', effect: 'allow' }];
  }
  if (!Array.isArray(allowedTools)) {
    return [];
  }

  const capabilities = new Set<string>();
  const mcpServers = new Set<string>();

  for (const tool of allowedTools) {
    if (typeof tool !== 'string') {
      continue;
    }

    // `@server` → `server/*`, `@server/tool` → `server/tool` (KAS slash form).
    if (tool.startsWith('@') && tool.length > 1) {
      const ref = tool.slice(1);
      const slashIdx = ref.indexOf('/');
      if (slashIdx > 0) {
        mcpServers.add(ref);
      } else {
        mcpServers.add(`${ref}/*`);
      }
      continue;
    }

    const capability = V3_CAP_BY_V2_NAME.get(tool);
    if (capability) {
      capabilities.add(capability);
    } else if (NO_POLICY_TOOLS.has(tool)) {
      // V3 handles these without policy checks — silently skip.
    } else if (DEPRECATED_TOOLS.has(tool)) {
      warnings.push({
        kind: 'deprecated-aws-tool',
        detail: tool,
      });
    } else {
      warnings.push({
        kind: 'unmapped-allowed-tool',
        detail: tool,
      });
    }
  }

  const rules: V3PermissionRule[] = Array.from(capabilities)
    .sort()
    .map((capability) => ({ capability, effect: 'allow' as const }));

  if (mcpServers.size > 0) {
    rules.push({
      capability: 'mcp',
      match: Array.from(mcpServers).sort(),
      effect: 'allow',
    });
  }

  return rules;
}
