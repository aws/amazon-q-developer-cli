import { describe, it, expect } from 'bun:test';
import {
  groupSessions,
  recencyBucket,
  type MetaLookup,
} from '../session-grouping';
import type { SessionListingInput } from '../session-dashboard';

const NOW = new Date('2026-07-28T12:00:00.000Z').getTime();
const daysAgo = (n: number) =>
  new Date(NOW - n * 24 * 60 * 60 * 1000).toISOString();

function mk(id: string, cwd: string, updatedAt: string): SessionListingInput {
  return { sessionId: id, cwd, title: id, updatedAt };
}

// Default meta: nothing bookmarked, no tags.
const emptyMeta: MetaLookup = {
  isBookmarked: () => false,
  getTags: () => [],
};

describe('recencyBucket', () => {
  it('classifies by calendar day and rolling windows', () => {
    expect(recencyBucket('2026-07-28T09:00:00.000Z', NOW)).toBe('Today');
    expect(recencyBucket('2026-07-27T23:00:00.000Z', NOW)).toBe('Yesterday');
    expect(recencyBucket(daysAgo(2), NOW)).toBe('Last 3 days');
    expect(recencyBucket(daysAgo(5), NOW)).toBe('Last week');
    expect(recencyBucket(daysAgo(20), NOW)).toBe('Last month');
    expect(recencyBucket(daysAgo(90), NOW)).toBe('Older');
    expect(recencyBucket('', NOW)).toBe('Unknown');
    expect(recencyBucket('garbage', NOW)).toBe('Unknown');
  });
});

describe('groupSessions — workspace', () => {
  it('groups by cwd with current pinned first', () => {
    const sessions = [
      mk('a', '/w/alpha', daysAgo(1)),
      mk('b', '/w/beta', daysAgo(1)),
      mk('c', '/w/alpha', daysAgo(2)),
    ];
    const groups = groupSessions(sessions, '/w/beta', {
      groupBy: 'workspace',
      meta: emptyMeta,
      now: NOW,
    });
    expect(groups[0]!.isCurrent).toBe(true);
    expect(groups[0]!.workspace).toBe('/w/beta');
    expect(
      groups.find((g) => g.workspace === '/w/alpha')!.sessions
    ).toHaveLength(2);
  });
});

describe('groupSessions — recency', () => {
  it('buckets by time in chronological order', () => {
    const sessions = [
      mk('today', '/w', '2026-07-28T08:00:00.000Z'),
      mk('yest', '/w', '2026-07-27T10:00:00.000Z'),
      mk('week', '/w', daysAgo(5)),
      mk('old', '/w', daysAgo(100)),
    ];
    const groups = groupSessions(sessions, '/w', {
      groupBy: 'recency',
      meta: emptyMeta,
      now: NOW,
    });
    expect(groups.map((g) => g.label)).toEqual([
      'Today',
      'Yesterday',
      'Last week',
      'Older',
    ]);
    // Empty buckets (Last 3 days, Last month) are omitted.
    expect(groups.find((g) => g.label === 'Last 3 days')).toBeUndefined();
  });

  it('marks Today as the current group', () => {
    const groups = groupSessions(
      [mk('t', '/w', '2026-07-28T08:00:00.000Z')],
      '/w',
      { groupBy: 'recency', meta: emptyMeta, now: NOW }
    );
    expect(groups[0]!.isCurrent).toBe(true);
  });
});

