/**
 * Pure row/routing model for the `/config` overlay (`ConfigPanel`).
 *
 * `/config` is the consolidated view of everything that shapes a session:
 * agents, MCP servers, powers, steering, skills, hooks, env variables and
 * secrets (Figma "Kiro Next" frames 23-41). The panel itself is a thin shell;
 * category rows, per-category pages, footers and enter-routing all live here
 * as pure functions so they are unit-testable, mirroring
 * `settings-panel-model.ts`.
 *
 * The whole surface is dark-shipped behind `Feature.CloudConfig` — the
 * `/config` command is only registered when the flag is on, so nothing in
 * this file is reachable for regular users.
 */

import type {
  McpServerInfo,
  HookInfo,
  PowerEntry,
  SteeringDocEntry,
} from '../../stores/app-store.js';
import type { SkillEntry, SteeringEntry } from '../../types/commands.js';
import type { AgentEntry } from '../../utils/kas-config-options.js';
import { truncateToWidth } from '../../utils/text-width.js';

/** Where a config item is defined. Mirrors the mocks' Source column. */
export type ConfigSource = 'local' | 'cloud';

/** A cloud-config sync diagnostic message for the footer. */
export interface ConfigDiagnosticLine {
  severity: string;
  message: string;
}

export type ConfigCategoryId =
  | 'agents'
  | 'mcp'
  | 'powers'
  | 'steering'
  | 'skills'
  | 'hooks'
  | 'env'
  | 'secrets';

export interface ConfigCategoryRow {
  id: ConfigCategoryId;
  /** Display label, e.g. "MCP servers". */
  label: string;
  /** Source summary, e.g. "local", "cloud", "local + cloud", or "—". */
  source: string;
  /** Status summary, e.g. "5 active", "2 configured", or "—". */
  status: string;
}

/**
 * The store data the row builders need. Callers resolve store state and pass
 * it in so the builders stay pure. All lists are the existing store caches —
 * `/config` introduces no new data plumbing for the category counts.
 */
export interface ConfigSnapshot {
  /** Whether the active session is a cloud-sandbox session. */
  cloudSession: boolean;
  agents: readonly AgentEntry[];
  mcpServers: readonly McpServerInfo[];
  /**
   * Slash-command steering projection — the fallback listing when KAS
   * hasn't pushed `_kiro/steering/documents_changed` (older KAS, V2).
   */
  steering: readonly SteeringEntry[];
  /**
   * Steering documents from `_kiro/steering/documents_changed`, carrying
   * the mock-specified inclusion mode. Preferred over `steering` when
   * non-empty.
   */
  steeringDocs: readonly SteeringDocEntry[];
  skills: readonly SkillEntry[];
  hooks: readonly HookInfo[];
  /** Installed powers from `_kiro/powers/items_changed`. */
  powers: readonly PowerEntry[];
  /** KIRO_*-prefixed env vars visible to this process. */
  kiroEnv: ReadonlyArray<readonly [string, string]>;
  /**
   * Cloud-config sync diagnostics (`_kiro/diagnostics/changed`, domain
   * `cloudConfig`) — surfaced in the footer. Empty when nothing to report.
   */
  diagnostics: readonly ConfigDiagnosticLine[];
}

/** Subcommand aliases accepted after `/config `, mapped to category ids. */
const SUBCOMMAND_ALIASES: Record<string, ConfigCategoryId> = {
  agents: 'agents',
  agent: 'agents',
  mcp: 'mcp',
  powers: 'powers',
  power: 'powers',
  steering: 'steering',
  skills: 'skills',
  skill: 'skills',
  hooks: 'hooks',
  env: 'env',
  environment: 'env',
  // 'secrets' deliberately absent: the category is deferred (no row, no
  // page), so the typed form gets the honest "Unknown config category"
  // alert instead of silently doing nothing.
};

/** Resolve a `/config <sub>` token to a category, or undefined if unknown. */
export function resolveConfigSubcommand(
  token: string
): ConfigCategoryId | undefined {
  return SUBCOMMAND_ALIASES[token.toLowerCase()];
}

/** Subcommand values surfaced in autocomplete for `/config`. */
export const CONFIG_SUBCOMMANDS: readonly string[] = [
  'agents',
  'mcp',
  'powers',
  'steering',
  'skills',
  'hooks',
  'env',
];

/**
 * loadingMessage strings the routed /config → mcp/hooks handlers claim for
 * their V2 RPC window. Shared so ESC-cancel in the panel can release the
 * lock without clobbering an unrelated owner's spinner (claim-only rule).
 */
