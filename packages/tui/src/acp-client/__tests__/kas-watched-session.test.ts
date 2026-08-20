import { describe, expect, it } from 'bun:test';
import {
  AgentEventType,
  type AgentStreamEvent,
} from '../../types/agent-events';
import {
  isWatchedSession,
  KasAcpClient,
  parseSpecTaskStatusNotification,
} from '../kas';

class TestKasAcpClient extends KasAcpClient {
  globalEvents!: AgentStreamEvent[];

  protected override broadcastStreamEvent(event: AgentStreamEvent): void {
    this.globalEvents.push(event);
  }

  routeTurnBoundary(
    kind: 'turn_start' | 'turn_end',
    sink: (event: AgentStreamEvent) => void
  ): void {
    this.convertAcpUpdateToEvent(
      {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind } },
      } as never,
      'spec-session',
      sink
    );
  }
}

/**
 * Pins the scope of session-tagged notifications. A spec task run executes on
 * the spec's own session while the user stays in their chat, so scoping to the
 * driven session alone discards the progress of every watched run — which is
 * indistinguishable, on screen, from a run that never starts.
 */
describe('isWatchedSession', () => {
  const CHAT = 'sess_chat-0f31';
  const SPEC = 'sess_spec-b9e0';
  const OTHER = 'sess_other-77c2';

  it('accepts the session the client drives', () => {
    expect(isWatchedSession(CHAT, CHAT, new Set())).toBe(true);
  });

  it('accepts a session the client only observes', () => {
    expect(isWatchedSession(SPEC, CHAT, new Set([SPEC]))).toBe(true);
  });

  it('rejects a session it neither drives nor observes', () => {
    expect(isWatchedSession(OTHER, CHAT, new Set([SPEC]))).toBe(false);
  });

  it('rejects an untagged notification', () => {
    expect(isWatchedSession(undefined, CHAT, new Set([SPEC]))).toBe(false);
  });

  it('rejects everything before a session exists', () => {
    expect(isWatchedSession(SPEC, undefined, new Set())).toBe(false);
  });

  it('stops accepting a session once it is no longer observed', () => {
    const observed = new Set([SPEC]);
    expect(isWatchedSession(SPEC, CHAT, observed)).toBe(true);
    observed.delete(SPEC);
    expect(isWatchedSession(SPEC, CHAT, observed)).toBe(false);
  });

  it('routes watched turn boundaries through the session sink', () => {
    const sinkEvents: AgentStreamEvent[] = [];
    const client = Object.create(
      TestKasAcpClient.prototype
    ) as TestKasAcpClient;
    client.globalEvents = [];
    client.routeTurnBoundary('turn_start', (event) => sinkEvents.push(event));

    expect(sinkEvents).toEqual([
      { type: AgentEventType.TurnStart, sessionId: 'spec-session' },
    ]);
    expect(client.globalEvents).toEqual([]);
  });
});

describe('parseSpecTaskStatusNotification', () => {
  it('accepts valid changes and drops malformed entries', () => {
    expect(
      parseSpecTaskStatusNotification({
        sessionId: 'spec-session',
        tasksFilePath: '/w/tasks.md',
        changes: [
          {
            taskId: '1.1 First',
            executionStatus: 'running',
            lastExecutionId: 'exec-1',
          },
          {
            taskId: '1.2 Second',
            executionStatus: 'unknown',
            lastExecutionId: 'exec-1',
          },
          { taskId: '1.3 Missing identity', executionStatus: 'running' },
          null,
        ],
      })
    ).toEqual({
      sessionId: 'spec-session',
      tasksFilePath: '/w/tasks.md',
      changes: [
        {
          taskId: '1.1 First',
          executionStatus: 'running',
          lastExecutionId: 'exec-1',
        },
      ],
    });
  });

  it('rejects malformed containers and notifications with no valid changes', () => {
    for (const value of [
      null,
      {},
      { sessionId: 'spec-session', tasksFilePath: '/w/tasks.md', changes: {} },
      {
        sessionId: 'spec-session',
        tasksFilePath: '/w/tasks.md',
        changes: [{ taskId: '1.1 First', executionStatus: 'unknown' }],
      },
    ]) {
      expect(parseSpecTaskStatusNotification(value)).toBeNull();
    }
  });
});
