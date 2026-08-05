import type {
  WorkflowNodeStatus,
  WorkflowRunCompleteStatus,
  WorkflowStatus,
} from './workflow.js';

export type LiveWorkflowStatus = Extract<WorkflowStatus, 'running' | 'paused'>;
export type RetryableWorkflowStatus = Extract<
  WorkflowStatus,
  'failed' | 'aborted'
>;
export type TerminalWorkflowStatus = Exclude<
  WorkflowRunCompleteStatus,
  'paused'
>;
export type ActiveWorkflowNodeStatus = Extract<
  WorkflowNodeStatus,
  'running' | 'paused'
>;
export type TerminalWorkflowNodeStatus = Extract<
  WorkflowNodeStatus,
  'completed' | 'failed' | 'aborted' | 'skipped'
>;

const WORKFLOW_STATUS_MEMBERS = {
  running: true,
  paused: true,
  completed: true,
  failed: true,
  aborted: true,
} as const satisfies Record<WorkflowStatus, true>;

const WORKFLOW_NODE_STATUS_MEMBERS = {
  pending: true,
  running: true,
  paused: true,
  completed: true,
  failed: true,
  aborted: true,
  skipped: true,
} as const satisfies Record<WorkflowNodeStatus, true>;

const WORKFLOW_STATUSES: ReadonlySet<WorkflowStatus> = new Set(
  Object.keys(WORKFLOW_STATUS_MEMBERS) as WorkflowStatus[]
);

const LIVE_WORKFLOW_STATUSES: ReadonlySet<LiveWorkflowStatus> = new Set([
  'running',
  'paused',
]);

const RETRYABLE_WORKFLOW_STATUSES: ReadonlySet<RetryableWorkflowStatus> =
  new Set(['failed', 'aborted']);

const RUN_COMPLETE_WORKFLOW_STATUSES: ReadonlySet<WorkflowRunCompleteStatus> =
  new Set(['paused', 'completed', 'failed', 'aborted']);

const TERMINAL_WORKFLOW_STATUSES: ReadonlySet<TerminalWorkflowStatus> = new Set(
  ['completed', 'failed', 'aborted']
);

const WORKFLOW_NODE_STATUSES: ReadonlySet<WorkflowNodeStatus> = new Set(
  Object.keys(WORKFLOW_NODE_STATUS_MEMBERS) as WorkflowNodeStatus[]
);

const ACTIVE_WORKFLOW_NODE_STATUSES: ReadonlySet<ActiveWorkflowNodeStatus> =
  new Set(['running', 'paused']);

const TERMINAL_WORKFLOW_NODE_STATUSES: ReadonlySet<TerminalWorkflowNodeStatus> =
  new Set(['completed', 'failed', 'aborted', 'skipped']);

function contains<T extends string>(
  members: ReadonlySet<T>,
  value: unknown
): value is T {
  return typeof value === 'string' && members.has(value as T);
}

export function isWorkflowStatus(status: unknown): status is WorkflowStatus {
  return contains(WORKFLOW_STATUSES, status);
}

export function isRunCompleteWorkflowStatus(
  status: unknown
): status is WorkflowRunCompleteStatus {
  return contains(RUN_COMPLETE_WORKFLOW_STATUSES, status);
}

export function isWorkflowNodeStatus(
  status: unknown
): status is WorkflowNodeStatus {
  return contains(WORKFLOW_NODE_STATUSES, status);
}

export function isLiveWorkflowStatus(
  status: WorkflowStatus
): status is LiveWorkflowStatus {
  return contains(LIVE_WORKFLOW_STATUSES, status);
}

export function isRetryableWorkflowStatus(
  status: WorkflowStatus | undefined
): status is RetryableWorkflowStatus {
  return contains(RETRYABLE_WORKFLOW_STATUSES, status);
}

export function isTerminalWorkflowStatus(
  status: WorkflowStatus | undefined
): status is TerminalWorkflowStatus {
  return contains(TERMINAL_WORKFLOW_STATUSES, status);
}

export function isActiveWorkflowNodeStatus(
  status: WorkflowNodeStatus | undefined
): status is ActiveWorkflowNodeStatus {
  return contains(ACTIVE_WORKFLOW_NODE_STATUSES, status);
}

export function isTerminalWorkflowNodeStatus(
  status: WorkflowNodeStatus | undefined
): status is TerminalWorkflowNodeStatus {
  return contains(TERMINAL_WORKFLOW_NODE_STATUSES, status);
}
