import { describe, it, expect } from 'bun:test';
import type { AgentSession } from '../../../types/multi-session.js';

// ---------------------------------------------------------------------------
// Helpers — replicate the deduplication logic from SubagentToolPanel
// ---------------------------------------------------------------------------

function deduplicateSessions(sessions: AgentSession[]): AgentSession[] {
  const deduped = new Map<string, AgentSession>();
  for (const s of sessions) {
    const key = `${s.group ?? ''}::${s.name}`;
    const existing = deduped.get(key);
    if (!existing) {
      deduped.set(key, s);
    } else if ((s.loopIteration ?? 0) > (existing.loopIteration ?? 0)) {
      deduped.set(key, s);
    } else if ((s.loopIteration ?? 0) === (existing.loopIteration ?? 0)) {
      if (s.created.getTime() > existing.created.getTime()) {
        deduped.set(key, s);
      }
    }
  }
  return [...deduped.values()];
}

const makeSession = (overrides: Partial<AgentSession>): AgentSession => ({
  id: 'sess-1',
  name: 'writer',
  status: 'busy',
  type: 'ephemeral',
  created: new Date(1000),
  lastActivity: new Date(),
  ...overrides,
});

// ---------------------------------------------------------------------------
// 1. SubagentToolPanel deduplication (outside view)
// ---------------------------------------------------------------------------

describe('SubagentToolPanel loop deduplication', () => {
  it('keeps only the latest loop iteration per name+group', () => {
    const sessions = [
      makeSession({
        id: 's1',
        name: 'writer',
        status: 'terminated',
        group: 'g1',
        loopIteration: 0,
        created: new Date(100),
      }),
      makeSession({
        id: 's2',
        name: 'writer',
        status: 'busy',
        group: 'g1',
        loopIteration: 1,
        created: new Date(200),
      }),
    ];

    const result = deduplicateSessions(sessions);
    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe('s2');
    expect(result[0]!.status).toBe('busy');
  });

  it('does not show stale Completed when new iteration is running', () => {
    const sessions = [
      makeSession({
        id: 's1',
        name: 'writer',
        status: 'terminated',
        group: 'g1',
        loopIteration: 0,
        created: new Date(100),
      }),
      makeSession({
        id: 's3',
        name: 'writer',
        status: 'busy',
        group: 'g1',
        loopIteration: 0,
        created: new Date(300),
      }),
      makeSession({
        id: 's2',
        name: 'reviewer',
        status: 'terminated',
        group: 'g1',
        loopIteration: 0,
        created: new Date(200),
      }),
    ];

    const result = deduplicateSessions(sessions);
    expect(result).toHaveLength(2);
    const writer = result.find((s) => s.name === 'writer');
    expect(writer!.id).toBe('s3');
    expect(writer!.status).toBe('busy');
  });

  it('does not deduplicate across different groups', () => {
    const sessions = [
      makeSession({
        id: 's1',
        name: 'writer',
        group: 'g1',
        created: new Date(100),
      }),
      makeSession({
        id: 's2',
        name: 'writer',
        group: 'g2',
        created: new Date(200),
      }),
    ];

    const result = deduplicateSessions(sessions);
    expect(result).toHaveLength(2);
  });

  it('prefers most recently created at same iteration', () => {
    const sessions = [
      makeSession({
        id: 's1',
        name: 'writer',
        status: 'terminated',
        group: 'g1',
        loopIteration: 0,
        created: new Date(100),
      }),
      makeSession({
        id: 's2',
        name: 'writer',
        status: 'busy',
        group: 'g1',
        loopIteration: 0,
        created: new Date(200),
      }),
    ];

    const result = deduplicateSessions(sessions);
    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe('s2');
  });

  it('handles sessions without group field', () => {
    const sessions = [
      makeSession({
        id: 's1',
        name: 'writer',
        status: 'terminated',
        loopIteration: 0,
        created: new Date(100),
      }),
      makeSession({
        id: 's2',
        name: 'writer',
        status: 'busy',
        loopIteration: 1,
        created: new Date(200),
      }),
    ];

    const result = deduplicateSessions(sessions);
    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe('s2');
  });
});

// ---------------------------------------------------------------------------
// 2. Targeted session cleanup (onSessionEvent)
// ---------------------------------------------------------------------------

describe('Session cleanup on loop re-spawn', () => {
  /**
   * Replicate the targeted cleanup logic from index.tsx onSessionEvent:
   * Only clear conversations for terminated sessions that share the same
   * name+group as the new session.
   */
  function getSessionsToClear(
    sessions: Map<string, AgentSession>,
    newSession: { name: string; group?: string; status: string }
  ): string[] {
    if (newSession.status !== 'busy') return [];
    const cleared: string[] = [];
    const newGroup = newSession.group ?? '';
    for (const [id, s] of sessions) {
      if (
        s.status === 'terminated' &&
        s.name === newSession.name &&
        (s.group ?? '') === newGroup
      ) {
        cleared.push(id);
      }
    }
    return cleared;
  }

  it('only clears conversations for same name+group', () => {
    const sessions = new Map<string, AgentSession>([
      [
        's1',
        makeSession({
          id: 's1',
          name: 'writer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
      [
        's2',
        makeSession({
          id: 's2',
          name: 'reviewer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
    ]);

    const cleared = getSessionsToClear(sessions, {
      name: 'writer',
      group: 'g1',
      status: 'busy',
    });

    expect(cleared).toEqual(['s1']);
  });

  it('does not clear sessions from different groups', () => {
    const sessions = new Map<string, AgentSession>([
      [
        's1',
        makeSession({
          id: 's1',
          name: 'writer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
      [
        's2',
        makeSession({
          id: 's2',
          name: 'writer',
          status: 'terminated',
          group: 'g2',
        }),
      ],
    ]);

    const cleared = getSessionsToClear(sessions, {
      name: 'writer',
      group: 'g1',
      status: 'busy',
    });

    expect(cleared).toEqual(['s1']);
  });

  it('preserves other completed stages when writer re-spawns', () => {
    const sessions = new Map<string, AgentSession>([
      [
        's1',
        makeSession({
          id: 's1',
          name: 'writer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
      [
        's2',
        makeSession({
          id: 's2',
          name: 'reviewer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
      [
        's3',
        makeSession({
          id: 's3',
          name: 'linter',
          status: 'terminated',
          group: 'g1',
        }),
      ],
    ]);

    const cleared = getSessionsToClear(sessions, {
      name: 'writer',
      group: 'g1',
      status: 'busy',
    });

    // Only writer cleared, reviewer and linter preserved
    expect(cleared).toEqual(['s1']);
  });

  it('does nothing when new session is not busy', () => {
    const sessions = new Map<string, AgentSession>([
      [
        's1',
        makeSession({
          id: 's1',
          name: 'writer',
          status: 'terminated',
          group: 'g1',
        }),
      ],
    ]);

    const cleared = getSessionsToClear(sessions, {
      name: 'writer',
      group: 'g1',
      status: 'idle',
    });

    expect(cleared).toEqual([]);
  });
});
