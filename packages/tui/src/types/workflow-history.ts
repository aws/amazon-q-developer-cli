import type {
  WorkflowNodeDescriptor,
  WorkflowStateSnapshot,
  WorkflowStatus,
} from './workflow.js';

/** One workflow run returned by `_kiro/workflow/list`. */
export interface WorkflowRunSummary {
  workflowId: string;
  name: string;
  status: WorkflowStatus;
  createdAt: string;
  updatedAt: string;
  startedAt?: string;
  endedAt?: string;
  parentSessionId?: string;
}

export interface WorkflowListResponse {
  runs: WorkflowRunSummary[];
}

/** Full persisted run returned by `_kiro/workflow/inspect`. */
export interface WorkflowInspectResponse {
  workflowId: string;
  state: WorkflowStateSnapshot;
  nodePlan?: WorkflowNodeDescriptor[];
}
