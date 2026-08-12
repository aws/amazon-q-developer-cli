/**
 * Configurable grouping + filtering for the session dashboard.
 *
 * Workspace grouping is one opinionated view; users think about their
 * sessions along several axes. This layer turns a flat listing into
 * `WorkspaceGroup[]` (the shape the list renderer already consumes) under a
 * chosen dimension, after applying a set of composable filters.
 *
 * Dimensions: 'workspace' (default) · 'recency' · 'none'.
 * Filters (all optional, ANDed): current-workspace, bookmarked, tag.
 * Tag is deliberately a FILTER, not a group-by, to sidestep the multi-tag
 * "which bucket?" ambiguity.
 */

import {
  buildDashboardEntries,
  workspaceLabel,
  type SessionDashboardEntry,
  type WorkspaceGroup,
  type SessionListingInput,
} from './session-dashboard.js';

export type GroupByDimension = 'workspace' | 'recency' | 'status' | 'none';

export interface SessionFilters {
  /** Only sessions in the current working directory. */
  currentWorkspaceOnly?: boolean;
  /** Only bookmarked sessions. */
  bookmarkedOnly?: boolean;
  /** Only sessions carrying this tag (normalized, no leading '#'). */
  tag?: string | null;
}

/** Lookups the grouper needs from the bookmark sidecar (injected for tests). */
export interface MetaLookup {
  isBookmarked: (id: string) => boolean;
  getTags: (id: string) => string[];
}

const RECENCY_ORDER = [
  'Today',
  'Yesterday',
  'Last 3 days',
  'Last week',
  'Last month',
  'Older',
  'Unknown',
] as const;
type RecencyBucket = (typeof RECENCY_ORDER)[number];

const DAY_MS = 24 * 60 * 60 * 1000;

/** Classify an RFC3339 timestamp into a recency bucket relative to `now`. */
export function recencyBucket(
  updatedAt: string,
  now: number = Date.now()
): RecencyBucket {
  if (!updatedAt) return 'Unknown';
  const t = new Date(updatedAt).getTime();
  if (Number.isNaN(t)) return 'Unknown';
  // "Today"/"Yesterday" are calendar-relative; the rest are rolling windows.
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const startMs = startOfToday.getTime();
  if (t >= startMs) return 'Today';
  if (t >= startMs - DAY_MS) return 'Yesterday';
  const age = now - t;
  if (age <= 3 * DAY_MS) return 'Last 3 days';
  if (age <= 7 * DAY_MS) return 'Last week';
  if (age <= 30 * DAY_MS) return 'Last month';
  return 'Older';
}

function byRecency(a: SessionDashboardEntry, b: SessionDashboardEntry): number {
  if (!a.updatedAt && !b.updatedAt) return 0;
  if (!a.updatedAt) return 1;
  if (!b.updatedAt) return -1;
  return b.updatedAt.localeCompare(a.updatedAt);
}

const STATUS_ORDER = [
  'Waiting on you',
  'Working',
  'Provisioning',
  'Idle',
  'Done',
  'Failed',
  'No status',
] as const;
type StatusBucket = (typeof STATUS_ORDER)[number];

/** Map a SessionActivityStatus to a display bucket. Absent → "No status"
 *  (most local sessions never report a live status). */
export function statusBucket(status: string | undefined): StatusBucket {
  switch (status) {
    case 'in_progress':
      return 'Working';
    case 'waiting_on_user':
      return 'Waiting on you';
    case 'provisioning':
      return 'Provisioning';
    case 'idle':
      return 'Idle';
    case 'completed':
      return 'Done';
    case 'failed':
      return 'Failed';
    default:
      return 'No status';
  }
}

/** How one activity status renders: row label, glyph name, and theme color
 *  path. The single mapping shared by every status indicator, so a new
 *  status value needs exactly one edit. */
export interface SessionStatusDisplay {
  label: string;
  glyph:
    | 'dotFilled'
    | 'dotEmpty'
    | 'dotDouble'
    | 'dotDashed'
    | 'checkmark'
    | 'cross';
  color: 'success' | 'warning' | 'highlight' | 'error' | 'secondary';
}

export function sessionStatusDisplay(
  status: string | null | undefined,
  isActive: boolean
): SessionStatusDisplay {
  if (isActive)
    return { label: 'active', glyph: 'dotFilled', color: 'success' };
  switch (status) {
    case 'in_progress':
      return { label: 'working', glyph: 'dotFilled', color: 'warning' };
    case 'waiting_on_user':
      return { label: 'waiting', glyph: 'dotDouble', color: 'highlight' };
    case 'completed':
      return { label: 'done', glyph: 'checkmark', color: 'success' };
    case 'failed':
      return { label: 'failed', glyph: 'cross', color: 'error' };
    case 'provisioning':
      return { label: 'starting', glyph: 'dotDashed', color: 'warning' };
    default:
      return {
        label: status && status !== 'idle' ? status : '',
        glyph: 'dotEmpty',
        color: 'secondary',
      };
  }
}