export const CONFIG_MCP_LOADER = 'Loading MCP servers...';
export const CONFIG_HOOKS_LOADER = 'Loading hooks...';
export const CONFIG_HANDOFF_LOADERS: ReadonlySet<string> = new Set([
  CONFIG_MCP_LOADER,
  CONFIG_HOOKS_LOADER,
]);

const NONE = '—'; // — em dash for "no data"

/**
 * Summarize the Source column for a category from its item sources.
 * In a cloud session everything the session sees is cloud config; locally
 * items may be mixed once cloud propagation lands.
 */
function summarizeSources(sources: readonly ConfigSource[]): string {
  if (sources.length === 0) return NONE;
  const hasLocal = sources.includes('local');
  const hasCloud = sources.includes('cloud');
  if (hasLocal && hasCloud) return 'local + cloud';
  return hasCloud ? 'cloud' : 'local';
}

/**
 * Effective source of a config item given the session placement. `explicit`
 * is the per-item origin collapsed from the KAS ConfigResource descriptor
 * (`_meta.kiro.resource`, kiro-agent PR #2141) and wins when present;
 * placement is the fallback for older KAS or items without a descriptor: a
 * cloud session's config is served by the cloud, a local session's by local
 * files.
 */
export function effectiveSource(
  cloudSession: boolean,
  explicit?: ConfigSource
): ConfigSource {
  return explicit ?? (cloudSession ? 'cloud' : 'local');
}

function count(n: number, noun: string): string {
  return `${n} ${noun}`;
}

/**
 * Frame-25 Install column: KAS's agent-source values (`bundled` / `global` /
 * `workspace`) title-cased for display. Unknown or missing values render
 * blank rather than guessing.
 */
function installLabel(source: string | undefined): string {
  switch (source) {
    case 'bundled':
      return 'Bundled';
    case 'global':
      return 'Global';
    case 'workspace':
      return 'Workspace';
    default:
      // Unknown or missing → blank, never a guess or a raw passthrough.
      return '';
  }
}

/** Build the top-level category rows (frames 23/26). */
export function buildCategoryRows(snap: ConfigSnapshot): ConfigCategoryRow[] {
  // Per-item origins come from the KAS ConfigResource descriptor when
  // reported (`configSource`), with session placement as the fallback —
  // so a cloud-synced steering file in a local session reads "cloud" and
  // the category summarizes to "local + cloud".
  const perItem = (sources: ReadonlyArray<ConfigSource | undefined>) =>
    sources.map((s) => effectiveSource(snap.cloudSession, s));
  const mcpSources = perItem(snap.mcpServers.map((s) => s.source));
  // Prefer the documents_changed listing (inclusion mode + descriptor);
  // fall back to the slash-command projection for older KAS / V2.
  const steeringItems: ReadonlyArray<{ configSource?: ConfigSource }> =
    snap.steeringDocs.length > 0 ? snap.steeringDocs : snap.steering;
  return [
    {
      id: 'agents',
      label: 'agents',
      source: summarizeSources(perItem(snap.agents.map((a) => a.configSource))),
      status:
        snap.agents.length > 0 ? count(snap.agents.length, 'available') : NONE,
    },
    {
      id: 'mcp',
      label: 'MCP servers',
      source: summarizeSources(mcpSources),
      status:
        snap.mcpServers.length > 0
          ? count(
              snap.mcpServers.filter((s) => s.status === 'running').length,
              'active'
            )
          : NONE,
    },
    {
      id: 'powers',
      label: 'powers',
      source: summarizeSources(perItem(snap.powers.map((p) => p.configSource))),
      status:
        snap.powers.length > 0 ? count(snap.powers.length, 'installed') : NONE,
    },
    {
      id: 'steering',
      label: 'steering',
      source: summarizeSources(
        perItem(steeringItems.map((s) => s.configSource))
      ),
      status:
        steeringItems.length > 0
          ? count(
              steeringItems.length,
              steeringItems.length === 1 ? 'file' : 'files'
            )
          : NONE,
    },
    {
      id: 'skills',
      label: 'skills',
      source: summarizeSources(perItem(snap.skills.map((s) => s.configSource))),
      status:
        snap.skills.length > 0
          ? count(
              snap.skills.length,
              snap.skills.length === 1 ? 'skill' : 'skills'
            )
          : NONE,
    },
    {
      id: 'hooks',
      label: 'hooks',
      source: summarizeSources(perItem(snap.hooks.map((h) => h.configSource))),
      status:
        snap.hooks.length > 0 ? count(snap.hooks.length, 'configured') : NONE,
    },
    {
      id: 'env',
      label: 'env variables',
      source: 'local',
      status:
        snap.kiroEnv.length > 0
          ? count(snap.kiroEnv.length, 'configured')
          : NONE,
    },
    // Secrets deliberately absent: "not p0" in the mocks, KAS doesn't sync
    // them, and no local page exists either — a permanently-dash row is
    // noise. The category id stays in the type so the row (and a page)
    // return with one entry here when secrets ship.
  ];
}

