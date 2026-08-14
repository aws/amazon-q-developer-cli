/**
 * Session Dashboard data layer — types and grouping utilities.
 *
 * Transforms a flat KAS session listing into workspace-grouped entries
 * suitable for the SessionDashboard component. KAS engine only.
 */

import { homedir } from 'node:os';
import { join } from 'node:path';
import type {
  SessionInfoEntry,
  SessionActivityStatus,
  ExecutionTarget,
  SessionDiscoverySource,
} from '../types/session-client.js';

/**
 * A session entry enriched with dashboard-specific metadata:
 * workspace grouping, subagent relationship, and active-session indicator.
 */
export interface SessionDashboardEntry {
  sessionId: string;
  title: string;
  workspace: string;
  isCurrentWorkspace: boolean;
  updatedAt: string;
  status?: SessionActivityStatus;
  executionTarget?: ExecutionTarget;
  source?: SessionDiscoverySource;
  messageCount?: number;
  /** Which engine's store the row came from. Absent == unknown (treated as v3). */
  engine?: 'classic' | 'v2' | 'v3';
  /** True when this session is a child of another (subagent / crew stage). */
  isSubagent: boolean;
  /** The parent session id when this entry is a subagent, rewind, or tangent. */
  parentSessionId?: string;
  /**
   * Why the session was derived from a parent, when applicable:
   * - `subagent` — a subagent/crew child (nested + hidden, shown in preview)
   * - `rewind` — a fork of an earlier turn (nested + hidden, alternate timeline)
   * - `tangent` — a user-created side-conversation (nested but VISIBLE and
   *   resumable, rendered indented under its parent)
   * Absent for ordinary top-level sessions.
   */
  createdReason?: 'subagent' | 'rewind' | 'tangent';
  /** Hidden child sessions (subagents/crew stages) — surfaced in the preview
   *  pane, not the master list. */
  children: SessionDashboardEntry[];
  /** Visible tangent children, rendered indented under this entry in the
   *  master list (kept separate from `children` so the hidden-nesting and
   *  preview paths are unaffected). */
  tangentChildren: SessionDashboardEntry[];
  /** True when this session is the currently active physical session. */
  isActive: boolean;
}

/** Input entry shape: a KAS listing row optionally enriched with on-disk
 *  lineage metadata (parent id + created reason) that the disk scan reads
 *  from V2 session metadata. */
export type SessionListingInput = SessionInfoEntry & {
  parentSessionId?: string;
  createdReason?: 'subagent' | 'rewind' | 'tangent';
  /** Which engine's store the row came from. Absent == unknown (treated as v3). */
  engine?: 'classic' | 'v2' | 'v3';
};

export interface SessionPhysicalIdentity {
  sessionId: string;
  engine?: SessionListingInput['engine'];
  source?: SessionDiscoverySource;
}

export function sessionIdentityKey(identity: SessionPhysicalIdentity): string {
  return `${identity.engine ?? 'v3'}\u0000${identity.source === 'remote' ? 'remote' : 'local'}\u0000${identity.sessionId}`;
}

/**
 * A workspace group containing sessions in source listing order.
 */
export interface WorkspaceGroup {
  /** Absolute workspace path (used as the group key). */
  workspace: string;
  /** Short display label (basename or last two path segments). */
  label: string;
  /** True when this workspace matches the current working directory. */
  isCurrent: boolean;
  /** Sessions in source listing order. Subagent sessions are nested. */
  sessions: SessionDashboardEntry[];
}

export interface SessionDashboardPaneWidths {
  list: number;
  preview: number;
  showPreview: boolean;
}

export function sessionDashboardPaneWidths(
  width: number,
  previewRequested: boolean
): SessionDashboardPaneWidths {
  const safeWidth = Math.max(Math.floor(width), 1);
  const minimumList = 32;
  const minimumPreview = 24;
  const showPreview =
    previewRequested && safeWidth >= minimumList + minimumPreview + 1;
  if (!showPreview) return { list: safeWidth, preview: 0, showPreview: false };
  const list = Math.min(
    Math.max(Math.floor(safeWidth * 0.45), minimumList),
    safeWidth - minimumPreview - 1,
    70
  );
  return {
    list,
    preview: safeWidth - list - 1,
    showPreview: true,
  };
}

/**
 * Extract a short display label from a workspace path.
 * Uses the last path segment, or last two if the basename is generic (e.g. "src").
 */