function applyFilters(
  entries: SessionDashboardEntry[],
  filters: SessionFilters,
  meta: MetaLookup
): SessionDashboardEntry[] {
  return entries.filter((e) => {
    if (filters.currentWorkspaceOnly && !e.isCurrentWorkspace) return false;
    if (filters.bookmarkedOnly && !meta.isBookmarked(e.sessionId)) return false;
    if (filters.tag) {
      const tags = meta.getTags(e.sessionId);
      if (!tags.includes(filters.tag)) return false;
    }
    return true;
  });
}

/** Sentinel workspace key for the pinned Cloud group (never a real path). */
export const CLOUD_GROUP_KEY = '\u0000cloud';

/** Cloud/remote sessions have no local workspace and route to the backend. */
function isCloudSession(e: SessionDashboardEntry): boolean {
  return e.source === 'remote' || e.executionTarget?.kind === 'cloud-sandbox';
}

/**
 * Group + filter a raw listing into `WorkspaceGroup[]` for the list renderer.
 *
 * `groupBy: 'none'` yields a single "All sessions" group (recency-sorted).
 * `'recency'` yields fixed time buckets in chronological order.
 * `'workspace'` matches the classic view (current workspace pinned first).
 */
export function groupSessions(
  sessions: SessionListingInput[],
  currentCwd: string,
  opts: {
    groupBy: GroupByDimension;
    filters?: SessionFilters;
    activeSessionId?: string | null;
    activeEngine?: SessionListingInput['engine'];
    activeSource?: SessionListingInput['source'];
    meta: MetaLookup;
    now?: number;
  }
): WorkspaceGroup[] {
  const {
    groupBy,
    filters = {},
    activeSessionId,
    activeEngine,
    activeSource,
    meta,
    now,
  } = opts;
  const normalizedCwd = currentCwd.replace(/\/+$/, '');
  const all = buildDashboardEntries(
    sessions,
    currentCwd,
    activeSessionId,
    activeEngine,
    activeSource
  );
  const entries = applyFilters(all, filters, meta);

  if (groupBy === 'none') {
    const sorted = [...entries].sort(byRecency);
    return sorted.length > 0
      ? [
          {
            workspace: '\u0000all',
            label: `All sessions (${sorted.length})`,
            isCurrent: false,
            sessions: sorted,
          },
        ]
      : [];
  }

  if (groupBy === 'recency') {
    const buckets = new Map<RecencyBucket, SessionDashboardEntry[]>();
    for (const e of entries) {
      const b = recencyBucket(e.updatedAt, now);
      (buckets.get(b) ?? buckets.set(b, []).get(b)!).push(e);
    }
    const groups: WorkspaceGroup[] = [];
    for (const bucket of RECENCY_ORDER) {
      const list = buckets.get(bucket);
      if (!list || list.length === 0) continue;
      list.sort(byRecency);
      groups.push({
        // Sentinel key so this never collides with a real workspace path.
        workspace: `\u0000recency:${bucket}`,
        label: bucket,
        isCurrent: bucket === 'Today',
        sessions: list,
      });
    }
    return groups;
  }

  if (groupBy === 'status') {
    const buckets = new Map<StatusBucket, SessionDashboardEntry[]>();
    for (const e of entries) {
      // The active session is working by definition, even when the listing
      // doesn't carry a live status for it yet.
      const b = statusBucket(e.isActive ? 'in_progress' : e.status);
      (buckets.get(b) ?? buckets.set(b, []).get(b)!).push(e);
    }
    const groups: WorkspaceGroup[] = [];
    for (const bucket of STATUS_ORDER) {
      const list = buckets.get(bucket);
      if (!list || list.length === 0) continue;
      list.sort(byRecency);
      groups.push({
        workspace: `\u0000status:${bucket}`,
        label: bucket,
        // Live/active buckets lead; treat Working as the "current" highlight.
        isCurrent: bucket === 'Waiting on you',
        sessions: list,
      });
    }
    return groups;
  }

  // groupBy === 'workspace'
  const groupMap = new Map<string, SessionDashboardEntry[]>();
  for (const e of entries) {
    // Cloud/remote sessions carry no local workspace path, so grouping them
    // by cwd sinks them into "(unknown workspace)". Give them their own
    // group instead, keyed by a sentinel so it never collides with a path.
    const key = isCloudSession(e)
      ? CLOUD_GROUP_KEY
      : e.workspace.replace(/\/+$/, '');
    (groupMap.get(key) ?? groupMap.set(key, []).get(key)!).push(e);
  }
  const groups: WorkspaceGroup[] = [];
  for (const [workspace, list] of groupMap) {
    list.sort(byRecency);
    groups.push({
      workspace,
      label:
        workspace === CLOUD_GROUP_KEY ? 'Cloud' : workspaceLabel(workspace),
      isCurrent: workspace.replace(/\/+$/, '') === normalizedCwd,
      sessions: list,
    });
  }
  groups.sort((a, b) => {
    // Group-by-workspace order: Cloud, then the current workspace, then other
    // workspaces alphabetically. (The Bookmarked group is pinned ahead of all
    // of these by the caller.)
    const aCloud = a.workspace === CLOUD_GROUP_KEY;
    const bCloud = b.workspace === CLOUD_GROUP_KEY;
    if (aCloud !== bCloud) return aCloud ? -1 : 1;
    if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1;
    return a.label.localeCompare(b.label);
  });
  return groups;
}
