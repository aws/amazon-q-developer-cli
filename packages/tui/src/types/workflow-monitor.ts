import type {
  WorkflowCompletionSignal,
  WorkflowCompletionSignalSource,
  WorkflowNodeSessionTarget,
  WorkflowNodeStatus,
  WorkflowNodeType,
  WorkflowStatus,
  WorkflowStepSessionRef,
  WorkflowWatchOutcome,
} from './workflow.js';

export type WorkflowSurface = 'monitor' | 'tray';
export type WorkflowMonitorLayout = 'side-by-side' | 'stacked';

/** Flat, render-ready node derived from the recursive workflow plan/state. */
export interface WorkflowMonitorNode {
  id: string;
  type: WorkflowNodeType;
  status: WorkflowNodeStatus;
  label: string;
  parentId: string | null;
  depth: number;
  nodePath?: readonly string[];
  sessionId?: string;
  agentName?: string;
  modelId?: string;
  effortLevel?: string;
  iteration?: number;
  branchId?: string;
  maxIterations?: number;
  watchOutcome?: WorkflowWatchOutcome;
  durationSecs?: number;
  failureReason?: string;
  capturedOutput?: string;
  completionSignal?: WorkflowCompletionSignal;
  completionSignalSource?: WorkflowCompletionSignalSource;
  pauseReason?: string;
}

/** A durable child-session reference enriched with its latest lifecycle state. */
export interface WorkflowMonitorSession extends WorkflowStepSessionRef {
  status?: WorkflowNodeStatus;
  agentName?: string;
}

export interface WorkflowRunView {
  workflowId: string;
  parentSessionId?: string;
  name: string;
  status: WorkflowStatus;
  nodes: WorkflowMonitorNode[];
  queuedNodeIds?: readonly string[];
  stepSessions: WorkflowMonitorSession[];
  startedAt: number | null;
  completedAt: number | null;
  pauseReason?: string;
  /** Set when the user deliberately stopped this run, with their optional reason. */
  stopInitiator?: 'user';
  stopReason?: string;
}

export interface WorkflowNodeConversation {
  target: WorkflowNodeSessionTarget;
  label: string;
  agentName?: string;
  nodeStatus: WorkflowNodeStatus;
}

export interface WorkflowCollectionState {
  workflows: Map<string, WorkflowRunView>;
  archivedWorkflows: Map<string, WorkflowRunView>;
  activeWorkflowId: string | null;
  selectedNodeIndices: Map<string, number>;
  openWorkflowSurfaces: ReadonlySet<WorkflowSurface>;
  pauseRequestedWorkflowIds: ReadonlySet<string>;
  selectionLocked: boolean;
}