export function workspaceLabel(workspace: string): string {
  if (!workspace) return '(unknown)';
  // Split on both separators so Windows paths label by basename too.
  // A bare "." (a relative cwd recorded verbatim) identifies nothing.
  const segments = workspace
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .filter((s) => s !== '' && s !== '.');
  if (segments.length === 0) return '(unknown)';
  const last = segments[segments.length - 1]!;
  // Common generic names: show parent/child to disambiguate.
  const generic = new Set([
    'src',
    'app',
    'project',
    'workspace',
    'work',
    'code',
    'dev',
    'repo',
    'repos',
  ]);
  if (generic.has(last.toLowerCase()) && segments.length >= 2) {
    return `${segments[segments.length - 2]}/${last}`;
  }
  return last;
}

/** Normalize workspace identity for grouping and cross-workspace resume. */
export function normalizeWorkspace(workspace: unknown): string {
  if (typeof workspace !== 'string') return '';
  const trimmed = workspace.trim().replace(/[\\/]+$/, '');
  if (!trimmed || trimmed === '.') return '';
  if (trimmed === '~') return homedir();
  if (/^~[\\/]/.test(trimmed)) return join(homedir(), trimmed.slice(2));
  return trimmed;
}

const UUID_SOURCE =
  '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}';
const KAS_UUID_RE = new RegExp(`^(?:sess_)?(${UUID_SOURCE})$`, 'i');
const CONVERTED_KAS_UUID_RE = new RegExp(
  `^cli_(${UUID_SOURCE})_[A-Za-z0-9_-]+$`,
  'i'
);

/** Extract encoded conversion lineage without changing physical identity. */
export function conversationKey(sessionId: string): string {
  const match =
    KAS_UUID_RE.exec(sessionId) ?? CONVERTED_KAS_UUID_RE.exec(sessionId);
  return match?.[1]?.toLowerCase() ?? sessionId;
}

/** Compare a dashboard row to the active physical identity. */
export function sessionMatchesActive(
  session: SessionPhysicalIdentity,
  activeSessionId?: string | null,
  activeEngine?: SessionListingInput['engine'],
  activeSource?: SessionDiscoverySource
): boolean {
  if (!activeSessionId) return false;
  if (activeEngine !== undefined && (session.engine ?? 'v3') !== activeEngine) {
    return false;
  }
  if (
    activeSource !== undefined &&
    (session.source === 'remote' ? 'remote' : 'local') !== activeSource
  ) {
    return false;
  }
  if (session.sessionId === activeSessionId) return true;
  return session.engine === 'v3' || session.engine === undefined
    ? session.sessionId.replace(/^sess_/, '') ===
        activeSessionId.replace(/^sess_/, '')
    : false;
}

/**
 * Whether a listing row is a DERIVED artifact — a subagent or rewind fork that
 * nests under a parent and is hidden from the master list (surfaced only in the
 * preview pane). Tangents are deliberately NOT derived: they are user-created
 * side-conversations that stay visible and resumable. Kept as a standalone
 * predicate so the dashboard footer counts the same universe the list renders.
 */
export function isDerivedArtifactSession(s: SessionListingInput): boolean {
  return (
    s.createdReason === 'subagent' ||
    s.createdReason === 'rewind' ||
    // V2 subagent sessions lack createdReason; detect by title pattern.
    (!s.createdReason &&
      /^You are a (subagent|session naming agent|fresh-eyes|memory consolidation)\b/i.test(
        s.title ?? ''
      ))
  );
}

/** Build annotated dashboard entries from a raw listing (shared by groupers). */ export function buildDashboardEntries(
  sessions: SessionListingInput[],
  currentCwd: string,
  activeSessionId?: string | null,
  activeEngine?: SessionListingInput['engine'],
  activeSource?: SessionDiscoverySource
): SessionDashboardEntry[] {
  const normalizedCwd = normalizeWorkspace(currentCwd);
  return sessions.map((s) => ({
    sessionId: s.sessionId,
    title: s.title || '(no title)',
    workspace: normalizeWorkspace(s.cwd || ''),
    isCurrentWorkspace: normalizeWorkspace(s.cwd || '') === normalizedCwd,
    updatedAt: s.updatedAt || '',
    status: s.status,
    executionTarget: s.executionTarget,
    source: s.source,
    messageCount: s.messageCount,
    engine: s.engine,
    // Subagents and rewind forks nest under their parent and are HIDDEN from
    // the master list (surfaced in the preview pane). Tangents are deliberately
    // excluded here — they are user-created side-conversations that stay
    // VISIBLE and resumable, nested via `tangentChildren` instead.
    isSubagent: isDerivedArtifactSession(s),
    parentSessionId: s.parentSessionId,
    createdReason: s.createdReason,
    children: [],
    tangentChildren: [],
    isActive: sessionMatchesActive(
      s,
      activeSessionId,
      activeEngine,
      activeSource
    ),
  }));
}