/**
 * Footer lines under the category table. Frame 23 (local session) vs frame 26
 * (cloud session). The frame-26 `/config upload` sentence is intentionally
 * omitted until that command exists.
 *
 * Cloud-config sync diagnostics (KAS `_kiro/diagnostics/changed`, PR #2142)
 * lead the footer when present — a stale/unavailable sync is the first thing
 * a user needs to see. An empty set adds nothing (it is not a health claim).
 */
export function buildTopFooterLines(
  cloudSession: boolean,
  diagnostics: readonly ConfigDiagnosticLine[] = [],
  // Severity markers, passed in from the component so the pure model never
  // hardcodes glyphs (ASCII mode substitutes x/! via useGlyphs).
  glyphs: { cross: string; warning: string } = { cross: '✗', warning: '⚠' }
): string[] {
  // Severity → marker: error/warning are the covenant's known arms; any
  // other value (e.g. a future informational severity) renders unmarked
  // and after the marked lines rather than masquerading as a warning.
  const marker = (severity: string): string | null =>
    severity === 'error'
      ? glyphs.cross
      : severity === 'warning'
        ? glyphs.warning
        : null;
  const ordered = [...diagnostics].sort(
    (a, b) =>
      (marker(a.severity) === null ? 1 : 0) -
      (marker(b.severity) === null ? 1 : 0)
  );
  const diagLines = ordered.map((d) => {
    const m = marker(d.severity);
    return m ? `${m} ${d.message}` : d.message;
  });
  const base = cloudSession
    ? ['Edit cloud configs at https://app.kiro.dev/settings']
    : [
        'Manage cloud and local config syncing at https://app.kiro.dev/settings',
        'Cloud configs apply to cloud sessions; local configs to this machine',
      ];
  return [...diagLines, ...base];
}

/** What selecting a category does. */
export type ConfigSelectResult =
  // Open the shared /mcp panel — /config mcp and /mcp are the same view.
  | { kind: 'open-mcp' }
  // Open the existing /hooks panel.
  | { kind: 'open-hooks' }
  // Navigate to an in-panel category page.
  | { kind: 'page'; category: ConfigCategoryId }
  // No page behind this row (secrets).
  | { kind: 'none' };

export function resolveCategorySelect(
  id: ConfigCategoryId
): ConfigSelectResult {
  switch (id) {
    case 'mcp':
      return { kind: 'open-mcp' };
    case 'hooks':
      return { kind: 'open-hooks' };
    case 'secrets':
      return { kind: 'none' };
    default:
      return { kind: 'page', category: id };
  }
}

/** A rendered category page: columns + string rows + footer lines. */
export interface ConfigPage {
  title: string;
  columns: string[];
  rows: string[][];
  footerLines: string[];
  emptyMessage?: string;
}

const CLOUD_EDIT_LINE = 'To edit cloud configs: https://app.kiro.dev/settings';
const CONFLICT_LINE =
  'In case of conflict, local will override cloud configurations';

/**
 * Build a category page (frames 24/25/27/32/33/35-41). MCP and hooks never
 * reach here — they route to their existing panels via
 * {@link resolveCategorySelect}.
 */
