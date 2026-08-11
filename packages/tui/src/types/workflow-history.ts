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

/** Longer reasons are rejected at the boundary, so callers truncate instead. */
export const WORKFLOW_ACTION_REASON_MAX_LENGTH = 500;

/**
 * Who asked for a pause/resume/cancel, and why. Without `initiator: 'user'` KAS
 * wakes the parent session with a misleading "the workflow aborted" nudge for a
 * stop the user just made on purpose. Build these through
 * {@link workflowUserAction}, which enforces the two boundary rules.
 */
export interface WorkflowActionAttribution {
  initiator?: 'user';
  reason?: string;
}

/**
 * Attribution for a deliberate user action. Truncates `reason` and drops it when
 * empty, so `user`-only and no-bare-`reason` cannot be violated by a caller.
 */
export function workflowUserAction(reason?: string): WorkflowActionAttribution {
  const trimmed = reason?.trim();
  return {
    initiator: 'user',
    ...(trimmed
      ? { reason: trimmed.slice(0, WORKFLOW_ACTION_REASON_MAX_LENGTH) }
      : {}),
  };
}

export interface WorkflowPauseResponse {
  paused: boolean;
}

export interface WorkflowResumeResponse {
  workflowId: string;
  status: WorkflowStatus;
}

export interface WorkflowRetryResponse {
  workflowId: string;
  status: WorkflowStatus;
  retriedNodeIds: string[];
}

export interface WorkflowCancelResponse {
  ok: boolean;
  previousStatus: WorkflowStatus;
}

/** Typed control plane for persisted and live workflow runs. */
export interface WorkflowControlApi extends WorkflowLaunchApi {
  listRuns(workspacePaths: readonly string[]): Promise<WorkflowRunSummary[]>;
  inspectRun(workflowId: string): Promise<WorkflowInspectResponse>;
  pauseRun(
    workflowId: string,
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowPauseResponse>;
  resumeRun(
    workflowId: string,
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowResumeResponse>;
  retryRun(workflowId: string, nodeId?: string): Promise<WorkflowRetryResponse>;
  cancelRun(
    workflowId: string,
    targetStatus?: 'aborted' | 'completed',
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowCancelResponse>;
}
