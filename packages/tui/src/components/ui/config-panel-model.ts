/**
 * Pure row/routing model for the `/config` overlay.
 *
 * `/config` is the consolidated view of everything that shapes a session:
 * agents, MCP servers, powers, steering, skills, hooks, env variables and
 * secrets. The panel itself is a thin shell; category rows, per-category
 * pages, footers and enter-routing all live here as pure functions so they
 * are unit-testable.
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

/** Where a config item is defined. */
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
  /**
   * Whether Source columns would convey a config-origin FACT rather than a
   * placement guess. True when any snapshot item carries a cloud origin
   * (descriptor-derived) or the session itself is cloud (placement 'cloud'
   * is genuinely true there). False on V2 (no origins reported at all) AND
   * on a descriptor-free local KAS session — either way the column would
   * read 'local' on every row, which is a guess, not information.
   */
  sourcesReported: boolean;
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
  // 'env' and 'secrets' deliberately absent: both are deferred (no row, no
  // page). KAS propagates no source for either — env is the local CLI
  // process's own vars, misleading in a cloud session — so the typed form
  // gets the honest "Unknown config category" alert rather than silently
  // showing local-only data. The ids stay in the type so each can return
  // once KAS reports it.
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
 * Effective source of a config item. Per UX: in a CLOUD session everything
 * reads "cloud" — the whole config surface (bundled, sandbox user/workspace
 * files, synced resources) lives in the cloud sandbox, so distinguishing
 * KAS origins there is noise. In a LOCAL session the per-item descriptor
 * origin decides: `cloud` (synced from cloud settings) → "cloud",
 * everything else (bundled/client/user/workspace) → "local"; placement is
 * the fallback for items without a descriptor.
 */
export function effectiveSource(
  cloudSession: boolean,
  explicit?: ConfigSource
): ConfigSource {
  if (cloudSession) return 'cloud';
  return explicit ?? 'local';
}

function count(n: number, noun: string): string {
  return `${n} ${noun}`;
}

/** Build the top-level category rows. */
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
    // 'env' and 'secrets' deliberately absent: KAS propagates no source for
    // either. Env is only the local CLI process's own KIRO_* vars — the same
    // values in local and cloud sessions, never the sandbox's — so a row
    // that always reads "local" is misleading in a cloud session. Both ids
    // stay in the type so each returns with a row (and a page) once KAS
    // reports it.
  ];
}

/**
 * Footer lines under the category table. A `/config upload` sentence is
 * intentionally omitted until that command exists.
 *
 * Cloud-config sync diagnostics (`_kiro/diagnostics/changed`) lead the
 * footer when present — a stale/unavailable sync is the first thing a user
 * needs to see. An empty set adds nothing (it is not a health claim).
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
        'Cloud configs apply to cloud sessions; local configs apply to this machine',
      ];
  return [...diagLines, ...base];
}

/** What selecting a category does. */
export type ConfigSelectResult =
  // Open the shared /mcp panel — /config mcp and /mcp are the same view.
  | { kind: 'open-mcp' }
  // Open the existing /hooks panel.
  | { kind: 'open-hooks' }
  // Open the /agent picker — selectable, not a read-only page.
  | { kind: 'open-agent' }
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
    case 'agents':
      // The agents row opens the same selectable picker as /agent, not a
      // view-only inventory page.
      return { kind: 'open-agent' };
    case 'env':
    case 'secrets':
      // Deferred categories: no row, no alias, and — should either id ever
      // reach here through a reintroduced row — no blank page.
      return { kind: 'none' };
    default:
      return { kind: 'page', category: id };
  }
}

/**
 * Whether Source columns would state config-origin FACTS for this data:
 * true when any item carries a descriptor-derived source, or the session is
 * cloud (placement 'cloud' is true there — the session's config genuinely
 * comes from the cloud replica). A descriptor-free LOCAL session — V2, and
 * older/descriptor-less KAS — would render 'local' on every row from
 * placement alone: a guess, not information, so the columns are dropped.
 *
 * Deliberately SNAPSHOT-wide, not per-category: once any surface proves a
 * descriptor pipeline exists, every page keeps its column (cells without a
 * descriptor then read placement, which is meaningful next to real cloud
 * rows). Per-category gating would make the column appear and vanish
 * between pages of one open panel.
 */