export function buildCategoryPage(
  category: ConfigCategoryId,
  snap: ConfigSnapshot
): ConfigPage {
  const src = (explicit?: ConfigSource) =>
    effectiveSource(snap.cloudSession, explicit);
  switch (category) {
    case 'agents':
      return {
        title: '/config — agents',
        columns: ['Agent', 'Source', 'Install', 'Details'],
        // Install (frame 25): where the agent definition is installed —
        // Bundled (ships with KAS) / Global (~/.kiro) / Workspace (.kiro).
        // From the mode option's _meta.kiro.source; blank when unreported.
        rows: snap.agents.map((a) => [
          a.name,
          src(a.configSource),
          installLabel(a.source),
          a.description ?? '',
        ]),
        footerLines: snap.cloudSession
          ? [CLOUD_EDIT_LINE]
          : [CONFLICT_LINE, CLOUD_EDIT_LINE],
        emptyMessage: 'No agents available.',
      };
    case 'steering':
      return {
        title: '/config — steering',
        columns: ['Name', 'Source', 'Inclusion'],
        // Prefer the documents_changed listing: it carries the real
        // inclusion mode (always/manual/fileMatch — frames 33/39). The
        // slash-command projection is the fallback (older KAS / V2); it has
        // no inclusion fact, so those rows show the scope kind instead.
        rows:
          snap.steeringDocs.length > 0
            ? snap.steeringDocs.map((s) => [
                s.name,
                src(s.configSource),
                s.inclusion ?? s.scope,
              ])
            : snap.steering.map((s) => [
                s.name,
                src(s.configSource),
                s.source.kind,
              ]),
        footerLines: snap.cloudSession
          ? [CLOUD_EDIT_LINE]
          : [
              CONFLICT_LINE,
              'To edit local configs: modify ~/.kiro/steering/ (user-level) or .kiro/steering/ (workspace-level)',
              CLOUD_EDIT_LINE,
            ],
        emptyMessage: 'No steering files configured.',
      };
    case 'skills':
      return {
        title: '/config — skills',
        columns: ['Name', 'Source', 'Description'],
        rows: snap.skills.map((s) => [
          s.name,
          src(s.configSource),
          s.description ?? '',
        ]),
        footerLines: snap.cloudSession
          ? [CLOUD_EDIT_LINE]
          : [
              CONFLICT_LINE,
              'To edit local configs: modify ~/.kiro/skills/ (user-level) or .kiro/skills/ (workspace-level)',
              CLOUD_EDIT_LINE,
            ],
        emptyMessage: 'No skills configured.',
      };
    case 'powers':
      return {
        title: '/config — powers',
        columns: ['Name', 'Source', 'Description'],
        // Installed powers from _kiro/powers/items_changed (frames 32/37).
        rows: snap.powers.map((p) => [
          p.displayName ?? p.name,
          src(p.configSource),
          p.description ?? '',
        ]),
        footerLines: snap.cloudSession
          ? [CLOUD_EDIT_LINE]
          : [
              CONFLICT_LINE,
              'To edit local configs: modify ~/.kiro/powers/installed.json or the power files in ~/.kiro/powers/installed/<power-name>',
              CLOUD_EDIT_LINE,
            ],
        emptyMessage: 'No powers configured.',
      };
    case 'env': {
      return {
        title: '/config — environment variables',
        columns: ['Name', 'Value'],
        rows: snap.kiroEnv.map(([k, v]) => [k, v]),
        footerLines: snap.cloudSession
          ? [CLOUD_EDIT_LINE]
          : [
              'To edit local configs: set them in your shell profile or prefix them when launching the CLI',
            ],
        emptyMessage: 'No KIRO_* environment variables set.',
      };
    }
    default:
      // mcp/hooks/secrets never render as in-panel pages.
      return {
        title: `/config — ${category}`,
        columns: [],
        rows: [],
        footerLines: [],
        emptyMessage: 'Nothing to show.',
      };
  }
}

/** KIRO_*-prefixed env pairs from a process env, sorted, values truncated. */
/**
 * Env var names whose values are secrets and must never render. Matched as
 * substrings of the KIRO_-prefixed name so future variants (e.g. a
 * *_TOKEN_FILE sibling or provider-scoped *_SECRET) stay covered without a
 * list update. KIRO_API_KEY is the documented non-interactive auth
 * credential; printing it would leak a live key into scrollback and
 * anything capturing the terminal.
 */
const SECRET_ENV_MARKERS = ['KEY', 'TOKEN', 'SECRET', 'PASSWORD', 'CREDENTIAL'];

function isSecretEnvName(name: string): boolean {
  const upper = name.toUpperCase();
  return SECRET_ENV_MARKERS.some((marker) => upper.includes(marker));
}

export function collectKiroEnv(
  env: Record<string, string | undefined>
): Array<[string, string]> {
  return Object.entries(env)
    .filter((e): e is [string, string] =>
      Boolean(e[0].startsWith('KIRO_') && e[1] !== undefined)
    )
    .map(([k, v]): [string, string] => [
      k,
      isSecretEnvName(k)
        ? '<redacted>'
        : // Grapheme-aware truncation — a naive slice can split a surrogate pair.
          truncateToWidth(v, 60, '...'),
    ])
    .sort((a, b) => a[0].localeCompare(b[0]));
}
