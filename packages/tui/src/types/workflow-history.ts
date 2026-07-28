import type {
  WorkflowNodeDescriptor,
  WorkflowStateSnapshot,
  WorkflowStatus,
} from './workflow.js';
import type { WorkflowLaunchApi } from './workflow-launch.js';

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

export interface WorkflowHistoryViewState {
  isOpen: boolean;
  runs: readonly WorkflowRunSummary[];
}

export interface WorkflowListResponse {
  runs: WorkflowRunSummary[];
}

/** Full persisted run returned by `_kiro/workflow/inspect`. */
export interface WorkflowInspectResponse {
  workflowId: string;
  state: WorkflowStateSnapshot;
  pendingSteps?: WorkflowNodeDescriptor[];
  nodePlan?: WorkflowNodeDescriptor[];
}

export interface WorkflowPauseResponse {
  paused: boolean;
}

export interface WorkflowResumeResponse {
  workflowId: string;
  status: WorkflowStatus;
}

export interface WorkflowCancelResponse {
  ok: boolean;
  previousStatus: WorkflowStatus;
}

/** Typed control plane for persisted and live workflow runs. */
export interface WorkflowControlApi extends WorkflowLaunchApi {
  listRuns(workspacePaths: readonly string[]): Promise<WorkflowRunSummary[]>;
  inspectRun(workflowId: string): Promise<WorkflowInspectResponse>;
  pauseRun(workflowId: string): Promise<WorkflowPauseResponse>;
  resumeRun(workflowId: string): Promise<WorkflowResumeResponse>;
  cancelRun(
    workflowId: string,
    targetStatus?: 'aborted' | 'completed'
  ): Promise<WorkflowCancelResponse>;
}
