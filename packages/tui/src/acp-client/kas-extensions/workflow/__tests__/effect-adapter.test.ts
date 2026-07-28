import { describe, expect, it } from 'bun:test';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type AgentStreamEvent,
  type ApprovalRequestEvent,
} from '../../../../types/agent-events.js';
import {
  SessionLifecycleOwner,
  type SessionEvent,
} from '../../../../types/multi-session.js';
import type { WorkflowExtensionEffect } from '../effects.js';
import { createWorkflowEffectSink } from '../effect-adapter.js';

describe('workflow effect adapter', () => {
  it('maps every domain effect to one typed TUI port', () => {
    const main: AgentStreamEvent[] = [];
    const sessions: SessionEvent[] = [];
    const children: Array<{
      sessionId: string;
      event: AgentStreamEvent;
    }> = [];
    const approvals: ApprovalRequestEvent[] = [];
    const now = new Date('2026-07-19T12:00:00.000Z');
    const emit = createWorkflowEffectSink({
      emitMain: (event) => main.push(event),
      emitSession: (event) => sessions.push(event),
      emitChild: (sessionId, event) => children.push({ sessionId, event }),
      emitApproval: (event) => approvals.push(event),
      createId: () => 'workflow-progress-id',
      now: () => now,
    });
    const owner = {
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      nodeId: 'build',
      nodePath: ['workflow-1', 'build'],
      sessionId: 'child-1',
      status: 'completed',
      agentName: 'builder',
    } as const;
    const childEvent: AgentStreamEvent = {
      type: AgentEventType.Content,
      id: 'content-1',
      content: { type: ContentType.Text, text: 'done' },
    };
    const approvalEvent: ApprovalRequestEvent = {
      type: AgentEventType.ApprovalRequest,
      value: {
        sessionId: owner.sessionId,
        originSessionId: owner.sessionId,
        toolCall: { toolCallId: 'tool-1' },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow once',
            optionId: 'allow-once',
          },
        ],
        resolve: () => {},
      },
    };
    const effects: WorkflowExtensionEffect[] = [
      {
        type: 'workflow_progress',
        event: {
          type: 'run_start',
          workflowId: owner.workflowId,
          parentSessionId: owner.parentSessionId,
          workflowName: 'Workflow 1',
          inputs: {},
          nodeTree: [],
        },
      },
      { type: 'child_registered', owner },
      { type: 'child_status_restored', owner },
      { type: 'child_busy', sessionId: owner.sessionId },
      { type: 'child_conversation_reset', sessionId: owner.sessionId },
      { type: 'child_turn_started', sessionId: owner.sessionId },
      { type: 'child_event', sessionId: owner.sessionId, event: childEvent },
      {
        type: 'child_approval_requested',
        sessionId: owner.sessionId,
        event: approvalEvent,
      },
      { type: 'child_approvals_cancelled', sessionId: owner.sessionId },
      { type: 'child_removed', sessionId: owner.sessionId },
    ];

    for (const effect of effects) emit(effect);

    expect(main).toEqual([
      {
        type: AgentEventType.WorkflowProgress,
        id: 'workflow-progress-id',
        event: {
          type: 'run_start',
          workflowId: owner.workflowId,
          parentSessionId: owner.parentSessionId,
          workflowName: 'Workflow 1',
          inputs: {},
          nodeTree: [],
        },
      },
    ]);
    expect(sessions).toEqual([
      {
        type: 'session_created',
        session: {
          id: owner.sessionId,
          name: owner.agentName,
          agentName: owner.agentName,
          status: 'idle',
          type: 'ephemeral',
          group: 'workflow',
          parentSession: owner.parentSessionId,
          lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
          created: now,
          lastActivity: now,
        },
      },
      {
        type: 'session_status_changed',
        sessionId: owner.sessionId,
        status: 'idle',
      },
      {
        type: 'session_status_changed',
        sessionId: owner.sessionId,
        status: 'busy',
      },
      {
        type: 'session_conversation_reset',
        sessionId: owner.sessionId,
      },
      { type: 'session_turn_started', sessionId: owner.sessionId },
      {
        type: 'session_approvals_cancelled',
        sessionId: owner.sessionId,
      },
      { type: 'session_removed', sessionId: owner.sessionId },
    ]);
    expect(children).toEqual([
      { sessionId: owner.sessionId, event: childEvent },
      { sessionId: owner.sessionId, event: approvalEvent },
    ]);
    expect(approvals).toEqual([approvalEvent]);
  });
});
