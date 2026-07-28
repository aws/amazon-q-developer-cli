import { describe, it, expect } from 'bun:test';
import {
  SessionLifecycleOwner,
  type AgentSession,
} from '../../../types/multi-session.js';
import { MessageRole, type MessageType } from '../../../stores/app-store.js';
import {
  selectActiveSubagentToolScopes,
  selectSubagentToolMessagesForScope,
  selectSubagentToolSessions,
  activePipelineGroupConstraint,
  selectScopeSeedSessions,
} from '../subagent-session-filter.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deduplicateSessions(sessions: AgentSession[]): AgentSession[] {
  return selectSubagentToolSessions(sessions);
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
// 1. Generic orchestration session isolation and loop deduplication
// ---------------------------------------------------------------------------

describe('SubagentToolPanel loop deduplication', () => {
  it('keeps interleaved unfinished pipeline invocations isolated', () => {
    const messages: MessageType[] = [
      {
        id: 'parent-a',
        role: MessageRole.ToolUse,
        name: 'orchestrate_subagent',
        pipelineGroupId: 'group-a',
        content: '{}',
      },
      {
        id: 'a-before',
        role: MessageRole.ToolUse,
        name: 'read',
        sessionId: 'session-a',
        pipelineGroupId: 'group-a',
        agentName: 'worker',
        content: '{}',
      },
      {
        id: 'parent-b',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        pipelineGroupId: 'group-b',
        content: '{}',
      },
      {
        id: 'b-after',
        role: MessageRole.ToolUse,
        name: 'write',
        sessionId: 'session-b',
        pipelineGroupId: 'group-b',
        agentName: 'worker',
        content: '{}',
      },
      {
        id: 'a-after',
        role: MessageRole.ToolUse,
        name: 'shell',
        sessionId: 'session-a',
        pipelineGroupId: 'group-a',
        agentName: 'worker',
        content: '{}',
      },
    ];

    const scopes = selectActiveSubagentToolScopes(messages);
    expect(scopes.map((scope) => scope.parentToolCallId)).toEqual([
      'parent-a',
      'parent-b',
    ]);
    expect(
      selectSubagentToolMessagesForScope(messages, scopes[0]!).map(
        (message) => message.id
      )
    ).toEqual(['a-before', 'a-after']);
    expect(
      selectSubagentToolMessagesForScope(messages, scopes[1]!).map(
        (message) => message.id
      )
    ).toEqual(['b-after']);

    const sessions = [
      makeSession({
        id: 'session-a',
        name: 'worker',
        group: 'group-a',
      }),
      makeSession({
        id: 'session-b',
        name: 'worker',
        group: 'group-b',
      }),
      makeSession({
        id: 'stale',
        name: 'worker',
        group: 'old-group',
      }),
    ];
    const selected = selectSubagentToolSessions(sessions, {
      pipelineGroupIds: new Set(['group-a', 'group-b']),
    });
    expect(selected.map((session) => session.id)).toEqual([
      'session-a',
      'session-b',
    ]);
  });

  it('excludes workflow children from generic orchestration rows', () => {
    const sessions = [
      makeSession({
        id: 'workflow-child',
        name: 'branch-alpha',
        group: 'workflow',
        lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
      }),
      makeSession({
        id: 'crew-child',
        name: 'current-agent',
        group: 'pipeline-current',
      }),
    ];

    const result = selectSubagentToolSessions(sessions);
    expect(result.map((session) => session.id)).toEqual(['crew-child']);
  });

  it('scopes rows to the current pipeline invocation', () => {
    const sessions = [
      makeSession({
        id: 'old-busy',
        name: 'old-agent',
        group: 'pipeline-previous',
        status: 'busy',
      }),
      makeSession({
        id: 'old-done',
        name: 'old-reviewer',
        group: 'pipeline-previous',
        status: 'terminated',
      }),
      makeSession({
        id: 'current-done',
        name: 'research',
        group: 'pipeline-current',
        status: 'terminated',
      }),
      makeSession({
        id: 'current-busy',
        name: 'implement',
        group: 'pipeline-current',
        status: 'busy',
      }),
    ];

    const result = selectSubagentToolSessions(sessions, {
      pipelineGroupId: 'pipeline-current',
    });
    expect(result.map((session) => session.id)).toEqual([
      'current-done',
      'current-busy',
    ]);
  });

  it('excludes the main session and pending placeholders', () => {
    const sessions = [
      makeSession({ id: 'main', name: 'main' }),
      makeSession({ id: 'pending:review', name: 'review' }),
      makeSession({ id: 'crew', name: 'crew' }),
    ];

    const result = selectSubagentToolSessions(sessions, {
      mainSessionId: 'main',
    });
    expect(result.map((session) => session.id)).toEqual(['crew']);
  });

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

// ---------------------------------------------------------------------------
// V2 (local ACP Rust engine) footer scoping
//
// V2 tags crew sessions with a `crew-*` group (via subagent_list_update) but
// never emits meta.kiro.pipeline.groupId, so tool MESSAGES have
// pipelineGroupId === undefined while SESSIONS carry a defined group. KAS (V3)
// sets both sides to the same value. These tests pin the mixed V2 state so the
// Lite footer strip keeps rendering current crew stages instead of dropping
// them (and does not resurrect the V3 per-invocation scoping regression).
// ---------------------------------------------------------------------------

describe('V2 ungrouped footer scoping', () => {
  it('activePipelineGroupConstraint: no group gate when a scope is ungrouped (V2)', () => {
    const scopes = selectActiveSubagentToolScopes([
      {
        id: 'parent-v2',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        content: '{}',
      },
    ] as MessageType[]);
    expect(scopes).toHaveLength(1);
    expect(scopes[0]!.pipelineGroupId).toBeUndefined();
    // undefined == "no constraint" == show all ephemeral (like the inline panel).
    expect(activePipelineGroupConstraint(scopes)).toBeUndefined();
  });

  it('activePipelineGroupConstraint: exact per-invocation groups when all grouped (V3/KAS)', () => {
    const scopes = selectActiveSubagentToolScopes([
      {
        id: 'pa',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        pipelineGroupId: 'group-a',
        content: '{}',
      },
      {
        id: 'pb',
        role: MessageRole.ToolUse,
        name: 'orchestrate_subagent',
        pipelineGroupId: 'group-b',
        content: '{}',
      },
    ] as MessageType[]);
    const constraint = activePipelineGroupConstraint(scopes);
    expect(constraint).toBeInstanceOf(Set);
    expect([...constraint!].sort()).toEqual(['group-a', 'group-b']);
  });

  it('seeds V2 crew sessions even though messages carry no pipelineGroupId (regression)', () => {
    const messages: MessageType[] = [
      {
        id: 'parent-v2',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        content: '{}',
      },
    ];
    const scopes = selectActiveSubagentToolScopes(messages);
    const constraint = activePipelineGroupConstraint(scopes);

    const sessions = [
      makeSession({ id: 's-research', name: 'research', group: 'crew-foo' }),
      makeSession({ id: 's-impl', name: 'implement', group: 'crew-foo' }),
    ];
    // Pre-fix: constraint was Set{undefined}, so every crew session (group
    // "crew-foo") was excluded here and the footer lost its session seeds.
    const crew = selectSubagentToolSessions(sessions, {
      pipelineGroupIds: constraint,
    });
    expect(crew.map((s) => s.id).sort()).toEqual(['s-impl', 's-research']);

    const seeded = selectScopeSeedSessions(scopes[0]!, crew);
    expect(seeded.map((s) => s.name).sort()).toEqual(['implement', 'research']);
  });

  it('owns in-window child messages for an ungrouped scope despite crew session groups', () => {
    const messages: MessageType[] = [
      {
        id: 'parent-v2',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        content: '{}',
      },
      {
        id: 'c1',
        role: MessageRole.ToolUse,
        name: 'read',
        sessionId: 's-research',
        agentName: 'research',
        content: '{}',
      },
      {
        id: 'c2',
        role: MessageRole.ToolUse,
        name: 'shell',
        sessionId: 's-impl',
        agentName: 'implement',
        content: '{}',
      },
    ];
    const [scope] = selectActiveSubagentToolScopes(messages);
    // Once crew sessions seed the strip, sessionGroupById resolves these
    // children to "crew-foo". The ungrouped scope must still own them —
    // otherwise ("crew-foo" !== undefined) the seed rows never get tool
    // activity and never transition to complete.
    const sessionGroupById = new Map<string, string | undefined>([
      ['s-research', 'crew-foo'],
      ['s-impl', 'crew-foo'],
    ]);
    const owned = selectSubagentToolMessagesForScope(
      messages,
      scope!,
      sessionGroupById
    );
    expect(owned.map((m) => m.id)).toEqual(['c1', 'c2']);
  });

  it('grouped scopes (V3/KAS) still exclude sessions from other invocations', () => {
    const [scope] = selectActiveSubagentToolScopes([
      {
        id: 'p',
        role: MessageRole.ToolUse,
        name: 'agent_crew',
        pipelineGroupId: 'group-current',
        content: '{}',
      },
    ] as MessageType[]);
    const sessions = [
      makeSession({ id: 's-cur', name: 'research', group: 'group-current' }),
      makeSession({ id: 's-old', name: 'research', group: 'group-old' }),
    ];
    const crew = selectSubagentToolSessions(sessions, {
      pipelineGroupIds: activePipelineGroupConstraint([scope!]),
    });
    expect(crew.map((s) => s.id)).toEqual(['s-cur']);
    expect(selectScopeSeedSessions(scope!, crew).map((s) => s.id)).toEqual([
      's-cur',
    ]);
  });
});
