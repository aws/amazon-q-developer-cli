/**
 * Nav-model building for the session dashboard list: which rows are
 * navigable, where each group starts, what the expanders hide, and the
 * honest session total under the active filter — all derived from the
 * filtered groups by one pass.
 */

import type {
  SessionDashboardEntry,
  WorkspaceGroup,
} from './session-dashboard.js';
import type { GroupByDimension } from './session-grouping.js';

/** One navigable row: a session, or a group's "show more" expander. */
export type NavItem =
  | { type: 'session'; entry: SessionDashboardEntry }
  | { type: 'expand'; workspace: string };

/** One rendered row: a nav item or a non-navigable group header. */
export type DisplayItem = NavItem | { type: 'header'; group: WorkspaceGroup };

export interface NavModelInput {
  groups: readonly WorkspaceGroup[];
  /** The shared row-visibility predicate (empties/orphan-subagent rules). */
  isRowVisible: (s: SessionDashboardEntry) => boolean;
  /** Groups the user expanded (by workspace key). */
  expandedGroups: ReadonlySet<string>;
  /** Searching shows all matches — no caps, no expanders. */
  isSearching: boolean;
  groupBy: GroupByDimension;
  /** The pinned Bookmarked group's sentinel key — always fully expanded. */
  pinnedGroupKey: string;
}

export interface NavModel {
  navItems: NavItem[];
  displayItems: DisplayItem[];
  /** Nav index of each group's first row — target list for ←/→ jumps. */
  groupStarts: number[];
  /** Workspace key of each rendered group, parallel to `groupStarts`. */
  groupKeys: string[];
  /** Group ordinal per nav index (for band-cost accounting and ← collapse). */
  navGroupIdx: number[];
  /** Sessions reachable in this view (visible + behind expanders). */
  eligibleTotal: number;
}

/** Cap: 10 rows for the current workspace / today / active groups, 5 for
 *  everything else. Keeps the overview scannable while giving the most
 *  relevant groups breathing room. */
function groupCap(group: WorkspaceGroup): number {
  const isProminent =
    group.isCurrent || group.label === 'Today' || group.label === 'Active';
  return isProminent ? 10 : 5;
}

export function buildNavModel(input: NavModelInput): NavModel {
  const nav: NavItem[] = [];
  const display: DisplayItem[] = [];
  const starts: number[] = [];
  const keys: string[] = [];
  let eligible = 0;

  for (const group of input.groups) {
    const isPinned = group.workspace === input.pinnedGroupKey;
    const expanded =
      isPinned ||
      input.isSearching ||
      input.expandedGroups.has(group.workspace);
    // Source order is preserved so empties surface by recency instead of
    // sinking below every cap.
    const ordered = group.sessions.filter(input.isRowVisible);
    // Groups with nothing visible are pure clutter — skip them.
    if (ordered.length === 0) continue;
    display.push({ type: 'header', group });
    starts.push(nav.length);
    keys.push(group.workspace);
    eligible += ordered.length;
    for (const s of ordered) {
      eligible += s.tangentChildren.filter(input.isRowVisible).length;
    }
    const cap =
      input.groupBy === 'none' || input.isSearching
        ? ordered.length
        : groupCap(group);
    const visible = expanded ? ordered : ordered.slice(0, cap);
    for (const session of visible) {
      const item: NavItem = { type: 'session', entry: session };
      nav.push(item);
      display.push(item);
      // Tangent children ride along with their (visible) parent regardless of
      // the group cap, so a resumable side-conversation is never cut off from
      // the parent it belongs to.
      for (const child of session.tangentChildren) {
        if (!input.isRowVisible(child)) continue;
        const childItem: NavItem = { type: 'session', entry: child };
        nav.push(childItem);
        display.push(childItem);
      }
    }
    if (ordered.length > visible.length) {
      const expander: NavItem = { type: 'expand', workspace: group.workspace };
      nav.push(expander);
      display.push(expander);
    }
  }

  const navGroupIdx: number[] = new Array(nav.length);
  let g = -1;
  for (let i = 0; i < nav.length; i++) {
    if (g + 1 < starts.length && starts[g + 1] === i) g++;
    navGroupIdx[i] = g;
  }

  return {
    navItems: nav,
    displayItems: display,
    groupStarts: starts,
    groupKeys: keys,
    navGroupIdx,
    eligibleTotal: eligible,
  };
}
