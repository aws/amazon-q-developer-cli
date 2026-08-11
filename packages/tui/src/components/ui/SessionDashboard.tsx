/**
 * SessionDashboard — session browser for the KAS engine.
 *
 * A grouped, searchable table of every session across workspaces and
 * stores: type-to-search (FTS over titles and prompts), filter and group
 * controls, per-row actions (resume, rename, tag, bookmark,
 * delete), a bulk-cleanup review for empty sessions, and an optional
 * preview pane. Hosted full-screen by the dashboard screen or as an
 * overlay panel.
 */
import { statSync } from 'node:fs';
import { homedir } from 'node:os';
import { sep } from 'node:path';
import React, {
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  useCallback,
  useRef,
} from 'react';
import { Box, CURSOR_MARKER } from '../../renderer.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { themeHex, getTerminalChalkColor } from '../../utils/colorUtils.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import {
  padToWidth,
  padToWidthRight,
  truncateToWidth,
  truncateToWidthTail,
  visibleWidth,
} from '../../utils/text-width.js';
import { formatRelativeTimeShort } from '../../utils/sessions.js';
import {
  sanitizeSessionTitleForDisplay,
  stripTerminalEscapes,
} from '../../utils/sanitize-title.js';
import {
  filterSessionsByText,
  applySubagentNesting,
  workspaceLabel,
  sessionIdentityKey,
  sessionMatchesActive,
  type WorkspaceGroup,
  type SessionDashboardEntry,
  type SessionListingInput,
} from '../../utils/session-dashboard.js';
import {
  groupSessions,
  sessionStatusDisplay,
  type GroupByDimension,
} from '../../utils/session-grouping.js';
import { getSessionSearchIndex } from '../../utils/session-search.js';
import {
  enrichSessions,
  sessionEnrichmentKey,
  type EnrichmentSource,
} from '../../utils/session-enrichment.js';
import { packScrollWindow } from '../../utils/session-dashboard-window.js';
import {
  buildNavModel,
  type DisplayItem,
} from '../../utils/session-dashboard-nav.js';
import { MIN_CONTENT_QUERY } from '../../utils/session-content-index.js';
import {
  getSessionPreviewProvider,
  type SessionPreview,
} from '../../utils/session-preview.js';
import {
  buildKasTurnTree,
  type SessionTurn,
} from '../../utils/kas-session-turns.js';
import { findKasSessionDir, sessionsRoot } from '../../utils/session-store.js';
import { useAppStore } from '../../stores/app-store.js';
import {
  getSessionBookmarkStore,
  parseTags,
} from '../../utils/session-bookmarks.js';
import {
  deleteSession,
  deleteLocalKasSessionWithAgent,
  gcScan,
  gcEmptySessions,
  type GcCandidate,
  type GcScan as GcScanResult,
} from '../../utils/session-mutations.js';
import { deleteClassicSession } from '../../utils/list-all-sessions-cli.js';
import {
  formatSessionLockOwner,
  isSessionLocked,
} from '../../utils/session-lock.js';
import { logger } from '../../utils/logger.js';
import { readCliSettings, updateCliSetting } from '../../utils/cli-settings.js';
import { Settings } from '../../constants/settings.js';
import { chalk } from '../../utils/color.js';

export interface SessionDashboardProps {
  /** Flat session list from KAS session/list (all workspaces). */
  sessions: SessionListingInput[];
  /** Current working directory for workspace highlighting. */
  currentCwd: string;
  /** ID of the currently active session. */
  activeSessionId?: string | null;
  /** Store identity of the active session, when known. */
  activeSessionEngine?: SessionListingInput['engine'];
  activeSessionSource?: SessionListingInput['source'];
  /** Hide the inline preview pane (the full-screen host renders its own
   *  wide preview beside the list, so the cramped inline one is redundant). */
  hidePreview?: boolean;
  /** Host-owned preview toggle. When set (full-screen mode), Tab calls this
   *  instead of toggling the inline pane. */
  onTogglePreview?: () => void;
  /** Host-owned preview view cycling: Shift+Tab calls this when set, so the
   *  wide pane's view actually changes (the inline tabs are not rendered
   *  in full-screen mode). */
  onCyclePreviewView?: () => void;
  /**
   * Rendering width in columns. Defaults to the full terminal width
   * (overlay mode). The full-screen host passes its list-column width.
   */
  width?: number;
  /**
   * Called when the user loads a session. `targetCwd` is the session's
   * workspace when it differs from the current directory (a cross-workspace
   * load that should switch the working directory); omitted otherwise.
   */
  onSelect: (
    sessionId: string,
    environment: 'local' | 'cloud',
    targetCwd?: string,
    meta?: {
      via: 'search' | 'browse';
      engine: 'classic' | 'v2' | 'v3';
    }
  ) => void;
  /** Called after a delete/GC so the owner can refresh the session listing. */
  onRefresh?: () => void;
  /** Called when user closes the dashboard. */
  onClose: () => void;
  /** True while the background scan is still merging sessions. */
  isRefreshing?: boolean;
  /** True once listing/catalog I/O has finished and background walks may run. */
  backgroundReady?: boolean;
  /** True when any listing source returned only a partial catalog. */
  catalogIncomplete?: boolean;
}

type PreviewTab = 'summary' | 'messages' | 'turns';

// Shared column widths — header and data rows must derive from the same
// constants or the labels drift off their columns.
const COL_MSGS_W = 5;
const COL_AGE_W = 9; // fits the 'Last Used' header label
const COL_STATUS_W = 8;
const COL_ID_W = 8;

function dashboardSearchKey(
  sessionId: string,
  engine: SessionListingInput['engine'],
  source?: SessionListingInput['source']
): string {
  return sessionIdentityKey({ sessionId, engine, source });
}
const COL_WS_W = 18;
const PREVIEW_MAX_LINES = 6;

function dashboardColumns(contentW: number, groupBy: GroupByDimension) {
  const showStatus = contentW >= 24;
  const showAge = contentW >= 35;
  const showMessages = contentW >= 42;
  return {
    showStatus,
    showAge,
    showMessages,
    showId: contentW >= 90,
    showWorkspace: contentW >= 110 && groupBy !== 'workspace',
  };
}

/** Normalize a path for cross-workspace comparison (drop trailing slashes). */
function normalizeWs(p: string): string {
  return (p || '').replace(/\/+$/, '');
}

