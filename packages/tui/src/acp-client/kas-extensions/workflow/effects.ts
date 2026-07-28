import type {
  AgentStreamEvent,
  ApprovalRequestEvent,
} from '../../../types/agent-events.js';
import type { WorkflowProgressEvent } from '../../../types/workflow.js';
import type { WorkflowSessionOwner } from './owner-registry.js';

/**
 * Typed output of the workflow extension.
 *
 * The extension owns KAS workflow semantics but does not know how the TUI
 * stores sessions or renders stream events. The KAS client adapter translates
 * these effects at the integration boundary.
 */
export type WorkflowExtensionEffect =
  | {
      type: 'workflow_progress';
      event: WorkflowProgressEvent;
    }
  | {
      type: 'child_registered';
      owner: WorkflowSessionOwner;
    }
  | {
      type: 'child_removed';
      sessionId: string;
    }
  | {
      type: 'child_status_restored';
      owner: WorkflowSessionOwner;
    }
  | {
      type: 'child_busy';
      sessionId: string;
    }
  | {
      type: 'child_conversation_reset' | 'child_turn_started';
      sessionId: string;
    }
  | {
      type: 'child_event';
      sessionId: string;
      event: AgentStreamEvent;
    }
  | {
      type: 'child_approval_requested';
      sessionId: string;
      event: ApprovalRequestEvent;
    }
  | {
      type: 'child_approvals_cancelled';
      sessionId: string;
    };