export function snapshotReportsSources(
  snap: Omit<ConfigSnapshot, 'sourcesReported'>
): boolean {
  if (snap.cloudSession) return true;
  return (
    snap.agents.some((a) => a.configSource) ||
    snap.mcpServers.some((s) => s.source) ||
    snap.powers.some((p) => p.configSource) ||
    snap.steeringDocs.some((d) => d.configSource) ||
    snap.steering.some((s) => s.configSource) ||
    snap.skills.some((s) => s.configSource) ||
    snap.hooks.some((h) => h.configSource)
  );
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
 * Drop a page's Source column when no config origins are reported. A
 * placement-only column would read "local" on every row — a guess, not a
 * fact.
 */
function withoutSourceColumn(page: ConfigPage): ConfigPage {
  const idx = page.columns.indexOf('Source');
  if (idx === -1) return page;
  return {
    ...page,
    columns: page.columns.filter((_, i) => i !== idx),
    rows: page.rows.map((r) => r.filter((_, i) => i !== idx)),
  };
}

/**
 * Build a category page. MCP and hooks never reach here — selecting them
 * routes to their existing panels instead.
 */
export function buildCategoryPage(
  category: ConfigCategoryId,
  snap: ConfigSnapshot
): ConfigPage {
  const page = buildCategoryPageColumns(category, snap);
  return snap.sourcesReported ? page : withoutSourceColumn(page);
}

function buildCategoryPageColumns(
  category: ConfigCategoryId,
  snap: ConfigSnapshot
): ConfigPage {
  const src = (explicit?: ConfigSource) =>
    effectiveSource(snap.cloudSession, explicit);
  // Footer set follows the Source column: a page that reports no origins
  // must not explain how origins conflict. The local-edit path stays (an
  // engine-independent fact); the conflict/cloud-edit lines go with the
  // column.
  const footer = (localEditLine?: string): string[] =>
    snap.cloudSession
      ? [CLOUD_EDIT_LINE]
      : snap.sourcesReported
        ? [
            CONFLICT_LINE,
            ...(localEditLine ? [localEditLine] : []),
            CLOUD_EDIT_LINE,
          ]
        : localEditLine
          ? [localEditLine]
          : [];
  switch (category) {
    // 'agents' never renders an in-panel page: selecting the row opens the
    // selectable /agent picker instead.
    case 'steering':
      return {
        title: '/config — steering',
        columns: ['Name', 'Source', 'Inclusion'],
        // Prefer the documents_changed listing: it carries the real
        // inclusion mode (always/manual/fileMatch). The slash-command
        // projection is the fallback (older KAS / V2); it has no inclusion
        // fact, so those rows show the scope kind instead.
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
        footerLines: footer(
          'To edit local configs: modify ~/.kiro/steering/ (user-level) or .kiro/steering/ (workspace-level)'
        ),
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
        footerLines: footer(
          'To edit local configs: modify ~/.kiro/skills/ (user-level) or .kiro/skills/ (workspace-level)'
        ),
        emptyMessage: 'No skills configured.',
      };
    case 'powers':
      return {
        title: '/config — powers',
        columns: ['Name', 'Source', 'Description'],
        // Installed powers from _kiro/powers/items_changed.
        rows: snap.powers.map((p) => [
          p.displayName ?? p.name,
          src(p.configSource),
          p.description ?? '',
        ]),
        footerLines: footer(
          'To edit local configs: modify ~/.kiro/powers/installed.json or the power files in ~/.kiro/powers/installed/<power-name>'
        ),
        emptyMessage: 'No powers configured.',
      };
    default:
      // mcp/hooks and the deferred env/secrets never render in-panel pages.
      return {
        title: `/config — ${category}`,
        columns: [],
        rows: [],
        footerLines: [],
        emptyMessage: 'Nothing to show.',
      };
  }
}