function directoryExists(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

/** Compact display id: strips store prefixes, keeps the leading segment. */
function shortSessionId(id: string): string {
  return id.replace(/^(sess_|cli_)/, '').slice(0, COL_ID_W);
}

const DASHBOARD_GROUPS: readonly GroupByDimension[] = [
  'workspace',
  'recency',
  'status',
  'none',
];

function parseDashboardGroupBy(value: unknown): GroupByDimension {
  return typeof value === 'string' &&
    DASHBOARD_GROUPS.includes(value as GroupByDimension)
    ? (value as GroupByDimension)
    : 'workspace';
}

function deleteLastWord(value: string): string {
  let end = value.length;
  while (end > 0 && /\s/.test(value[end - 1]!)) end--;
  while (end > 0 && !/\s/.test(value[end - 1]!)) end--;
  return value.slice(0, end);
}

function pastedLine(input: string): string {
  return Array.from(input.split(/\r?\n/, 1)[0] ?? '')
    .filter((c) => {
      const code = c.charCodeAt(0);
      return code >= 32 && code !== 127;
    })
    .join('');
}

/** Footer action hints wrapped to the terminal width (never truncated).
 *  Pure so the scroll-window budget can count the SAME lines the footer
 *  renders. */
function wrapFooterHints(
  glyphs: ReturnType<typeof useGlyphs>,
  termWidth: number,
  hostPreview: boolean
): string[] {
  const dot = ` ${glyphs.smallDot} `;
  const hints = [
    `${glyphs.arrowUp}${glyphs.arrowDown} navigate`,
    `${glyphs.arrowLeft}${glyphs.arrowRight} groups`,
    `${glyphs.enter} resume`,
    hostPreview ? 'tab/ctrl+p preview' : 'tab preview',
    'ctrl+r rename',
    'ctrl+b bookmark',
    'ctrl+t tag',
    'ctrl+d delete',
  ];
  const maxW = Math.max(termWidth - 2, 1);
  const lines: string[] = [];
  let cur = '';
  for (const rawHint of hints) {
    const h = truncateToWidth(rawHint, maxW);
    const next = cur ? cur + dot + h : h;
    if (visibleWidth(next) > maxW && cur) {
      lines.push(cur);
      cur = h;
    } else {
      cur = next;
    }
  }
  if (cur) lines.push(cur);
  return lines;
}

export const SessionDashboard: React.FC<SessionDashboardProps> = ({
  sessions,
  currentCwd,
  activeSessionId,
  activeSessionEngine,
  activeSessionSource,
  hidePreview = false,
  onTogglePreview,
  onCyclePreviewView,
  width,
  onSelect,
  onRefresh,
  onClose,
  isRefreshing = false,
  backgroundReady = true,
  catalogIncomplete = false,
}) => {
  const { getColor, colors } = useTheme();
  const glyphs = useGlyphs();
  const { width: rawTermWidth, height: termHeight } = useTerminalSize();
  const terminalTooShort = termHeight < 13;
  // Effective render width: side panel passes a fixed column budget;
  // overlay mode uses the full terminal width.
  const termWidth = width ?? rawTermWidth;

  // Hex-string forms for chalk.hex/bgHex composition. Truecolor themes
  // carry .hex; the fallbacks only fire for named-color (safe-mode) themes.
  const hexOf = (path: string, fallback: string): string =>
    themeHex(getColor, path, fallback);
  const secondaryHex = hexOf('secondary', '#808080');
  const accentHex = hexOf('accent', '#ff00ff');
  const brandHex = hexOf('brand', '#C19AFF');

  const dim = getColor('secondary');
  const selectedTextHex = hexOf('components.dashboard.selectedText', '#1a1a1a');
  const bandBackground = getTerminalChalkColor(
    colors.components.dashboard.bandBackground,
    'bg'
  );
  const bandText = getTerminalChalkColor(colors.components.dashboard.bandText);
  const bandMutedText = getTerminalChalkColor(
    colors.components.dashboard.bandMutedText
  );
  const tagBgHex = hexOf('components.dashboard.tagBackground', '#333333');
  const tagTextHex = hexOf('components.dashboard.tagText', '#bbbbbb');

  // Bookmark/tag sidecar store. metaVersion bumps on every mutation to force
  // a re-render (the store lives outside React state).
  const bookmarkStore = useMemo(() => getSessionBookmarkStore(), []);
  const [metaVersion, setMetaVersion] = useState(0);
  const bumpMeta = useCallback(() => setMetaVersion((v) => v + 1), []);
  // After a mutation reflows the list (e.g. bookmarking moves a session into
  // the pinned group), keep the cursor ON that session instead of on whatever
  // index it previously occupied — otherwise the next
  // keypress hits the wrong row.
  const pendingCursorIdRef = useRef<string | null>(null);
  const selectedCursorIdentityRef = useRef<string | null>(null);
  // Tag-edit sub-mode: while active, typed chars edit `tagDraft` (not search).
  const [tagEditing, setTagEditing] = useState(false);
  const [tagDraft, setTagDraft] = useState('');
  // Rename sub-mode: same shape; commits to the sidecar title override.
  const [renameEditing, setRenameEditing] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
  // Group-by dimension + composable filters (the control bar). Tag filtering
  // rides the existing "#tag" search path, so it's not a control-bar toggle.
  const [groupBy, setGroupByState] = useState<GroupByDimension>(() =>
    parseDashboardGroupBy(
      readCliSettings()[Settings.CHAT_SESSION_DASHBOARD_GROUP_BY]
    )
  );
  const setGroupBy = useCallback((next: GroupByDimension) => {
    setGroupByState(next);
    void updateCliSetting(Settings.CHAT_SESSION_DASHBOARD_GROUP_BY, next).catch(
      (err) => logger.warn('[dashboard] failed to persist grouping:', err)
    );
  }, []);
  const [filterMode, setFilterMode] = useState<
    'all' | 'current' | 'bookmarked' | 'empties' | 'cleanup'
  >('all');
  // Cleanup review scope: null = all workspaces; a path = the expander's group.
  const [cleanupScope, setCleanupScope] = useState<string | null>(null);
  // Which control is expanded inline (^f / ^g). While one is open, ←/→ move
  // through its options and the selection applies live; Esc/Enter collapse.
  const [controlFocus, setControlFocus] = useState<'filter' | 'group' | null>(
    null
  );
  // Inline preview visibility (Tab). The full-screen host owns its own pane
  // and passes hidePreview + onTogglePreview instead.
  const [showInlinePreview, setShowInlinePreview] = useState(true);

  const [cursor, setCursor] = useState(0);
  const [search, setSearch] = useState('');
  const [baseInputOwner, setBaseInputOwner] = useState<'list' | 'search'>(
    'list'
  );
  const searchRef = useRef('');
  const inputOwner: 'list' | 'search' | 'tag' | 'rename' | 'control' =
    tagEditing
      ? 'tag'
      : renameEditing
        ? 'rename'
        : controlFocus
          ? 'control'
          : baseInputOwner;
  const [previewTab, setPreviewTab] = useState<PreviewTab>('summary');
  const [searchResults, setSearchResults] = useState<Array<{
    sessionId: string;
    engine: 'v2' | 'v3';
  }> | null>(null);
  // Match context per result id — shown in the preview so a hit explains itself.
  const [searchSnippets, setSearchSnippets] = useState<Map<string, string>>(
    () => new Map()
  );
  const [indexStatus, setIndexStatus] = useState<'idle' | 'indexing' | 'ready'>(
    'idle'
  );
  // A cross-workspace load stages here first so the user can confirm the
  // directory switch before it happens.
  const [pendingSwitch, setPendingSwitch] = useState<{
    sessionId: string;
    environment: 'local' | 'cloud';
    workspace: string;
    engine: 'classic' | 'v2' | 'v3';
  } | null>(null);
  // Staged destructive action (Ctrl+D): single-session delete, or a GC of a
  // workspace's empty sessions (staged from its "+ N more" expander row).
  const [pendingDelete, setPendingDelete] = useState<
    | {
        kind: 'session';
        sessionId: string;
        title: string;
        source?: 'local' | 'remote';
        cloud?: boolean;
        engine?: 'classic' | 'v2' | 'v3';
      }
    | {
        kind: 'gc';
        workspace: string;
        candidates: GcCandidate[];
        skipped: GcScanResult['skipped'];
      }
    | null
  >(null);
  const hasModal = pendingDelete !== null || pendingSwitch !== null;
  const [mutationNotice, setMutationNotice] = useState<{
    text: string;
    tone: 'success' | 'warning' | 'error';
  } | null>(null);
  useEffect(() => {
    if (!mutationNotice) return;
    const timer = setTimeout(() => setMutationNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [mutationNotice]);
  // Global GC scan, computed off the render path when the dashboard opens
  // and after every mutation. Feeds the "gc: N empty sessions" footer row
  // and lets the workspace expander stage GC without a synchronous rescan.
  const [globalGc, setGlobalGc] = useState<GcScanResult | null>(null);
  // One scan per dashboard open — NOT re-run on metaVersion bumps.
  // Marking a session (bookmark/tag/rename) exempts it from GC via the
  // userTouched set at STAGE time; recomputing the whole disk scan for a
  // footer count froze input for seconds on large stores.
  useEffect(() => {
    if (!backgroundReady) return;
    let cancelled = false;
    void (async () => {
      await getSessionSearchIndex().refresh();
      if (cancelled) return;
      const scan = await gcScan(
        activeSessionId,
        new Set(bookmarkStore.allUserTouched())
      );
      if (!cancelled) setGlobalGc(scan);
    })();
    return () => {
      cancelled = true;
    };
  }, [activeSessionId, backgroundReady, bookmarkStore]);

  // Title enrichment: KAS's listing reports "New Session" for sessions that
  // never persisted a title. The background search index has already read each
  // session's JSONL and derived a first-prompt title — swap it in once ready.
  // Also collect which sessions have an empty log (zero entries) so the list
  // can sink them behind the "show more" expander.
  const { enrichedSessions, emptyIds } = useMemo(() => {
    const indexReady = indexStatus === 'ready' || indexStatus === 'indexing';
    const index = indexReady ? getSessionSearchIndex() : null;
    const source: EnrichmentSource | null = index
      ? {
          getDocument: (id, engine) => index.getDocument(id, engine),
          isPromptless: (id, engine) => index.isPromptless(id, engine),
          getPromptTitle: (id, engine) => index.getPromptTitle(id, engine),
          getPromptCount: (id, engine) => index.getPromptCount(id, engine),
          canInferNeverIndexed:
            indexStatus === 'ready' &&
            index.getCoverage() === 'titles-and-prompts',
        }
      : null;
    return enrichSessions(
      sessions,
      activeSessionId,
      (id) => bookmarkStore.getTitle(id),
      source,
      activeSessionEngine,
      activeSessionSource
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    sessions,
    indexStatus,
    activeSessionId,
    activeSessionEngine,
    activeSessionSource,
    bookmarkStore,
    metaVersion,
  ]);

  // Status grouping is only meaningful while some row carries a status.
  const hasStatusData = useMemo(
    () => enrichedSessions.some((s) => s.status != null),
    [enrichedSessions]
  );

  // Build groups under the chosen dimension + filters.
  const baseGroups = useMemo(() => {
    // Cleanup review: the list shows exactly what the bulk delete
    // would remove — the candidates from the same scan the delete will use.
    // Marking a row (bookmark/tag/rename) exempts it from the scan, so the
    // review shrinks live as the user "unmarks" rows they want to keep.
    if (filterMode === 'cleanup') {
      const candidateIdentities = new Set(
        (globalGc?.candidates ?? [])
          .filter((c) => !cleanupScope || c.workspace === cleanupScope)
          .map((c) =>
            sessionIdentityKey({
              sessionId: c.sessionId,
              engine: c.store === 'kas' ? 'v3' : 'v2',
              source: 'local',
            })
          )
      );
      const candidates = enrichedSessions.filter((s) =>
        candidateIdentities.has(sessionIdentityKey(s))
      );
      return groupSessions(candidates, currentCwd, {
        groupBy: 'workspace',
        filters: {
          currentWorkspaceOnly: false,
          bookmarkedOnly: false,
          tag: null,
        },
        activeSessionId,
        activeEngine: activeSessionEngine,
        activeSource: activeSessionSource,
        meta: {
          isBookmarked: (id: string) => bookmarkStore.isBookmarked(id),
          getTags: (id: string) => bookmarkStore.getTags(id),
        },
      });
    }
    // Archive is shelved: the archived flag is ignored so previously
    // archived rows stay visible rather than being hidden with no way back.
    const visibleSessions = enrichedSessions;
    const meta = {
      isBookmarked: (id: string) => bookmarkStore.isBookmarked(id),
      getTags: (id: string) => bookmarkStore.getTags(id),
    };
    const grouped = applySubagentNesting(
      groupSessions(visibleSessions, currentCwd, {
        groupBy,
        filters: {
          currentWorkspaceOnly: filterMode === 'current',
          bookmarkedOnly: filterMode === 'bookmarked',
          tag: null,
        },
        activeSessionId,
        activeEngine: activeSessionEngine,
        activeSource: activeSessionSource,
        meta,
      })
    );
    // Pinned Bookmarked group only makes sense in the workspace view, and is
    // redundant once the bookmarked FILTER is on.
    if (groupBy !== 'workspace' || filterMode === 'bookmarked') {
      return grouped;
    }
    if (bookmarkStore.allBookmarked().length === 0) return grouped;
    const pinned: SessionDashboardEntry[] = [];
    for (const group of grouped) {
      group.sessions = group.sessions.filter((s) => {
        if (bookmarkStore.isBookmarked(s.sessionId)) {
          pinned.push(s);
          return false;
        }
        return true;
      });
    }
    if (pinned.length === 0) return grouped;
    const pinnedGroup: WorkspaceGroup = {
      workspace: '\u0000bookmarked', // sentinel key, never a real path
      label: 'Bookmarked',
      isCurrent: false,
      sessions: pinned,
    };
    return [pinnedGroup, ...grouped.filter((g) => g.sessions.length > 0)];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    enrichedSessions,
    currentCwd,
    activeSessionId,
    activeSessionEngine,
    activeSessionSource,
    bookmarkStore,
    metaVersion,
    groupBy,
    filterMode,
    globalGc,
    cleanupScope,
  ]);

  useEffect(() => {
    const cloudGroup = baseGroups.find(
      (group) => group.workspace === '\u0000cloud'
    );
    logger.info('[sessions-debug] dashboard grouping', {
      groupBy,
      filterMode,
      groupCount: baseGroups.length,
      firstGroup: baseGroups[0]?.label ?? null,
      cloudGroupIndex: cloudGroup ? baseGroups.indexOf(cloudGroup) : -1,
      cloudRowCount: cloudGroup?.sessions.length ?? 0,
      cloudSessionIds: cloudGroup?.sessions.map((s) => s.sessionId) ?? [],
    });
  }, [baseGroups, filterMode, groupBy]);

  const BOOKMARK_GROUP_KEY = '\u0000bookmarked';

  // Query-independent grouping bases for search — hoisted so a keystroke
  // only re-runs the cheap substring/tag filter, not a full 10K re-group.
  const searchBases = useMemo(() => {
    const meta = {
      isBookmarked: (id: string) => bookmarkStore.isBookmarked(id),
      getTags: (id: string) => bookmarkStore.getTags(id),
    };
    const noFilters = {
      currentWorkspaceOnly: false,
      bookmarkedOnly: false,
      tag: null,
    };
    const flat = groupSessions(enrichedSessions, currentCwd, {
      groupBy: 'none',
      filters: noFilters,
      activeSessionId,
      activeEngine: activeSessionEngine,
      activeSource: activeSessionSource,
      meta,
    });
    const byIdentity = new Map(
      flat.flatMap((g) =>
        g.sessions.map(
          (s) =>
            [dashboardSearchKey(s.sessionId, s.engine, s.source), s] as const
        )
      )
    );
    const grouped = groupSessions(enrichedSessions, currentCwd, {
      groupBy,
      filters: noFilters,
      activeSessionId,
      activeEngine: activeSessionEngine,
      activeSource: activeSessionSource,
      meta,
    });
    return { flat, byIdentity, grouped };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    enrichedSessions,
    currentCwd,
    groupBy,
    activeSessionId,
    activeSessionEngine,
    activeSessionSource,
    bookmarkStore,
    metaVersion,
  ]);

  // Apply text filter or search results.
  const filteredGroups = useMemo(() => {
    if (searchResults) {
      // Content-search results: one flat group in rank order, built from the
      // full enriched listing rather than the filtered groups — search
      // ignores every filter, because a match you cannot see is worse than
      // no match.
      const { flat, byIdentity } = searchBases;
      const ranked = searchResults
        .map(({ sessionId, engine }) =>
          byIdentity.get(dashboardSearchKey(sessionId, engine))
        )
        .filter((s): s is NonNullable<typeof s> => s !== undefined);
      // A search hit on a subagent/derived session surfaces its PARENT
      // instead — the derived session lives in the parent's preview pane,
      // not the master list.
      const resolvedRanked: SessionDashboardEntry[] = [];
      const seenIdentities = new Set<string>();
      for (const s of ranked) {
        const identity = sessionIdentityKey(s);
        if (s.isSubagent && s.parentSessionId) {
          const parent = byIdentity.get(
            dashboardSearchKey(s.parentSessionId, s.engine, s.source)
          );
          const parentIdentity = parent ? sessionIdentityKey(parent) : null;
          if (parent && parentIdentity && !seenIdentities.has(parentIdentity)) {
            resolvedRanked.push(parent);
            seenIdentities.add(parentIdentity);
          }
        } else if (s.isSubagent) {
          // Orphan subagent (parent not in listing) — skip rather than
          // surfacing a row that belongs in a preview pane.
        } else if (!seenIdentities.has(identity)) {
          resolvedRanked.push(s);
          seenIdentities.add(identity);
        }
      }
      // Rows omitted from the local content index (classic and remote KAS)
      // still participate through title matching below ranked prompt hits.
      // Subagent/derived title hits resolve to their parent row exactly like
      // ranked hits — the derived session belongs in the preview pane, never
      // the master list.
      const q = search.trim().toLowerCase();
      const titleOnly: SessionDashboardEntry[] = [];
      if (q) {
        for (const s of flat.flatMap((g) => g.sessions)) {
          if (!s.title.toLowerCase().includes(q)) continue;
          const target =
            s.isSubagent && s.parentSessionId
              ? byIdentity.get(
                  dashboardSearchKey(s.parentSessionId, s.engine, s.source)
                )
              : s.isSubagent
                ? undefined
                : s;
          if (!target) continue;
          const identity = sessionIdentityKey(target);
          if (seenIdentities.has(identity)) continue;
          seenIdentities.add(identity);
          titleOnly.push(target);
        }
      }
      const combined = [...resolvedRanked, ...titleOnly];
      if (combined.length > 0) {
        if (groupBy === 'none') {
          return [
            {
              workspace: '\u0000search',
              label: 'Results',
              isCurrent: false,
              sessions: combined,
            },
          ];
        }
        // Search ignores filters but keeps the active grouping: bucket the
        // hits into the unfiltered grouped view, rank-ordered within each
        // group so relevance still reads top-down.
        const rank = new Map(
          combined.map((s, i) => [sessionIdentityKey(s), i] as const)
        );
        const groupedHits: WorkspaceGroup[] = [];
        for (const group of searchBases.grouped) {
          const matching = group.sessions
            .filter((s) => rank.has(sessionIdentityKey(s)))
            .sort(
              (a, b) =>
                rank.get(sessionIdentityKey(a))! -
                rank.get(sessionIdentityKey(b))!
            );
          if (matching.length > 0) {
            groupedHits.push({ ...group, sessions: matching });
          }
        }
        if (groupedHits.length > 0) return groupedHits;
      }
      // No intersection with the listing (the index covers all workspaces
      // but the listing may be narrower) — fall through to the text filter.
    }
    // Text filter, extended to match tags: a session matches if its title/
    // workspace/id matches (filterSessionsByText) OR any of its tags
    // contains the query. Supports `#tag` queries too (leading # ignored).
    // The title floor also ignores filters: it runs over groups built from
    // every session, not the filtered view.
    const q = search.trim().toLowerCase().replace(/^#/, '');
    if (!q) return baseGroups;
    const searchBase = searchBases.grouped;
    const byText = filterSessionsByText(searchBase, search);
    const textIdentities = new Set(
      byText.flatMap((g) => g.sessions.map((s) => sessionIdentityKey(s)))
    );
    const result: WorkspaceGroup[] = [];
    for (const group of searchBase) {
      const matching = group.sessions.filter(
        (s) =>
          !s.isSubagent &&
          (textIdentities.has(sessionIdentityKey(s)) ||
            bookmarkStore.getTags(s.sessionId).some((t) => t.includes(q)))
      );
      if (matching.length > 0) result.push({ ...group, sessions: matching });
    }
    return result;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    baseGroups,
    searchBases,
    search,
    searchResults,
    bookmarkStore,
    metaVersion,
  ]);

  // Per-group collapse: groups render a capped number of rows with an
  // expandable "more" row; ← collapses an expanded group again. Searching
  // shows all matches (no collapse).
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    () => new Set()
  );
  const isSearching = search.trim().length > 0 || searchResults !== null;

  // The single row-visibility rule shared by the list, group-header counts,
  // and the footer total — so they can never disagree. Empty sessions show
  // only in the `+empty`/cleanup modes; orphan subagents (no parent row to
  // nest under) are hidden unless they are themselves empty in those modes,
  // where they count as GC candidates.
  const isRowVisible = useCallback(
    (s: SessionDashboardEntry) => {
      const showEmpties = filterMode === 'empties' || filterMode === 'cleanup';
      return emptyIds.has(sessionEnrichmentKey(s.sessionId, s.engine, s.source))
        ? showEmpties
        : !(s.isSubagent && !s.parentSessionId);
    },
    [emptyIds, filterMode]
  );

  const {
    navItems,
    displayItems,
    groupStarts,
    groupKeys,
    navGroupIdx,
    eligibleTotal,
  } = useMemo(
    () =>
      buildNavModel({
        groups: filteredGroups,
        isRowVisible,
        expandedGroups,
        isSearching,
        groupBy,
        pinnedGroupKey: BOOKMARK_GROUP_KEY,
      }),
    [filteredGroups, expandedGroups, isSearching, isRowVisible, groupBy]
  );

  // Keep the cursor on the same physical session whenever an async catalog
  // refresh, search result, or metadata mutation reorders the navigation model.
  useLayoutEffect(() => {
    const identity =
      pendingCursorIdRef.current ?? selectedCursorIdentityRef.current;
    pendingCursorIdRef.current = null;
    setCursor((current) => {
      if (identity) {
        const anchored = navItems.findIndex(
          (item) =>
            item.type === 'session' &&
            sessionIdentityKey(item.entry) === identity
        );
        if (anchored >= 0) return anchored;
      }
      return Math.min(current, Math.max(0, navItems.length - 1));
    });
  }, [navItems]);

  // Current selected item / session.
  const selectedItem = navItems[cursor] ?? null;
  const selectedSession =
    selectedItem?.type === 'session' ? selectedItem.entry : null;
  const selectedSessionId = selectedSession?.sessionId ?? null;
  const selectedSessionIdentity = selectedSession
    ? sessionIdentityKey(selectedSession)
    : null;
  useLayoutEffect(() => {
    selectedCursorIdentityRef.current = selectedSessionIdentity;
  }, [selectedSessionIdentity]);

  // Mirror the highlighted session to the store so the full-screen mode's
  // wide preview pane can follow the cursor.
  const setHighlighted = useAppStore((s) => s.setDashboardHighlightedSession);
  // Session lifecycle manager, for agent-routed delete/rename RPCs.
  const kiro = useAppStore((s) => s.kiro);
  useEffect(() => {
    setHighlighted(
      selectedSession
        ? {
            sessionId: selectedSession.sessionId,
            engine: selectedSession.engine,
            source: selectedSession.source,
          }
        : null
    );
  }, [selectedSession, setHighlighted]);

  // Preview data (lazy loaded).
  const [previewState, setPreviewState] = useState<{
    identity: string;
    preview: SessionPreview | null;
  } | null>(null);
  const previewProvider = useMemo(() => getSessionPreviewProvider(), []);

  useEffect(() => {
    if (!selectedSessionId || !selectedSessionIdentity) return;
    const timer = setTimeout(() => {
      setPreviewState({
        identity: selectedSessionIdentity,
        preview: previewProvider.getPreview(
          selectedSessionId,
          selectedSession?.engine,
          selectedSession?.source
        ),
      });
    }, 150);
    return () => clearTimeout(timer);
  }, [
    selectedSessionId,
    selectedSessionIdentity,
    selectedSession?.engine,
    selectedSession?.source,
    previewProvider,
  ]);
  const preview =
    previewState?.identity === selectedSessionIdentity
      ? previewState.preview
      : null;

  // KAS-native turn tree (lazy, debounced — building it walks the store and
  // reads every sub-execution file). Only KAS-native sessions have a
  // sub-executions/ dir; V2 sessions resolve to null and the Turns tab
  // falls back to a notice.
  const [turnTreeState, setTurnTreeState] = useState<{
    identity: string;
    turns: SessionTurn[];
  } | null>(null);
  useEffect(() => {
    if (
      !selectedSessionId ||
      !selectedSessionIdentity ||
      previewTab !== 'turns'
    ) {
      return;
    }
    const timer = setTimeout(() => {
      const root = sessionsRoot();
      const dir =
        selectedSession?.source !== 'remote' &&
        (selectedSession?.engine === 'v3' ||
          selectedSession?.engine === undefined)
          ? findKasSessionDir(root, selectedSessionId)
          : null;
      setTurnTreeState({
        identity: selectedSessionIdentity,
        turns: dir ? buildKasTurnTree(dir) : [],
      });
    }, 150);
    return () => clearTimeout(timer);
  }, [
    selectedSessionId,
    selectedSessionIdentity,
    selectedSession?.engine,
    selectedSession?.source,
    previewTab,
  ]);
  const turnTree =
    turnTreeState?.identity === selectedSessionIdentity
      ? turnTreeState.turns
      : null;

  // Search index lifecycle: trigger build on mount and track status.
  useEffect(() => {
    const index = getSessionSearchIndex();
    const unsub = index.onStatusChange((s) => {
      setIndexStatus(
        s.state === 'ready'
          ? 'ready'
          : s.state === 'indexing'
            ? 'indexing'
            : 'idle'
      );
    });
    const currentStatus = index.getStatus();
    setIndexStatus(
      currentStatus.state === 'ready'
        ? 'ready'
        : currentStatus.state === 'indexing'
          ? 'indexing'
          : 'idle'
    );
    return unsub;
  }, []);

  // Debounced semantic search.
  const searchTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null
  );

  const runIndexSearch = useCallback((query: string) => {
    const results = getSessionSearchIndex().search(query, 20);
    setSearchResults(
      results.length > 0
        ? results.map((result) => ({
            sessionId: result.sessionId,
            engine: result.engine,
          }))
        : null
    );
    setSearchSnippets(
      new Map(
        results.map((result) => [
          dashboardSearchKey(result.sessionId, result.engine),
          stripTerminalEscapes(`${result.matchField}: ${result.snippet}`),
        ])
      )
    );
  }, []);

  const handleSearchChange = useCallback(
    (query: string) => {
      searchRef.current = query;
      setSearch(query);

      // Clear pending debounce.
      if (searchTimerRef.current) {
        clearTimeout(searchTimerRef.current);
        searchTimerRef.current = null;
      }

      // Empty query: reset.
      if (!query.trim()) {
        setSearchResults(null);
        setSearchSnippets(new Map());
        return;
      }

      // Short queries: text-only filter (instant). `#`-prefixed queries are
      // tag lookups — the text floor matches tags; FTS5 would tokenize away
      // the `#` and return transcript hits instead of the tagged session.
      if (
        query.trim().length < MIN_CONTENT_QUERY ||
        query.trim().startsWith('#')
      ) {
        setSearchResults(null);
        setSearchSnippets(new Map());
        return;
      }

      // 3+ chars: debounce then run semantic/index search.
      setSearchResults(null);

      searchTimerRef.current = setTimeout(() => {
        const index = getSessionSearchIndex();
        const status = index.getStatus();
        if (status.state === 'ready' || status.state === 'indexing') {
          runIndexSearch(query.trim());
        }
      }, 400); // 400ms debounce for index search.
    },
    [runIndexSearch]
  );

  useEffect(() => {
    const query = searchRef.current.trim();
    if (
      indexStatus === 'ready' &&
      query.length >= MIN_CONTENT_QUERY &&
      !query.startsWith('#')
    ) {
      runIndexSearch(query);
    }
  }, [indexStatus, runIndexSearch]);

  // Cleanup debounce timer on unmount.
  useEffect(() => {
    return () => {
      if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
    };
  }, []);

  /** Route one confirmed deletion through its physical store owner. */
  const deleteRouted = useCallback(
    async (staged: {
      sessionId: string;
      source?: 'local' | 'remote';
      cloud?: boolean;
      engine?: 'classic' | 'v2' | 'v3';
    }): Promise<{ ok: boolean; reason: string }> => {
      if (staged.engine === 'classic') {
        const r = await deleteClassicSession(staged.sessionId);
        return { ok: r.ok, reason: r.ok ? '' : (r.error ?? 'error') };
      }
      if (staged.engine === 'v2') {
        const out = deleteSession(
          staged.sessionId,
          activeSessionEngine === 'v2' && activeSessionSource !== 'remote'
            ? activeSessionId
            : null,
          undefined,
          'v2'
        );
        return { ok: out.ok, reason: out.ok ? '' : out.reason };
      }
      const isCloud = staged.cloud || staged.source === 'remote';
      if (isCloud) {
        const deleted = sessionMatchesActive(
          staged,
          activeSessionId,
          activeSessionEngine,
          activeSessionSource
        )
          ? false
          : await kiro.deleteSessionById(staged.sessionId, {
              source: 'remote',
            });
        return {
          ok: deleted,
          reason: deleted ? '' : 'cloud delete failed',
        };
      }
      const out = await deleteLocalKasSessionWithAgent(
        staged.sessionId,
        activeSessionEngine === 'v3' && activeSessionSource !== 'remote'
          ? activeSessionId
          : null,
        (sessionId) => kiro.deleteSessionById(sessionId, { source: 'local' })
      );
      return { ok: out.ok, reason: out.ok ? '' : out.reason };
    },
    [activeSessionId, activeSessionEngine, activeSessionSource, kiro]
  );

  // Keyboard handling.
  useKeypress((input, key) => {
    // Ctrl+C closes the dashboard outright, whatever sub-state is open —
    // its terminal-wide reflex meaning is "get me out of here".
    if (key.ctrl && (input === 'c' || input === 'C')) {
      onClose();
      return;
    }
    if (terminalTooShort) {
      if (
        key.escape ||
        (!key.ctrl && !key.meta && input.toLowerCase() === 'q')
      ) {
        onClose();
      }
      return;
    }
    // Staged deletion is modal, and `y` is the only key that confirms —
    // never Enter, whose reflex meaning everywhere else is "open". Any
    // other keypress cancels; nothing leaks through to search or the list.
    if (pendingDelete) {
      if (!key.ctrl && !key.meta && (input === 'y' || input === 'Y')) {
        if (pendingDelete.kind === 'session') {
          const staged = pendingDelete;
          void (async () => {
            const { ok, reason } = await deleteRouted(staged);
            if (ok) {
              getSessionSearchIndex().update(staged.sessionId, staged.engine);
              previewProvider.invalidate(staged.sessionId);
            }
            setMutationNotice({
              text: ok
                ? `Deleted "${staged.title.slice(0, 40)}"`
                : `Not deleted (${reason})`,
              tone: ok ? 'success' : 'error',
            });
            bumpMeta();
            onRefresh?.();
          })();
        } else {
          const staged = pendingDelete;
          void (async () => {
            const result = await gcEmptySessions(
              staged.candidates,
              activeSessionId,
              undefined,
              (id) => kiro.deleteSessionById(id)
            );
            const index = getSessionSearchIndex();
            for (const c of staged.candidates) {
              index.update(c.sessionId, c.store === 'kas' ? 'v3' : 'v2');
            }
            setMutationNotice({
              text:
                `GC: deleted ${result.deleted} empty session${result.deleted === 1 ? '' : 's'}` +
                (result.failed > 0 ? `, ${result.failed} skipped` : ''),
              tone: result.failed > 0 ? 'warning' : 'success',
            });
            // The review's candidate list is gone with the deletion —
            // leave the mode rather than showing an empty review.
            setFilterMode((m) => (m === 'cleanup' ? 'all' : m));
            setCleanupScope(null);
            bumpMeta();
            onRefresh?.();
          })();
        }
        setPendingDelete(null);
      } else {
        setPendingDelete(null);
      }
      return;
    }

    // The staged cross-workspace switch is modal too: Enter or y confirms
    // (opening is not destructive, so the reflex key is allowed), anything
    // else cancels. Nothing leaks into search or moves the list under the
    // staged session.
    if (pendingSwitch) {
      if (
        key.return ||
        (!key.ctrl && !key.meta && (input === 'y' || input === 'Y'))
      ) {
        const lockInfo = isSessionLocked(pendingSwitch.sessionId, {
          engine: pendingSwitch.engine,
          source: pendingSwitch.environment === 'cloud' ? 'remote' : 'local',
        });
        if (lockInfo) {
          setMutationNotice({
            text: `Cannot open: session is active in another terminal (${formatSessionLockOwner(lockInfo)})`,
            tone: 'error',
          });
          setPendingSwitch(null);
          return;
        }
        onSelect(
          pendingSwitch.sessionId,
          pendingSwitch.environment,
          pendingSwitch.workspace,
          {
            via: searchResults ? 'search' : 'browse',
            engine: pendingSwitch.engine,
          }
        );
      }
      setPendingSwitch(null);
      return;
    }

    // Tag-edit sub-mode owns all typing: Enter commits, Esc cancels.
    if (tagEditing) {
      if (key.escape) {
        setTagEditing(false);
        setTagDraft('');
        return;
      }
      if (key.return) {
        if (selectedSession) {
          const result = bookmarkStore.setTags(
            selectedSession.sessionId,
            parseTags(tagDraft)
          );
          if (result.ok) {
            bumpMeta();
          } else {
            setMutationNotice({
              text: `Tags were not saved (${result.reason})`,
              tone: 'error',
            });
          }
        }
        setTagEditing(false);
        setTagDraft('');
        return;
      }
      if (key.ctrl && (input === 'w' || input === 'W')) {
        setTagDraft((d) => deleteLastWord(d));
        return;
      }
      if (key.ctrl && (input === 'u' || input === 'U')) {
        setTagDraft('');
        return;
      }
      if (key.backspace || key.delete) {
        setTagDraft((d) => d.slice(0, -1));
        return;
      }
      if (input && !key.ctrl && !key.meta) {
        const text = pastedLine(input);
        if (text) setTagDraft((d) => d + text);
      }
      return;
    }

    // Rename sub-mode mirrors tag editing: Enter commits, Esc cancels.
    // An empty commit clears the override, falling back down the title chain.
    if (renameEditing) {
      if (key.escape) {
        setRenameEditing(false);
        setRenameDraft('');
        return;
      }
      if (key.return) {
        if (selectedSession) {
          pendingCursorIdRef.current = sessionIdentityKey(selectedSession);
          const saved = bookmarkStore.setTitle(
            selectedSession.sessionId,
            renameDraft
          );
          if (saved.ok) {
            previewProvider.invalidate(selectedSession.sessionId);
            bumpMeta();
            const clean = renameDraft.replace(/\s+/g, ' ').trim();
            if (clean && selectedSession.engine === 'v3') {
              kiro
                .renameSessionById(selectedSession.sessionId, clean, {
                  source:
                    selectedSession.source === 'remote' ? 'remote' : 'local',
                })
                .catch((err) =>
                  logger.warn('[dashboard] rename RPC failed:', err)
                );
            }
          } else {
            setMutationNotice({
              text: `Rename was not saved (${saved.reason})`,
              tone: 'error',
            });
          }
        }
        setRenameEditing(false);
        setRenameDraft('');
        return;
      }
      if (key.ctrl && (input === 'w' || input === 'W')) {
        setRenameDraft((d) => deleteLastWord(d));
        return;
      }
      if (key.ctrl && (input === 'u' || input === 'U')) {
        setRenameDraft('');
        return;
      }
      if (key.backspace || key.delete) {
        setRenameDraft((d) => d.slice(0, -1));
        return;
      }
      if (input && !key.ctrl && !key.meta) {
        const text = pastedLine(input);
        if (text) setRenameDraft((d) => d + text);
      }
      return;
    }

    // Ctrl+B toggles a bookmark on the highlighted session.
    if (key.ctrl && (input === 'b' || input === 'B')) {
      setBaseInputOwner('list');
      if (selectedSession) {
        pendingCursorIdRef.current = sessionIdentityKey(selectedSession);
        const result = bookmarkStore.toggleBookmark(selectedSession.sessionId);
        if (result.ok) {
          bumpMeta();
        } else {
          setMutationNotice({
            text: `Bookmark was not saved (${result.reason})`,
            tone: 'error',
          });
        }
      }
      return;
    }
    // Ctrl+T enters tag-edit mode, seeded with the session's current tags.
    if (key.ctrl && (input === 't' || input === 'T')) {
      if (selectedSession) {
        const existing = bookmarkStore.getTags(selectedSession.sessionId);
        setBaseInputOwner('list');
        setTagDraft(existing.join(', '));
        setTagEditing(true);
      }
      return;
    }
    // Ctrl+R enters rename mode, seeded with the current effective title.
    if (key.ctrl && (input === 'r' || input === 'R')) {
      if (selectedSession) {
        setBaseInputOwner('list');
        setRenameDraft(
          sanitizeSessionTitleForDisplay(selectedSession.title) ?? ''
        );
        setRenameEditing(true);
      }
      return;
    }
    // Ctrl+G expands the group control inline (or collapses it). While open,
    // ←/→ move through the options and the selection applies live.
    if (key.ctrl && (input === 'g' || input === 'G')) {
      setBaseInputOwner('list');
      setControlFocus((f) => (f === 'group' ? null : 'group'));
      return;
    }
    // Ctrl+F expands the filter control inline, same interaction.
    if (key.ctrl && (input === 'f' || input === 'F')) {
      setBaseInputOwner('list');
      setControlFocus((f) => (f === 'filter' ? null : 'filter'));
      return;
    }
    // Ctrl+X clears the filter back to the default view. Search is cleared
    // by Esc/backspace — the two resets are deliberately separate keys.
    if (key.ctrl && (input === 'x' || input === 'X')) {
      setBaseInputOwner('list');
      setControlFocus(null);
      setFilterMode('all');
      setCleanupScope(null);
      setCursor(0);
      return;
    }

    // While a control is expanded it owns ←/→/Enter/Esc; anything else
    // collapses it and falls through to the list.
    if (controlFocus) {
      // Status grouping is only offered while the listing carries status
      // data — a cycle stop that groups everything under "No status" is
      // noise, not an option.
      const groupOrder: GroupByDimension[] = hasStatusData
        ? ['workspace', 'recency', 'status', 'none']
        : ['workspace', 'recency', 'none'];
      const filterOrder = ['all', 'current', 'bookmarked', 'empties'] as const;
      const step = (dir: -1 | 1) => {
        setCursor(0);
        if (controlFocus === 'group') {
          const i = groupOrder.indexOf(groupBy);
          const next =
            groupOrder[(i + dir + groupOrder.length) % groupOrder.length]!;
          setGroupBy(next);
        } else {
          const i = (filterOrder as readonly string[]).indexOf(filterMode);
          const next =
            filterOrder[(i + dir + filterOrder.length) % filterOrder.length]!;
          setFilterMode(next);
        }
      };
      if (key.leftArrow) {
        step(-1);
        return;
      }
      if (key.rightArrow) {
        step(1);
        return;
      }
      if (key.return || key.escape) {
        setControlFocus(null);
        return;
      }
      // ↑/↓ hand focus back to the list and act there.
      if (key.upArrow || key.downArrow) {
        setControlFocus(null);
      }
    }

    // Ctrl+D stages a destructive action: delete (on a session row) or a
    // workspace GC of empty sessions (on the "+ N more" expander row).
    if (key.ctrl && (input === 'd' || input === 'D')) {
      setBaseInputOwner('list');
      setMutationNotice(null);
      // Inside cleanup review, ^d stages the bulk delete of exactly what is
      // shown — the review IS the candidate list.
      if (filterMode === 'cleanup') {
        // Re-apply the user-touched exemption at stage time: a row
        // bookmarked/tagged/renamed during review is no longer a candidate.
        // Also require each candidate to be present in the enriched listing —
        // the review renders that intersection, and a candidate the user
        // never saw must not be staged.
        const touched = new Set(bookmarkStore.allUserTouched());
        const listedIdentities = new Set(
          enrichedSessions.map((s) => sessionIdentityKey(s))
        );
        const candidates = (globalGc?.candidates ?? []).filter(
          (c) =>
            (!cleanupScope || c.workspace === cleanupScope) &&
            !touched.has(c.sessionId) &&
            listedIdentities.has(
              sessionIdentityKey({
                sessionId: c.sessionId,
                engine: c.store === 'kas' ? 'v3' : 'v2',
                source: 'local',
              })
            )
        );
        if (candidates.length > 0 && globalGc) {
          setPendingDelete({
            kind: 'gc',
            workspace: cleanupScope ?? '(all workspaces)',
            candidates,
            skipped: globalGc.skipped,
          });
        }
        return;
      }
      // GC rows open the review flow rather than a blind count: the list
      // filters to exactly the candidates so the user sees what will go
      // and can exempt rows (^b/^t/^r) before confirming with another ^d.
      if (selectedItem?.type === 'expand') {
        handleSearchChange('');
        setCleanupScope(selectedItem.workspace);
        setFilterMode('cleanup');
        setCursor(0);
      } else if (selectedSession && !selectedSession.isActive) {
        const lockInfo = isSessionLocked(selectedSession.sessionId, {
          engine: selectedSession.engine,
          source: selectedSession.source,
        });
        if (lockInfo) {
          setMutationNotice({
            text: `Cannot delete: session is open in another terminal (${formatSessionLockOwner(lockInfo)})`,
            tone: 'error',
          });
          return;
        }
        setPendingDelete({
          kind: 'session',
          sessionId: selectedSession.sessionId,
          title: sanitizeSessionTitleForDisplay(selectedSession.title),
          source: selectedSession.source,
          cloud: selectedSession.executionTarget?.kind === 'cloud-sandbox',
          engine: selectedSession.engine,
        });
      } else if (selectedSession?.isActive) {
        setMutationNotice({
          text: 'Cannot delete the active session',
          tone: 'error',
        });
      }
      return;
    }

    if (key.escape) {
      setBaseInputOwner('list');
      // Leave cleanup review first — it is a mode, not a filter choice.
      if (filterMode === 'cleanup') {
        setFilterMode('all');
        setCleanupScope(null);
        setCursor(0);
        return;
      }
      if (controlFocus) {
        setControlFocus(null);
        return;
      }
      if (search) {
        handleSearchChange('');
      } else {
        onClose();
      }
      return;
    }
    if (key.return) {
      setBaseInputOwner('list');
      if (selectedItem?.type === 'expand') {
        setExpandedGroups((prev) => {
          const next = new Set(prev);
          next.add(selectedItem.workspace);
          return next;
        });
        return;
      }
      if (selectedSession && !selectedSession.isActive) {
        const isCloud =
          selectedSession.source === 'remote' ||
          selectedSession.executionTarget?.kind === 'cloud-sandbox';
        const environment = isCloud ? 'cloud' : 'local';
        // Cross-workspace load: stage a confirmation so the user knows the
        // working directory will switch. Same-workspace loads go straight
        // through with no targetCwd.
        const sameWorkspace =
          normalizeWs(selectedSession.workspace) === normalizeWs(currentCwd);
        if (!isCloud && !sameWorkspace && selectedSession.workspace) {
          if (!directoryExists(selectedSession.workspace)) {
            setMutationNotice({
              text: `Cannot open: session directory no longer exists (${selectedSession.workspace})`,
              tone: 'error',
            });
            return;
          }
          setPendingSwitch({
            sessionId: selectedSession.sessionId,
            environment: environment as 'local' | 'cloud',
            workspace: selectedSession.workspace,
            engine: selectedSession.engine ?? 'v3',
          });
          return;
        }
        const lockInfo = isSessionLocked(selectedSession.sessionId, {
          engine: selectedSession.engine,
          source: selectedSession.source,
        });
        if (lockInfo) {
          setMutationNotice({
            text: `Cannot open: session is active in another terminal (${formatSessionLockOwner(lockInfo)})`,
            tone: 'error',
          });
          return;
        }
        onSelect(
          selectedSession.sessionId,
          environment as 'local' | 'cloud',
          undefined,
          {
            via: searchResults ? 'search' : 'browse',
            engine: selectedSession.engine ?? 'v3',
          }
        );
      } else if (selectedSession?.isActive) {
        setMutationNotice({
          text: 'Session is already open in this terminal',
          tone: 'warning',
        });
      }
      return;
    }
    if (key.upArrow) {
      setBaseInputOwner('list');
      setCursor((i) => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow) {
      setBaseInputOwner('list');
      setCursor((i) => Math.min(navItems.length - 1, i + 1));
      return;
    }
    // Paging: a store with thousands of rows must not be traversed
    // line-by-line. One page = the current visible window.
    if (key.pageUp) {
      setBaseInputOwner('list');
      setCursor((i) => Math.max(0, i - Math.max(endIdx - startIdx, 1)));
      return;
    }
    if (key.pageDown) {
      setBaseInputOwner('list');
      setCursor((i) =>
        Math.min(navItems.length - 1, i + Math.max(endIdx - startIdx, 1))
      );
      return;
    }
    // ←/→ jump between workspace groups (wrapping at the ends).
    if (key.rightArrow) {
      setBaseInputOwner('list');
      if (groupStarts.length > 0) {
        setCursor((c) => groupStarts.find((s) => s > c) ?? groupStarts[0]!);
      }
      return;
    }
    if (key.leftArrow) {
      setBaseInputOwner('list');
      // Inside a group the user expanded, ← collapses it (from ANY row —
      // the group's end can be thousands of rows away) and parks the
      // cursor on its first row. Otherwise ← jumps to the previous group.
      const g = navGroupIdx[cursor];
      const ws = g != null && g >= 0 ? groupKeys[g] : undefined;
      if (ws && expandedGroups.has(ws) && !isSearching) {
        setExpandedGroups((prev) => {
          const next = new Set(prev);
          next.delete(ws);
          return next;
        });
        setCursor(groupStarts[g!] ?? 0);
        return;
      }
      if (groupStarts.length > 0) {
        setCursor(
          (c) =>
            [...groupStarts].reverse().find((s) => s < c) ??
            groupStarts[groupStarts.length - 1]!
        );
      }
      return;
    }
    // Keep the hosted shortcut here so modal/editor branches own the keypress.
    if (key.ctrl && (input === 'p' || input === 'P')) {
      if (onTogglePreview) onTogglePreview();
      else setShowInlinePreview((visible) => !visible);
      return;
    }
    // Tab toggles the preview section; Shift+Tab cycles its view while open.
    if (key.tab && key.shift) {
      if (onCyclePreviewView) onCyclePreviewView();
      else
        setPreviewTab((t) =>
          t === 'summary' ? 'messages' : t === 'messages' ? 'turns' : 'summary'
        );
      return;
    }
    if (key.tab) {
      if (onTogglePreview) onTogglePreview();
      else setShowInlinePreview((v) => !v);
      return;
    }
    // Cleanup review owns the keyboard like a modal: typing must not swap
    // the list to filter-ignoring search results while ^d stays armed to
    // stage the bulk delete of the (no longer visible) candidates.
    if (filterMode === 'cleanup') return;
    // Type-to-search, with readline-style end-of-buffer editing.
    if (key.ctrl && (input === 'w' || input === 'W')) {
      setBaseInputOwner('search');
      handleSearchChange(deleteLastWord(searchRef.current));
      return;
    }
    if (key.ctrl && (input === 'u' || input === 'U')) {
      setBaseInputOwner('search');
      handleSearchChange('');
      return;
    }
    if (key.backspace || key.delete) {
      setBaseInputOwner('search');
      handleSearchChange(searchRef.current.slice(0, -1));
      return;
    }
    if (input && !key.ctrl && !key.meta) {
      const text = pastedLine(input);
      if (text) {
        setBaseInputOwner('search');
        handleSearchChange(searchRef.current + text);
      }
    }
  });

  // Scroll window over navItems, packed against a real line budget. Every
  // chrome line is counted, not estimated: fixed chrome, the footer hint
  // lines at the CURRENT width, the GC hint when present, active banners,
  // and the inline preview. Each nav item then pays its true rendered cost
  // — one line per row, two extra for the band above a group's first
  // in-window row, one for the cursor row's match snippet.
  const inlinePreviewShown =
    !hidePreview && showInlinePreview && !!selectedSession && termHeight >= 28;
  const bannerLines =
    (controlFocus ? 1 : 0) +
    (tagEditing ? 2 : 0) +
    (renameEditing ? 2 : 0) +
    (filterMode === 'cleanup' && !pendingDelete ? 2 : 0) +
    (pendingDelete ? (pendingDelete.kind === 'gc' ? 4 : 3) : 0) +
    (mutationNotice && !pendingDelete ? 2 : 0) +
    (pendingSwitch ? 4 : 0);
  const footerHintLines = wrapFooterHints(
    glyphs,
    termWidth,
    Boolean(onTogglePreview)
  ).slice(0, Math.max(termHeight - 12 - bannerLines, 0));
  const gcHintShown = Boolean(globalGc && globalGc.candidates.length > 0);
  // Fixed chrome: title 1 + search box 3 + controls 2 + column header 2 +
  // rule 1 + footer summary 1.
  const chromeReserve =
    10 +
    footerHintLines.length +
    (gcHintShown ? 1 : 0) +
    (inlinePreviewShown ? PREVIEW_MAX_LINES + 4 : 0) +
    bannerLines;
  // +1: the window's topmost band renders without its leading blank line.
  const lineBudget = Math.max(termHeight - chromeReserve, 0) + 1;

  const { startIdx, endIdx } = useMemo(() => {
    const anchor = Math.min(cursor, Math.max(navItems.length - 1, 0));
    const anchorItem = navItems[anchor];
    const anchorSnippet =
      searchResults &&
      anchorItem?.type === 'session' &&
      searchSnippets.has(
        dashboardSearchKey(anchorItem.entry.sessionId, anchorItem.entry.engine)
      )
        ? 1
        : 0;
    if (lineBudget < 3 + anchorSnippet) {
      return { startIdx: 0, endIdx: 0 };
    }
    return packScrollWindow({
      itemCount: navItems.length,
      navGroupIdx,
      cursor,
      lineBudget,
      anchorExtraLines: anchorSnippet,
    });
  }, [
    navItems,
    navGroupIdx,
    cursor,
    lineBudget,
    searchResults,
    searchSnippets,
  ]);

  // Slice displayItems so nav rows within [startIdx, endIdx) are shown,
  // carrying each group's header along.
  const visibleContent = useMemo(() => {
    const content: Array<DisplayItem & { navIdx?: number }> = [];
    let navIdx = 0;
    let pendingHeader: DisplayItem | null = null;
    for (const item of displayItems) {
      if (item.type === 'header') {
        pendingHeader = item;
        continue;
      }
      if (navIdx >= startIdx && navIdx < endIdx) {
        if (pendingHeader) {
          content.push(pendingHeader);
          pendingHeader = null;
        }
        content.push({ ...item, navIdx });
      }
      navIdx++;
    }
    return content;
  }, [displayItems, startIdx, endIdx]);

  // Status indicators — one shared mapping drives glyph, label, and color.
  const statusGlyph = (entry: SessionDashboardEntry): string => {
    const d = sessionStatusDisplay(entry.status, Boolean(entry.isActive));
    return getColor(d.color)(glyphs[d.glyph]);
  };

  const statusLabel = (entry: SessionDashboardEntry): string =>
    sessionStatusDisplay(entry.status, Boolean(entry.isActive)).label;

  // Preview rendering.
  const renderPreview = (): React.ReactNode => {
    // Turns view reads the KAS turn tree, not the V2 `preview` — render it
    // first so KAS-native sessions (which have no V2 preview) still show.
    if (previewTab === 'turns') {
      if (turnTree === null) {
        return [
          <Text key="t">{chalk.hex(secondaryHex)('Loading turns…')}</Text>,
        ];
      }
      if (turnTree.length === 0) {
        return [
          <Text key="t">
            {chalk.hex(secondaryHex)('No turn data for this session.')}
          </Text>,
        ];
      }
      const rows: React.ReactNode[] = [];
      let shown = 0;
      const mainAgent = getColor('success'); // main-agent line — distinct from subagent dim
      for (
        let ti = 0;
        ti < turnTree.length && shown < PREVIEW_MAX_LINES;
        ti++
      ) {
        const turn = turnTree[ti]!;
        // User prompt line — the turn header (brand, ▸ marker to match legend).
        const head = truncateToWidth(
          `${glyphs.arrowRight} ${ti + 1}. ${turn.userText || '(no prompt)'}`,
          Math.max(termWidth - 8, 1)
        );
        rows.push(
          <Text key={`t-${ti}`}>{chalk.hex(brandHex).bold(head)}</Text>
        );
        shown++;
        // Main agent line — labeled "main", green, its own tools.
        if (shown < PREVIEW_MAX_LINES && turn.toolNames.length > 0) {
          const mainLine = truncateToWidth(
            `  ${glyphs.dotFilled} main · ${turn.toolNames.slice(0, 6).join(', ')}`,
            Math.max(termWidth - 8, 1)
          );
          rows.push(<Text key={`t-${ti}-main`}>{mainAgent(mainLine)}</Text>);
          shown++;
        }
        // Subagent lines — labeled "sub", dim, tree-connected, with tool count.
        for (
          let si = 0;
          si < turn.subagents.length && shown < PREVIEW_MAX_LINES;
          si++
        ) {
          const sub = turn.subagents[si]!;
          const branch =
            si === turn.subagents.length - 1
              ? glyphs.cornerBottomLeft
              : glyphs.teeRight;
          const line = truncateToWidth(
            `    ${branch} sub · ${sub.summary || 'subagent'} (${sub.toolCallCount} tools)`,
            Math.max(termWidth - 8, 1)
          );
          rows.push(
            <Text key={`t-${ti}-s-${si}`}>{chalk.hex(secondaryHex)(line)}</Text>
          );
          shown++;
        }
      }
      return rows;
    }

    if (!preview) return null;

    // During a search, lead with the match context so the hit explains itself.
    const matchLine =
      searchResults && selectedSession
        ? searchSnippets.get(
            dashboardSearchKey(
              selectedSession.sessionId,
              selectedSession.engine,
              selectedSession.source
            )
          )
        : undefined;
    const matchNode = matchLine ? (
      <Text key="match">
        {chalk.hex(accentHex)(
          truncateToWidth(
            `${glyphs.search} ${matchLine}`,
            Math.max(termWidth - 8, 1)
          )
        )}
      </Text>
    ) : null;

    if (previewTab === 'summary') {
      const lines: string[] = [];
      if (preview.summary.firstPrompt) {
        lines.push(preview.summary.firstPrompt);
      }
      if (preview.summary.toolsSummary.length > 0) {
        lines.push(
          `${preview.summary.isComplete ? 'Tools' : 'Recent tools'}: ${preview.summary.toolsSummary.slice(0, 8).join(', ')}`
        );
      }
      const previewTags = selectedSession
        ? bookmarkStore.getTags(selectedSession.sessionId)
        : [];
      if (previewTags.length > 0) {
        lines.push(`Tags: ${previewTags.map((t) => `#${t}`).join(' ')}`);
      }
      lines.push(
        preview.summary.isComplete
          ? `${preview.summary.turnCount} turns`
          : `${preview.summary.turnCount} recent turns`
      );
      const budget = matchNode ? PREVIEW_MAX_LINES - 1 : PREVIEW_MAX_LINES;
      return [
        matchNode,
        ...lines
          .slice(0, budget)
          .map((line, i) => (
            <Text key={i}>
              {chalk.hex(secondaryHex)(
                truncateToWidth(line, Math.max(termWidth - 8, 1))
              )}
            </Text>
          )),
      ];
    }

    // Messages tab.
    const budget = matchNode ? PREVIEW_MAX_LINES - 1 : PREVIEW_MAX_LINES;
    return [
      matchNode,
      ...preview.recentMessages.slice(-budget).map((msg, i) => {
        const rolePrefix = msg.role === 'user' ? 'You: ' : 'AI: ';
        const line = rolePrefix + msg.content;
        const color = msg.role === 'user' ? brandHex : secondaryHex;
        return (
          <Text key={i}>
            {chalk.hex(color)(
              truncateToWidth(line, Math.max(termWidth - 8, 1))
            )}
          </Text>
        );
      }),
    ];
  };

  // Self-contained chrome (not the shared Panel primitive): Panel registers
  // its own Esc handler which double-fires with ours — Esc during an active
  // search would clear the query AND close the panel. Owning the chrome also
  // lets the pinned side panel render at a fixed column width.
  const hr = chalk.hex(secondaryHex)(
    glyphs.lineHorizontal.repeat(Math.max(termWidth - 2, 1))
  );

  if (terminalTooShort) {
    return (
      <Box flexDirection="column" width={termWidth} paddingX={1}>
        <Text>{chalk.hex(brandHex).bold('Sessions')}</Text>
        <Text>
          {CURSOR_MARKER}
          {getColor('warning')(
            'Terminal is too short to select a session safely.'
          )}
        </Text>
        <Text>{dim('Resize to at least 13 rows, or press esc to close.')}</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" width={termWidth}>
      {/* Header: title left, close hint right — mock style. */}
      <Box paddingX={1}>
        <Text>
          {chalk
            .hex(brandHex)
            .bold(truncateToWidth('Sessions', Math.max(termWidth - 2, 1)))}
          {termWidth >= 24
            ? dim(
                ' '.repeat(
                  termWidth -
                    2 -
                    visibleWidth('Sessions') -
                    visibleWidth('esc to close')
                ) + 'esc to close'
              )
            : null}
        </Text>
      </Box>
      <Box flexDirection="column" paddingX={1}>
        {/* Search field — bordered, full width, mock style. */}
        <Box
          borderStyle={termWidth >= 8 ? 'round' : undefined}
          borderColor={secondaryHex}
          paddingX={termWidth >= 8 ? 1 : 0}
          width={Math.max(termWidth - 2, 1)}
        >
          <Text>
            {search
              ? getColor('warning')(
                  truncateToWidth(
                    `search: ${search}`,
                    Math.max(termWidth - (termWidth >= 8 ? 6 : 2), 1)
                  )
                )
              : dim('search: ')}
          </Text>
          {!hasModal && inputOwner === 'search' && (
            <Text inverse>{CURSOR_MARKER} </Text>
          )}
          {termWidth >= 48 && (
            <Text>
              {(() => {
                // The label states COVERAGE (what search can see right now),
                // not where the last match came from. '#'-prefixed queries
                // are tag lookups that never touch the content index.
                const isTagQuery = search.trim().startsWith('#');
                const showMode =
                  !isTagQuery && search.trim().length >= MIN_CONTENT_QUERY;
                const contentCovered =
                  (indexStatus === 'ready' || indexStatus === 'indexing') &&
                  getSessionSearchIndex().getCoverage() ===
                    'titles-and-prompts';
                const suffix = [
                  isTagQuery ? '[tags]' : '',
                  showMode
                    ? contentCovered
                      ? '[titles+prompts]'
                      : '[titles only]'
                    : '',
                  searchResults ? `${eligibleTotal} found` : '',
                  showMode && indexStatus === 'indexing' ? 'indexing…' : '',
                ]
                  .filter(Boolean)
                  .join(' ');
                if (!search) return dim('find by title, prompt or #tag');
                return suffix
                  ? (contentCovered && showMode) || isTagQuery
                    ? chalk.hex(accentHex)(suffix)
                    : dim(suffix)
                  : '';
              })()}
            </Text>
          )}
        </Box>

        {/* Controls bar — collapsed shows each control's key + selected value;
            ^f/^g expand a control's options inline for ←/→ selection. */}
        {(() => {
          const sel = chalk.hex(accentHex).bold;
          const keycap = (k: string, on: boolean) =>
            on ? sel(`[${k}]`) : dim(`[${k}]`);
          const groupOptions: { id: GroupByDimension; label: string }[] = [
            { id: 'workspace', label: 'workspace' },
            { id: 'recency', label: 'recency' },
            ...(hasStatusData
              ? [{ id: 'status' as GroupByDimension, label: 'status' }]
              : []),
            { id: 'none', label: 'none' },
          ];
          const filterState = filterMode;
          const filterOptions = [
            { id: 'all', label: 'all' },
            { id: 'current', label: 'current workspace' },
            { id: 'bookmarked', label: `${glyphs.sparkle} bookmarked` },
            { id: 'empties', label: 'all + empty' },
          ];
          if (termWidth < 48) {
            const compact = `ctrl+f ${filterState} ${glyphs.smallDot} ctrl+g ${groupBy} ${glyphs.smallDot} ctrl+x clear filter`;
            return (
              <Box marginBottom={1}>
                <Text>
                  {!hasModal && inputOwner === 'control' ? CURSOR_MARKER : null}
                  {dim(truncateToWidth(compact, Math.max(termWidth - 4, 1)))}
                </Text>
              </Box>
            );
          }
          const renderOptions = (
            options: { id: string; label: string }[],
            current: string
          ) =>
            options.map((o, i) => (
              <React.Fragment key={o.id}>
                {i > 0 ? dim(` ${glyphs.lineVertical} `) : null}
                {o.id === current
                  ? chalk
                      .bgHex(brandHex)
                      .hex(selectedTextHex)
                      .bold(` ${o.label} `)
                  : dim(o.label)}
              </React.Fragment>
            ));
          return (
            <Box flexDirection="column" marginBottom={1}>
              <Text>
                {!hasModal && inputOwner === 'control' ? CURSOR_MARKER : null}
                {keycap('ctrl+f', controlFocus === 'filter')}
                {dim(' filter')}
                {controlFocus === 'filter' ? (
                  <>
                    {dim(': ')}
                    {renderOptions(filterOptions, filterState)}
                  </>
                ) : (
                  <>
                    {dim(': ')}
                    {filterState === 'all'
                      ? dim('all')
                      : chalk.hex(brandHex)(
                          `<${filterOptions.find((o) => o.id === filterState)?.label ?? filterState}>`
                        )}
                  </>
                )}
                {dim('   ')}
                {keycap('ctrl+g', controlFocus === 'group')}
                {dim(' group')}
                {controlFocus === 'group' ? (
                  <>
                    {dim(': ')}
                    {renderOptions(groupOptions, groupBy)}
                  </>
                ) : (
                  <>
                    {dim(': ')}
                    {chalk.hex(brandHex)(`<${groupBy}>`)}
                  </>
                )}
                {dim('   ')}
                {keycap('ctrl+x', false)}
                {dim(' clear filter')}
                {searchResults && filterState !== 'all'
                  ? dim('  (search ignores filter)')
                  : null}
              </Text>
              {/* Interaction hints on their own line while a control is
                  expanded — inline they read as another option. */}
              {controlFocus && (
                <Text>
                  {dim(
                    `${glyphs.arrowLeft}${glyphs.arrowRight} select ${glyphs.smallDot} ${glyphs.enter} done ${glyphs.smallDot} esc cancel`
                  )}
                </Text>
              )}
            </Box>
          );
        })()}

        {/* Tag-edit input line (Ctrl+T) */}
        {tagEditing && (
          <Box marginBottom={1}>
            <Text>{getColor('warning')('tags: ')}</Text>
            {tagDraft ? (
              <Text>
                {chalk.hex(brandHex)(
                  truncateToWidthTail(
                    tagDraft,
                    Math.max(termWidth - (termWidth >= 80 ? 52 : 10), 1)
                  )
                )}
              </Text>
            ) : null}
            {!hasModal && inputOwner === 'tag' && (
              <Text inverse>{CURSOR_MARKER} </Text>
            )}
            {termWidth >= 80 && (
              <Text>{dim('comma-separated · enter save · esc cancel')}</Text>
            )}
          </Box>
        )}

        {/* Rename input line (Ctrl+R) */}
        {renameEditing && (
          <Box marginBottom={1}>
            <Text>{getColor('warning')('rename: ')}</Text>
            {renameDraft ? (
              <Text>
                {chalk.hex(brandHex)(
                  truncateToWidthTail(
                    renameDraft,
                    Math.max(termWidth - (termWidth >= 80 ? 52 : 10), 1)
                  )
                )}
              </Text>
            ) : null}
            {!hasModal && inputOwner === 'rename' && (
              <Text inverse>{CURSOR_MARKER} </Text>
            )}
            {termWidth >= 80 && (
              <Text>{dim('empty resets · enter save · esc cancel')}</Text>
            )}
          </Box>
        )}

        {/* Cleanup review banner: the list below is exactly what a
            confirmed cleanup would delete. */}
        {filterMode === 'cleanup' && !pendingDelete && (
          <Box marginBottom={1}>
            <Text>
              {getColor('warning')(
                truncateToWidth(
                  `cleanup review: ${
                    (globalGc?.candidates ?? []).filter(
                      (c) => !cleanupScope || c.workspace === cleanupScope
                    ).length
                  } empty session(s) shown — ctrl+d delete all · ctrl+b/t/r keeps a row · esc exit`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
          </Box>
        )}

        {/* Staged delete / GC confirmation */}
        {pendingDelete && (
          <Box flexDirection="column" marginBottom={1}>
            <Text>
              {getColor('error')(
                truncateToWidth(
                  pendingDelete.kind === 'session'
                    ? `${glyphs.warning} Delete "${pendingDelete.title.slice(0, 30)}"?`
                    : `${glyphs.warning} Delete ${pendingDelete.candidates.length} empty sessions?`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
            {pendingDelete.kind === 'gc' &&
              pendingDelete.workspace === '(all workspaces)' && (
                <Text>
                  {dim(
                    truncateToWidth(
                      `Skipping ${pendingDelete.skipped.locked} locked, ${pendingDelete.skipped.recent} recent, ${pendingDelete.skipped.userTouched} marked, ${pendingDelete.skipped.hasParent} derived`,
                      Math.max(termWidth - 4, 1)
                    )
                  )}
                </Text>
              )}
            <Text>
              {CURSOR_MARKER}
              {getColor('error')(
                truncateToWidth(
                  `y delete ${glyphs.smallDot} esc cancel`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
          </Box>
        )}

        {/* Post-mutation notice */}
        {mutationNotice && !pendingDelete && (
          <Box marginBottom={1}>
            <Text>
              {getColor(mutationNotice.tone)(
                truncateToWidth(
                  `${mutationNotice.tone === 'success' ? glyphs.checkmark : glyphs.warning} ${mutationNotice.text}`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
          </Box>
        )}

        {/* Cross-workspace switch confirmation */}
        {pendingSwitch && (
          <Box flexDirection="column" marginBottom={1}>
            <Text>
              {getColor('warning')(
                truncateToWidth(
                  `${glyphs.warning} Switch directory and load session?`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
            <Text>
              {dim(
                truncateToWidth(
                  pendingSwitch.workspace,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
            <Text>
              {CURSOR_MARKER}
              {chalk.hex(brandHex)(
                truncateToWidth(
                  `${glyphs.enter} switch & load ${glyphs.smallDot} esc cancel`,
                  Math.max(termWidth - 4, 1)
                )
              )}
            </Text>
          </Box>
        )}

        {/* Column header — derived from the same width constants as data
            rows so labels sit exactly over their columns. */}
        {(() => {
          const contentW = Math.max(termWidth - 4, 1);
          const columns = dashboardColumns(contentW, groupBy);
          const right = [
            columns.showWorkspace
              ? padToWidth('Workspace', COL_WS_W) + '  '
              : '',
            columns.showMessages ? padToWidthRight('Msgs', COL_MSGS_W) : '',
            columns.showAge ? padToWidthRight('Last Used', COL_AGE_W) : '',
            columns.showStatus ? padToWidth('Status', 2 + COL_STATUS_W) : '',
          ]
            .filter(Boolean)
            .join(' ');
          const left =
            '  ' +
            (columns.showId ? padToWidth('ID', COL_ID_W) + '  ' : '') +
            'Session Name';
          const gap = right
            ? Math.max(contentW - visibleWidth(left) - visibleWidth(right), 1)
            : 0;
          const header = truncateToWidth(
            left + ' '.repeat(gap) + right,
            contentW
          );
          return (
            <>
              <Text>{chalk.hex(secondaryHex).bold(header)}</Text>
              <Text>
                {chalk.hex(secondaryHex)(
                  glyphs.lineHorizontal.repeat(contentW)
                )}
              </Text>
            </>
          );
        })()}

        {/* Session list */}
        {visibleContent.map((item, i) => {
          // Inner content width: panel width minus its own horizontal padding.
          const contentW = Math.max(termWidth - 4, 1);

          if (item.type === 'header') {
            // Band-style group headers (mock): label + count on a subtle
            // background strip spanning the full content width.
            const count = item.group.sessions.filter(isRowVisible).length;
            const userExpanded =
              !isSearching && expandedGroups.has(item.group.workspace);
            const band = (label: string) => {
              const text = ` ${label} `;
              const countText = userExpanded
                ? `(${count}) ${glyphs.arrowLeft} to collapse `
                : `(${count}) `;
              const pad = Math.max(
                contentW - visibleWidth(text) - visibleWidth(countText),
                0
              );
              return (
                bandBackground(bandText.bold(text)) +
                bandBackground(bandMutedText(countText + ' '.repeat(pad)))
              );
            };
            // Pinned Bookmarked group: a starred band, no directory path.
            if (item.group.workspace === BOOKMARK_GROUP_KEY) {
              const star = glyphs.sparkle;
              return (
                <React.Fragment key="h-bookmarked">
                  {i > 0 && <Text> </Text>}
                  <Text>{band(`${star} Bookmarked`)}</Text>
                </React.Fragment>
              );
            }
            // Synthetic groups (recency buckets, "All sessions") have a
            // sentinel \u0000 key and no real path — band with the label.
            if (item.group.workspace.startsWith('\u0000')) {
              return (
                <React.Fragment key={`h-${item.group.workspace}`}>
                  {i > 0 && <Text> </Text>}
                  <Text>{band(item.group.label)}</Text>
                </React.Fragment>
              );
            }
            const workspace = item.group.workspace;
            const home = homedir();
            const displayPath = !workspace
              ? '(unknown workspace)'
              : workspace === home
                ? workspace
                : workspace.startsWith(`${home}${sep}`)
                  ? `~${workspace.slice(home.length)}`
                  : workspace;
            const label = truncateToWidth(
              displayPath,
              Math.max(contentW - 12, 1)
            );
            return (
              <React.Fragment key={`h-${item.group.workspace}`}>
                {i > 0 && <Text> </Text>}
                <Text>{band(label)}</Text>
              </React.Fragment>
            );
          }

          if (item.type === 'expand') {
            const isCursor = item.navIdx === cursor;
            const label = truncateToWidth(
              `${glyphs.arrowDown} more (${glyphs.enter} to expand)`,
              Math.max(contentW - 4, 1)
            );
            return (
              <Text key={`e-${item.workspace}`}>
                {isCursor && !hasModal && inputOwner === 'list'
                  ? CURSOR_MARKER
                  : null}
                {'  '}
                {isCursor
                  ? chalk.hex(accentHex).bold(`${glyphs.chevron} `)
                  : '  '}
                {isCursor
                  ? chalk.hex(accentHex).bold(label)
                  : chalk.hex(brandHex)(label)}
              </Text>
            );
          }

          const { entry } = item;
          const isCursor = item.navIdx === cursor;
          const chevron = isCursor
            ? chalk.hex(accentHex).bold(`${glyphs.chevron} `)
            : '  ';

          // Rewind forks get a ↩ marker folded into the title (plain text so
          // the fixed-column width math stays correct) — distinguishes an
          // alternate timeline from an ordinary session.
          const rewindMark =
            entry.createdReason === 'rewind' ? `${glyphs.rewind} ` : '';
          // Cloud rows (remote store or cloud-sandbox placement) get a ☁
          // marker: deleting one goes through the agent to the backend, so
          // the row must be visually distinct from local ones.
          const cloudMark =
            entry.source === 'remote' ||
            entry.executionTarget?.kind === 'cloud-sandbox'
              ? `${glyphs.cloud}  `
              : '';
          // Bookmark star prefix + tags suffix, all folded into the title as
          // plain text so the fixed-column widths stay correct.
          const starMark = bookmarkStore.isBookmarked(entry.sessionId)
            ? `${glyphs.sparkle} `
            : '';
          // Engine marker as title prefix (only for older engines).
          const engineChip =
            entry.engine === 'classic' || entry.engine === 'v2'
              ? `[${entry.engine}]  `
              : '';
          const entryTags = bookmarkStore.getTags(entry.sessionId);
          const tagChips =
            entryTags.length > 0
              ? entryTags.map((t) => ` #${t} `).join(' ')
              : '';
          const titleText =
            starMark +
            rewindMark +
            cloudMark +
            engineChip +
            (sanitizeSessionTitleForDisplay(entry.title) || '(no title)');
          const age = entry.updatedAt
            ? formatRelativeTimeShort(entry.updatedAt)
            : '';
          const statusDisplay = sessionStatusDisplay(
            entry.status,
            Boolean(entry.isActive)
          );
          const status = statusGlyph(entry);
          const selectedStatus = glyphs[statusDisplay.glyph];
          // Every row carries a status label — a blank column reads as
          // missing data rather than a quiet state. Truncated to the
          // reserved column width so an unknown backend status can never
          // wrap the row (one row must always cost one line).
          const statusText = truncateToWidth(
            statusLabel(entry) || 'idle',
            COL_STATUS_W
          );

          const columns = dashboardColumns(contentW, groupBy);
          const msgs =
            entry.messageCount != null && entry.messageCount > 0
              ? entry.messageCount > 999
                ? '999+'
                : String(entry.messageCount)
              : '';
          const wsText = columns.showWorkspace
            ? truncateToWidth(workspaceLabel(entry.workspace), COL_WS_W)
            : '';
          const idText = columns.showId ? shortSessionId(entry.sessionId) : '';
          const rightW =
            (columns.showWorkspace ? COL_WS_W + 1 : 0) +
            (columns.showMessages ? COL_MSGS_W + 1 : 0) +
            (columns.showAge ? COL_AGE_W + 1 : 0) +
            (columns.showStatus ? COL_STATUS_W + 3 : 0);
          const titleWidth = Math.max(
            contentW - 2 - (columns.showId ? COL_ID_W + 2 : 0) - rightW,
            1
          );

          // Tags render only when the full chip area fits beside the title.
          const tagVisualWidth = tagChips ? visibleWidth('  ' + tagChips) : 0;
          const showTags = tagVisualWidth > 0 && tagVisualWidth < titleWidth;
          const titleBudget = Math.max(
            titleWidth - (showTags ? tagVisualWidth : 0),
            1
          );
          const truncTitle = truncateToWidth(titleText, titleBudget);
          const styledTags = showTags
            ? '  ' +
              entryTags
                .map((t) => chalk.bgHex(tagBgHex).hex(tagTextHex)(` #${t} `))
                .join(' ')
            : '';
          const padNeeded = Math.max(
            titleWidth -
              visibleWidth(truncTitle) -
              (showTags ? tagVisualWidth : 0),
            0
          );
          const row = isCursor
            ? chalk.hex(accentHex).bold(truncTitle) +
              styledTags +
              ' '.repeat(padNeeded)
            : (entry.status === 'idle' || !entry.status) && !entry.isActive
              ? chalk.hex(secondaryHex)(truncTitle) +
                styledTags +
                ' '.repeat(padNeeded)
              : truncTitle + styledTags + ' '.repeat(padNeeded);

          const snippet =
            isCursor && searchResults
              ? searchSnippets.get(
                  dashboardSearchKey(
                    entry.sessionId,
                    entry.engine,
                    entry.source
                  )
                )
              : undefined;
          const meta = isCursor
            ? chalk.hex(accentHex).bold
            : chalk.hex(secondaryHex);
          return (
            <React.Fragment key={sessionIdentityKey(entry)}>
              <Text>
                {isCursor && !hasModal && inputOwner === 'list'
                  ? CURSOR_MARKER
                  : null}
                {chevron}
                {columns.showId
                  ? meta(padToWidth(idText, COL_ID_W) + '  ')
                  : ''}
                {row}
                {columns.showWorkspace
                  ? meta(' ' + padToWidth(wsText, COL_WS_W))
                  : ''}
                {columns.showMessages
                  ? meta(' ' + padToWidthRight(msgs, COL_MSGS_W))
                  : ''}
                {columns.showAge
                  ? meta(' ' + padToWidthRight(age, COL_AGE_W))
                  : ''}
                {columns.showStatus ? (
                  <>
                    {' '}
                    {isCursor ? meta(selectedStatus) : status}
                    {meta(` ${statusText}`)}
                  </>
                ) : null}
              </Text>
              {snippet ? (
                <Text>
                  {'    '}
                  {dim(
                    truncateToWidth(
                      `${glyphs.search} ${snippet.replace(/\s+/g, ' ')}`,
                      Math.max(contentW - 6, 1)
                    )
                  )}
                </Text>
              ) : null}
            </React.Fragment>
          );
        })}

        {navItems.length === 0 && (
          <Text>
            {!hasModal && inputOwner === 'list' ? CURSOR_MARKER : null}
            {chalk.hex(secondaryHex)(
              isRefreshing ? '  Loading sessions…' : '  No sessions found.'
            )}
          </Text>
        )}

        {/* Preview pane (suppressed when the host shows a wide preview,
            toggled with Tab otherwise) */}
        {!hidePreview && inlinePreviewShown && selectedSession && (
          <>
            <Divider />
            <Box marginBottom={0}>
              <Text>
                {dim(
                  truncateToWidth(
                    `Preview of "${sanitizeSessionTitleForDisplay(selectedSession.title) || 'selected session'}"`,
                    Math.max(termWidth - 4, 1)
                  )
                )}
              </Text>
            </Box>
            <Box marginBottom={0}>
              <Text>
                {previewTab === 'summary'
                  ? chalk.hex(brandHex).bold(`${glyphs.dotFilled} Overview`)
                  : chalk.hex(secondaryHex)(`${glyphs.dotEmpty} Overview`)}
                {'  '}
                {previewTab === 'messages'
                  ? chalk
                      .hex(brandHex)
                      .bold(`${glyphs.dotFilled} Last messages`)
                  : chalk.hex(secondaryHex)(`${glyphs.dotEmpty} Last messages`)}
                {'  '}
                {previewTab === 'turns'
                  ? chalk.hex(brandHex).bold(`${glyphs.dotFilled} Turns`)
                  : chalk.hex(secondaryHex)(`${glyphs.dotEmpty} Turns`)}
                {dim('  (tab)')}
              </Text>
            </Box>
            {previewTab === 'turns' && (
              <Box marginBottom={0}>
                <Text>
                  {chalk.hex(brandHex)(`${glyphs.arrowRight} you`)}
                  {dim('  ')}
                  {getColor('success')(`${glyphs.dotFilled} main`)}
                  {dim('  ')}
                  {chalk.hex(secondaryHex)(`${glyphs.cornerBottomLeft} sub`)}
                </Text>
              </Box>
            )}
            <Box flexDirection="column" marginLeft={1}>
              <Box flexDirection="column" width={1} backgroundColor={brandHex}>
                {/* Decorative sidebar line — rendered as sibling of content */}
              </Box>
              {renderPreview()}
            </Box>
          </>
        )}
      </Box>
      <Text>{hr}</Text>
      <Box flexDirection="column" paddingX={1}>
        {/* Footer, mock style: visible-range summary line, then action hints
            wrapped across as many rows as the terminal can afford. */}
        {(() => {
          // Total reflects the FILTERED view — quoting the raw store count
          // under an active filter reads as "the filter did nothing".
          const total = eligibleTotal;
          const storeTotal = sessions.length;
          const shownFrom = navItems.length === 0 ? 0 : startIdx + 1;
          const shownTo = endIdx;
          // Whole groups scrolled out of the window, either side — folded
          // into the summary line so they cost no list space.
          const hiddenAbove =
            navItems.length === 0 ? 0 : Math.max(navGroupIdx[startIdx]!, 0);
          const hiddenBelow =
            navItems.length === 0
              ? 0
              : groupStarts.length - 1 - navGroupIdx[endIdx - 1]!;
          const groupCue =
            (hiddenAbove > 0
              ? ` ${glyphs.smallDot} ${glyphs.arrowUp} ${hiddenAbove} group${hiddenAbove === 1 ? '' : 's'} above`
              : '') +
            (hiddenBelow > 0
              ? ` ${glyphs.smallDot} ${glyphs.arrowDown} ${hiddenBelow} group${hiddenBelow === 1 ? '' : 's'} below`
              : '');
          const summary =
            `Showing ${shownFrom}\u2013${shownTo} of ${total} sessions` +
            (total < storeTotal ? ` (${storeTotal} in store)` : '') +
            (isRefreshing ? ' (refreshing\u2026)' : '') +
            groupCue;
          const gcHint = gcHintShown
            ? `${globalGc!.candidates.length} empty sessions \u2014 /sessions clean to review`
            : '';
          const lines = footerHintLines;
          return (
            <>
              <Text>
                {dim(summary)}
                {catalogIncomplete
                  ? getColor('warning')(
                      ' · catalog incomplete; cached sessions retained'
                    )
                  : ''}
              </Text>
              {gcHint && <Text>{getColor('warning')(gcHint)}</Text>}
              {lines.map((l, i) => (
                <Text key={i}>{dim(l)}</Text>
              ))}
            </>
          );
        })()}
      </Box>
    </Box>
  );
};
