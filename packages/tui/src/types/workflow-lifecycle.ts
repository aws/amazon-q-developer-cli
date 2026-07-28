import type { WorkflowStatus } from './workflow.js';
import type { MessageRole } from './message-role.js';

export type TerminalWorkflowStatus = Extract<
  WorkflowStatus,
  'completed' | 'failed' | 'aborted'
>;

export type WorkflowLifecycleStatus = 'started' | TerminalWorkflowStatus;

export interface WorkflowLifecycleNotice {
  workflowId: string;
  workflowName: string;
  status: WorkflowLifecycleStatus;
  workflowTurnId?: string;
}

export interface WorkflowLifecycleMessage {
  id: string;
  role: MessageRole.System;
  content: string;
  success: boolean;
  kind: 'workflow-lifecycle' | 'workflow-completion';
  workflowId: string;
  workflowName: string;
  workflowStatus: WorkflowLifecycleStatus;
  workflowTurnId?: string;
}
