import {
  AgentEventType,
  type AgentStreamEvent,
  type ApprovalRequestEvent,
} from '../../../types/agent-events.js';
import {
  SessionLifecycleOwner,
  type SessionEvent,
  type SessionStatus,
} from '../../../types/multi-session.js';
import type { WorkflowNodeStatus } from '../../../types/workflow.js';
import type { WorkflowExtensionEffect } from './effects.js';

export interface WorkflowEffectPorts {
  emitMain(event: AgentStreamEvent): void;
  emitSession(event: SessionEvent): void;
  emitChild(sessionId: string, event: AgentStreamEvent): void;
  emitApproval(event: ApprovalRequestEvent): void;
  createId(): string;
  now(): Date;
}

function sessionStatus(status: WorkflowNodeStatus): SessionStatus {
  if (status === 'running') return 'busy';
  if (status === 'pending') return 'pending';
  if (status === 'failed' || status === 'aborted') return 'failed';
  return 'idle';
}

/** Maps workflow-domain effects onto the TUI's existing event ports. */
export function createWorkflowEffectSink(
  ports: WorkflowEffectPorts
): (effect: WorkflowExtensionEffect) => void {
  return (effect) => {
    switch (effect.type) {
      case 'workflow_progress':
        ports.emitMain({
          type: AgentEventType.WorkflowProgress,
          id: ports.createId(),
          event: effect.event,
        });
        return;
      case 'child_registered': {
        const { owner } = effect;
        const name =
          owner.agentName || owner.nodeId || owner.sessionId.slice(0, 8);
        const now = ports.now();
        ports.emitSession({
          type: 'session_created',
          session: {
            id: owner.sessionId,
            name,
            agentName: owner.agentName ?? name,
            status: sessionStatus(owner.status),
            type: 'ephemeral',
            group: 'workflow',
            parentSession: owner.parentSessionId,
            lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
            created: now,
            lastActivity: now,
          },
        });
        return;
      }
      case 'child_removed':
        ports.emitSession({
          type: 'session_removed',
          sessionId: effect.sessionId,
        });
        return;
      case 'child_status_restored':
        ports.emitSession({
          type: 'session_status_changed',
          sessionId: effect.owner.sessionId,
          status: sessionStatus(effect.owner.status),
        });
        return;
      case 'child_busy':
        ports.emitSession({
          type: 'session_status_changed',
          sessionId: effect.sessionId,
          status: 'busy',
        });
        return;
      case 'child_conversation_reset':
        ports.emitSession({
          type: 'session_conversation_reset',
          sessionId: effect.sessionId,
        });
        return;
      case 'child_turn_started':
        ports.emitSession({
          type: 'session_turn_started',
          sessionId: effect.sessionId,
        });
        return;
      case 'child_event':
        ports.emitChild(effect.sessionId, effect.event);
        return;
      case 'child_approval_requested':
        ports.emitChild(effect.sessionId, effect.event);
        ports.emitApproval(effect.event);
        return;
      case 'child_approvals_cancelled':
        ports.emitSession({
          type: 'session_approvals_cancelled',
          sessionId: effect.sessionId,
        });
        return;
      default: {
        const exhaustive: never = effect;
        return exhaustive;
      }
    }
  };
}
