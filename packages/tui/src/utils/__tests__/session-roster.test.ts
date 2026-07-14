import { describe, it, expect } from 'bun:test';
import {
  mergeRosterDelta,
  deriveActiveSessionStatus,
  type RosterEntry,
} from '../session-roster';

const empty = new Map<string, RosterEntry>();

describe('mergeRosterDelta', () => {
  it('records a full upsert', () => {
    const r = mergeRosterDelta(empty, {
      upserted: [
        { sessionId: 's1', title: 't', status: 'provisioning', cwd: '/x' },
      ],
      deleted: [],
    });
    expect(r.get('s1')).toEqual({
      sessionId: 's1',
      title: 't',
      status: 'provisioning',
      cwd: '/x',
    });
  });

  it('merges a partial (CDC) upsert over the prior entry without clobbering unchanged fields', () => {
    let r = mergeRosterDelta(empty, {
      upserted: [
        { sessionId: 's1', title: 't', status: 'provisioning', cwd: '/x' },
      ],
      deleted: [],
    });
    r = mergeRosterDelta(r, {
      upserted: [{ sessionId: 's1', status: 'in_progress' }],
      deleted: [],
    });
    expect(r.get('s1')).toEqual({
      sessionId: 's1',
      title: 't',
      status: 'in_progress',
      cwd: '/x',
    });
  });

  it('retracts a session on a deleted delta', () => {
    let r = mergeRosterDelta(empty, {
      upserted: [{ sessionId: 's1', status: 'in_progress' }],
      deleted: [],
    });
    r = mergeRosterDelta(r, { upserted: [], deleted: ['s1'] });
    expect(r.has('s1')).toBe(false);
  });

  it('keeps a partial first sighting as an explicit partial (no field invented)', () => {
    const r = mergeRosterDelta(empty, {
      upserted: [{ sessionId: 's2', status: 'in_progress' }],
      deleted: [],
    });
    expect(r.get('s2')).toEqual({ sessionId: 's2', status: 'in_progress' });
    expect(r.get('s2')?.title).toBeUndefined();
  });

  it('skips an upsert without a sessionId', () => {
    const r = mergeRosterDelta(empty, {
      upserted: [{ sessionId: '' } as RosterEntry, null as never],
      deleted: [],
    });
    expect(r.size).toBe(0);
  });

  it('tolerates missing upserted/deleted arrays', () => {
    const r = mergeRosterDelta(
      empty,
      {} as Parameters<typeof mergeRosterDelta>[1]
    );
    expect(r.size).toBe(0);
  });

  it('does not mutate the input roster', () => {
    const base = mergeRosterDelta(empty, {
      upserted: [{ sessionId: 's1', status: 'idle' }],
      deleted: [],
    });
    mergeRosterDelta(base, { upserted: [], deleted: ['s1'] });
    expect(base.has('s1')).toBe(true);
  });
});

describe('deriveActiveSessionStatus', () => {
  const roster = mergeRosterDelta(empty, {
    upserted: [
      {
        sessionId: 'active',
        status: 'failed',
        provisioningFailure: { code: 'timeout' },
      },
      { sessionId: 'other', status: 'in_progress' },
    ],
    deleted: [],
  });

  it('returns the active session status with its provisioning failure', () => {
    expect(deriveActiveSessionStatus(roster, 'active')).toEqual({
      status: 'failed',
      provisioningFailure: { code: 'timeout' },
    });
  });

  it('omits provisioningFailure when the entry has none', () => {
    expect(deriveActiveSessionStatus(roster, 'other')).toEqual({
      status: 'in_progress',
    });
  });

  it('returns null for a session the roster has never seen (e.g. local)', () => {
    expect(deriveActiveSessionStatus(roster, 'unknown')).toBeNull();
    expect(deriveActiveSessionStatus(roster, null)).toBeNull();
  });

  it('returns null after the active session is retracted, so the status resets instead of going stale', () => {
    const gone = mergeRosterDelta(roster, {
      upserted: [],
      deleted: ['active'],
    });
    expect(deriveActiveSessionStatus(gone, 'active')).toBeNull();
  });

  it('returns null for an entry whose status has not arrived yet (partial first upsert)', () => {
    const partial = mergeRosterDelta(empty, {
      upserted: [{ sessionId: 'p1', title: 'only-title' }],
      deleted: [],
    });
    expect(deriveActiveSessionStatus(partial, 'p1')).toBeNull();
  });
});