describe('groupSessions — status', () => {
  function mkStatus(id: string, status?: string): SessionListingInput {
    return {
      sessionId: id,
      cwd: '/w',
      title: id,
      updatedAt: daysAgo(1),
      status: status as any,
    };
  }

  it('buckets by activity status in priority order, omitting empty buckets', () => {
    const sessions = [
      mkStatus('work', 'in_progress'),
      mkStatus('wait', 'waiting_on_user'),
      mkStatus('idle1', 'idle'),
      mkStatus('done1', 'completed'),
      mkStatus('none1', undefined),
    ];
    const groups = groupSessions(sessions, '/w', {
      groupBy: 'status',
      meta: emptyMeta,
      now: NOW,
    });
    expect(groups.map((g) => g.label)).toEqual([
      'Waiting on you',
      'Working',
      'Idle',
      'Done',
      'No status',
    ]);
    expect(groups.find((g) => g.label === 'Failed')).toBeUndefined();
  });

  it('marks Waiting on you as the current group', () => {
    const groups = groupSessions(
      [mkStatus('w', 'in_progress'), mkStatus('n', 'waiting_on_user')],
      '/w',
      { groupBy: 'status', meta: emptyMeta, now: NOW }
    );
    expect(groups[0]!.label).toBe('Waiting on you');
    expect(groups[0]!.isCurrent).toBe(true);
  });
});

describe('groupSessions — none', () => {
  it('returns a single recency-sorted All group', () => {
    const sessions = [
      mk('old', '/w', daysAgo(10)),
      mk('new', '/w', daysAgo(1)),
    ];
    const groups = groupSessions(sessions, '/w', {
      groupBy: 'none',
      meta: emptyMeta,
      now: NOW,
    });
    expect(groups).toHaveLength(1);
    expect(groups[0]!.label).toContain('All sessions (2)');
    expect(groups[0]!.sessions.map((s) => s.sessionId)).toEqual(['new', 'old']);
  });

  it('returns empty for no sessions', () => {
    expect(
      groupSessions([], '/w', { groupBy: 'none', meta: emptyMeta, now: NOW })
    ).toHaveLength(0);
  });
});

describe('groupSessions — filters', () => {
  const sessions = [
    mk('cur1', '/w/current', daysAgo(1)),
    mk('cur2', '/w/current', daysAgo(2)),
    mk('other', '/w/other', daysAgo(1)),
  ];

  it('currentWorkspaceOnly keeps only cwd sessions', () => {
    const groups = groupSessions(sessions, '/w/current', {
      groupBy: 'none',
      filters: { currentWorkspaceOnly: true },
      meta: emptyMeta,
      now: NOW,
    });
    const ids = groups[0]!.sessions.map((s) => s.sessionId).sort();
    expect(ids).toEqual(['cur1', 'cur2']);
  });

  it('bookmarkedOnly keeps only bookmarked sessions', () => {
    const meta: MetaLookup = {
      isBookmarked: (id) => id === 'other',
      getTags: () => [],
    };
    const groups = groupSessions(sessions, '/w/current', {
      groupBy: 'none',
      filters: { bookmarkedOnly: true },
      meta,
      now: NOW,
    });
    expect(groups[0]!.sessions.map((s) => s.sessionId)).toEqual(['other']);
  });

  it('tag filter keeps only tagged sessions', () => {
    const meta: MetaLookup = {
      isBookmarked: () => false,
      getTags: (id) => (id === 'cur1' ? ['wip'] : []),
    };
    const groups = groupSessions(sessions, '/w/current', {
      groupBy: 'none',
      filters: { tag: 'wip' },
      meta,
      now: NOW,
    });
    expect(groups[0]!.sessions.map((s) => s.sessionId)).toEqual(['cur1']);
  });

  it('composes filters (AND)', () => {
    const meta: MetaLookup = {
      isBookmarked: (id) => id !== 'other',
      getTags: () => ['wip'],
    };
    const groups = groupSessions(sessions, '/w/current', {
      groupBy: 'none',
      filters: { currentWorkspaceOnly: true, bookmarkedOnly: true, tag: 'wip' },
      meta,
      now: NOW,
    });
    // current (cur1,cur2) ∩ bookmarked (cur1,cur2) ∩ tagged wip (all) = cur1,cur2
    expect(groups[0]!.sessions.map((s) => s.sessionId).sort()).toEqual([
      'cur1',
      'cur2',
    ]);
  });

  it('filters that match nothing yield no groups', () => {
    const groups = groupSessions(sessions, '/w/current', {
      groupBy: 'workspace',
      filters: { tag: 'nonexistent' },
      meta: emptyMeta,
      now: NOW,
    });
    expect(groups).toHaveLength(0);
  });
});
