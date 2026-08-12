import { describe, it, expect } from 'bun:test';
import { buildNavModel } from '../session-dashboard-nav';
import type {
  SessionDashboardEntry,
  WorkspaceGroup,
} from '../session-dashboard';

const PINNED = '\u0000bookmarked';

function entry(id: string): SessionDashboardEntry {
  return {
    sessionId: id,
    title: id,
    workspace: '/w',
    isCurrentWorkspace: false,
    updatedAt: '2026-01-01T00:00:00.000Z',
    isSubagent: false,
    children: [],
    tangentChildren: [],
    isActive: false,
  };
}

function group(
  workspace: string,
  n: number,
  opts: Partial<WorkspaceGroup> = {}
): WorkspaceGroup {
  return {
    workspace,
    label: workspace,
    isCurrent: false,
    sessions: Array.from({ length: n }, (_, i) => entry(`${workspace}-${i}`)),
    ...opts,
  };
}

function build(
  groups: WorkspaceGroup[],
  opts: Partial<Parameters<typeof buildNavModel>[0]> = {}
) {
  return buildNavModel({
    groups,
    isRowVisible: () => true,
    expandedGroups: new Set(),
    isSearching: false,
    groupBy: 'workspace',
    pinnedGroupKey: PINNED,
    ...opts,
  });
}

describe('buildNavModel', () => {
  it('caps ordinary groups at 5 with an expander row', () => {
    const m = build([group('/a', 8)]);
    const sessions = m.navItems.filter((i) => i.type === 'session');
    const expanders = m.navItems.filter((i) => i.type === 'expand');
    expect(sessions).toHaveLength(5);
    expect(expanders).toHaveLength(1);
    expect(expanders[0]).toMatchObject({ workspace: '/a' });
  });

  it('caps prominent groups (current / Today / Active) at 10', () => {
    const current = build([group('/a', 15, { isCurrent: true })]);
    expect(current.navItems.filter((i) => i.type === 'session')).toHaveLength(
      10
    );
    const today = build([group('/b', 15, { label: 'Today' })]);
    expect(today.navItems.filter((i) => i.type === 'session')).toHaveLength(10);
  });

  it('shows everything for an expanded group (no expander row)', () => {
    const m = build([group('/a', 8)], { expandedGroups: new Set(['/a']) });
    expect(m.navItems.filter((i) => i.type === 'session')).toHaveLength(8);
    expect(m.navItems.filter((i) => i.type === 'expand')).toHaveLength(0);
  });

  it('search shows all matches uncapped', () => {
    const m = build([group('/a', 30)], { isSearching: true });
    expect(m.navItems.filter((i) => i.type === 'session')).toHaveLength(30);
    expect(m.navItems.filter((i) => i.type === 'expand')).toHaveLength(0);
  });

  it('groupBy none is uncapped', () => {
    const m = build([group('/a', 30)], { groupBy: 'none' });
    expect(m.navItems.filter((i) => i.type === 'session')).toHaveLength(30);
  });

  it('the pinned group is always fully expanded', () => {
    const m = build([group(PINNED, 12)]);
    expect(m.navItems.filter((i) => i.type === 'session')).toHaveLength(12);
    expect(m.navItems.filter((i) => i.type === 'expand')).toHaveLength(0);
  });

  it('skips groups whose rows are all invisible', () => {
    const m = build([group('/a', 3), group('/b', 3)], {
      isRowVisible: (s) => !s.sessionId.startsWith('/a'),
    });
    expect(m.groupKeys).toEqual(['/b']);
    expect(m.groupStarts).toEqual([0]);
  });

  it('eligibleTotal counts rows behind expanders, not just visible ones', () => {
    const m = build([group('/a', 8), group('/b', 2)]);
    expect(m.eligibleTotal).toBe(10);
  });

  it('groupStarts / groupKeys / navGroupIdx stay aligned across groups', () => {
    const m = build([group('/a', 7), group('/b', 2), group('/c', 6)]);
    // /a: 5 rows + expander (6 items), /b: 2 rows, /c: 5 rows + expander.
    expect(m.groupStarts).toEqual([0, 6, 8]);
    expect(m.groupKeys).toEqual(['/a', '/b', '/c']);
    expect(m.navGroupIdx[0]).toBe(0);
    expect(m.navGroupIdx[5]).toBe(0); // /a's expander belongs to /a
    expect(m.navGroupIdx[6]).toBe(1);
    expect(m.navGroupIdx[8]).toBe(2);
    expect(m.navGroupIdx).toHaveLength(m.navItems.length);
  });

  it('display items interleave one header per rendered group', () => {
    const m = build([group('/a', 2), group('/b', 2)]);
    const headers = m.displayItems.filter((d) => d.type === 'header');
    expect(headers).toHaveLength(2);
    expect(m.displayItems[0]!.type).toBe('header');
  });

  it('preserves source order within a group (empties by recency, not sunk)', () => {
    const g = group('/a', 3);
    g.sessions = [entry('newest'), entry('mid'), entry('oldest')];
    const m = build([g]);
    const ids = m.navItems
      .filter((i) => i.type === 'session')
      .map((i) => (i.type === 'session' ? i.entry.sessionId : ''));
    expect(ids).toEqual(['newest', 'mid', 'oldest']);
  });
});