/**
 * Nest subagent sessions under their parents using each entry's OWN lineage
 * (`isSubagent` + `parentSessionId`), no external map required. Derived
 * sessions of both kinds (subagents and rewind forks) nest under their
 * parent. Orphans (parent not listed) remain top-level but keep their
 * `isSubagent` flag.
 *
 * Mutates and returns the same groups array.
 */
export function applySubagentNesting(
  groups: WorkspaceGroup[]
): WorkspaceGroup[] {
  const entryByIdentity = new Map<string, SessionDashboardEntry>();
  for (const group of groups) {
    for (const session of group.sessions) {
      entryByIdentity.set(sessionIdentityKey(session), session);
    }
  }

  const nestedIdentities = new Set<string>();
  for (const entry of entryByIdentity.values()) {
    if (!entry.isSubagent || !entry.parentSessionId) continue;
    const parent = entryByIdentity.get(
      sessionIdentityKey({
        sessionId: entry.parentSessionId,
        engine: entry.engine,
        source: entry.source,
      })
    );
    if (parent) {
      parent.children.push(entry);
      nestedIdentities.add(sessionIdentityKey(entry));
    }
  }

  for (const group of groups) {
    group.sessions = group.sessions.filter(
      (session) => !nestedIdentities.has(sessionIdentityKey(session))
    );
  }
  return groups;
}

/**
 * Nest tangent sessions under their parent as VISIBLE children.
 *
 * Unlike {@link applySubagentNesting} (which hides its children in the preview
 * pane), tangents are user-created side-conversations that stay resumable, so
 * they move into the parent's `tangentChildren` and are re-emitted as indented
 * rows by the nav model. A tangent whose parent is not in the same group/list
 * (e.g. a deeper tangent whose parent was itself nested away) stays top-level
 * as a flat, still-tagged row rather than vanishing.
 *
 * Runs after {@link applySubagentNesting} so parents are already top-level.
 * Mutates and returns the same groups array.
 */
export function applyTangentNesting(
  groups: WorkspaceGroup[]
): WorkspaceGroup[] {
  const entryByIdentity = new Map<string, SessionDashboardEntry>();
  for (const group of groups) {
    for (const session of group.sessions) {
      entryByIdentity.set(sessionIdentityKey(session), session);
    }
  }

  const nestedIdentities = new Set<string>();
  for (const entry of entryByIdentity.values()) {
    if (entry.createdReason !== 'tangent' || !entry.parentSessionId) continue;
    const parent = entryByIdentity.get(
      sessionIdentityKey({
        sessionId: entry.parentSessionId,
        engine: entry.engine,
        source: entry.source,
      })
    );
    // Don't nest under a parent that is itself a nested tangent child — that
    // parent won't be emitted as a top-level row, so its child would be lost.
    if (parent && parent.createdReason !== 'tangent') {
      parent.tangentChildren.push(entry);
      nestedIdentities.add(sessionIdentityKey(entry));
    }
  }

  for (const group of groups) {
    group.sessions = group.sessions.filter(
      (session) => !nestedIdentities.has(sessionIdentityKey(session))
    );
  }
  return groups;
}

/**
 * Filter sessions across all groups by a text query (case-insensitive
 * substring match on title and workspace label).
 * Returns a new groups array with only matching sessions; empty groups are omitted.
 */
export function filterSessionsByText(
  groups: WorkspaceGroup[],
  query: string
): WorkspaceGroup[] {
  const q = query.trim().toLowerCase();
  if (!q) return groups;

  const filtered: WorkspaceGroup[] = [];
  for (const group of groups) {
    const labelMatches = group.label.toLowerCase().includes(q);
    const matchingSessions = labelMatches
      ? group.sessions
      : group.sessions.filter(
          (s) =>
            s.title.toLowerCase().includes(q) ||
            s.workspace.toLowerCase().includes(q) ||
            // A pasted session id finds its session — matched as an identifier.
            s.sessionId.toLowerCase().includes(q)
        );
    if (matchingSessions.length > 0) {
      filtered.push({ ...group, sessions: matchingSessions });
    }
  }
  return filtered;
}
