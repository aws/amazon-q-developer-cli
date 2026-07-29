import type {
  JoinPolicy,
  NodeStatus,
  NodeType,
  OnMaxIterations,
  StopCondition,
  WatchOutcome,
  WorkflowStepsQueuedResolution,
  WorkflowStatus as CovenantWorkflowStatus,
} from '@kiro/acp-type-covenant';

export type WorkflowStatus = CovenantWorkflowStatus;
export type WorkflowRunCompleteStatus = Exclude<WorkflowStatus, 'running'>;
export type WorkflowNodeStatus = NodeStatus;
export type WorkflowNodeType = NodeType;
export type WorkflowCompletionSignal = NonNullable<
  StopCondition['completionSignal']
>;
export type WorkflowJoinPolicy = JoinPolicy;
export type WorkflowMaxIterationPolicy = OnMaxIterations;
export type WorkflowWatchOutcome = WatchOutcome;

interface WorkflowStopConditionFields {
  containsText?: string;
  fileCheck?: {
    path: string;
    jsonPath: string;
    value: unknown;
  };
  completionSignal?: WorkflowCompletionSignal;
}

export type WorkflowStopCondition = WorkflowStopConditionFields &
  (
    | { containsText: string }
    | { fileCheck: NonNullable<WorkflowStopConditionFields['fileCheck']> }
    | { completionSignal: WorkflowCompletionSignal }
  );

/** Static plan node sent by run_start, inspect, and list-recipe calls. */
export interface WorkflowNodeDescriptor {
  nodeId: string;
  type: WorkflowNodeType;
  agentName?: string;
  modelId?: string;
  effortLevel?: string;
  steps?: WorkflowNodeDescriptor[];
  branches?: WorkflowNodeDescriptor[];
  joinPolicy?: WorkflowJoinPolicy;
  maxIterations?: number;
  stopCondition?: WorkflowStopCondition;
  stopWhen?: string;
  onMaxIterations?: WorkflowMaxIterationPolicy;
}

/** Durable runtime state for one node. */
export interface WorkflowNodeState {
  nodeId: string;
  type: WorkflowNodeType;
  status: WorkflowNodeStatus;
  agentName?: string;
  modelId?: string;
  effortLevel?: string;
  sessionId?: string;
  startedAt?: string;
  endedAt?: string;
  iteration?: number;
  branchId?: string;
  children?: WorkflowNodeState[];
  artifacts?: Record<string, string>;
  capturedOutput?: string;
  watchCursor?: unknown;
  watchTerminal?: boolean;
  failureReason?: string;
  completionSignal?: WorkflowCompletionSignal;
  continuationAttempts?: number;
}

/** Snapshot persisted by KAS and returned on terminal workflow events. */
export interface WorkflowStateSnapshot {
  workflowId: string;
  workflowName: string;
  status: WorkflowStatus;
  inputs: Record<string, string>;
  artifacts: Record<string, string>;
  capturedOutputs: Record<string, string>;
  root: WorkflowNodeState;
  pauseReason?: string;
  parentSessionId?: string;
  workspacePath?: string;
  additionalDirectories?: string[];
  createdAt?: string;
  parentModelId?: string;
  modelId?: string;
  parentEffortLevel?: string;
  effortLevel?: string;
  planRevision?: number;
}

/** Durable identity for one workflow-created ACP session. */
export interface WorkflowStepSessionRef {
  nodeId: string;
  nodePath: readonly string[];
  sessionId: string;
  iteration?: number;
  branchId?: string;
}

/** User-facing operations require the full ownership chain, not a bare ID. */
export interface WorkflowNodeSessionTarget extends WorkflowStepSessionRef {
  workflowId: string;
  parentSessionId: string;
}

/**
 * Optional engine capability for continuing workflow-created conversations
 * without changing the primary chat session.
 */
export interface WorkflowConversationApi {
  sendMessage(
    target: WorkflowNodeSessionTarget,
    content: string
  ): Promise<void>;
}

/** Authoritative response from `_kiro/workflow/load`. */
export interface WorkflowLoadResponse {
  workflowId: string;
  state: WorkflowStateSnapshot;
  stepSessions: WorkflowStepSessionRef[];
  nodePlan?: WorkflowNodeDescriptor[];
}

interface WorkflowEventBase {
  workflowId: string;
  /** Parent chat that owns this run. Present on current KAS lifecycle events. */
  parentSessionId?: string;
}

type WorkflowStepsQueuedEvent = WorkflowEventBase & {
  type: 'steps_queued';
} & (
    | {
        pendingSteps: [WorkflowNodeDescriptor, ...WorkflowNodeDescriptor[]];
        resolution?: never;
      }
    | {
        pendingSteps: [];
        resolution: WorkflowStepsQueuedResolution;
      }
  );

export type WorkflowEvent =
  | (WorkflowEventBase & {
      type: 'run_start';
      workflowName: string;
      inputs: Record<string, string>;
      nodeTree: WorkflowNodeDescriptor[];
    })
  | (WorkflowEventBase & {
      type: 'node_start';
      nodeId: string;
      nodePath: readonly string[];
      /** The wire payload calls this field `type`; normalized to avoid a clash. */
      nodeType: WorkflowNodeType;
      agentName?: string;
      prompt?: string;
      sessionId?: string;
      iteration?: number;
      branchId?: string;
    })
  | (WorkflowEventBase & {
      type: 'node_complete';
      nodeId: string;
      nodePath: readonly string[];
      status: WorkflowNodeStatus;
      sessionId?: string;
      iteration?: number;
      branchId?: string;
      artifacts?: Record<string, string>;
      capturedOutput?: string;
      durationSecs?: number;
      failureReason?: string;
    })
  | (WorkflowEventBase & {
      type: 'node_paused';
      nodeId: string;
      nodePath: readonly string[];
      sessionId?: string;
      iteration?: number;
      branchId?: string;
      reason: string;
    })
  | (WorkflowEventBase & {
      type: 'loop_iteration';
      loopId: string;
      iteration: number;
      stopConditionMet: boolean;
    })
  | (WorkflowEventBase & {
      type: 'watch_poll';
      nodeId: string;
      nodePath: readonly string[];
      outcome: WorkflowWatchOutcome;
      at: string;
    })
  | (WorkflowEventBase & {
      type: 'paused';
      pauseReason: string;
    })
  | (WorkflowEventBase & {
      type: 'run_complete';
      status: WorkflowRunCompleteStatus;
      finalState: WorkflowStateSnapshot;
    })
  | WorkflowStepsQueuedEvent;

/**
 * Local restore event built from `_kiro/workflow/load`.
 *
 * This is never decoded from a notification. It lets the workflow store
 * atomically restore a live run before its child sessions resume streaming.
 */
export interface WorkflowRunSnapshotEvent {
  type: 'run_snapshot';
  workflowId: string;
  parentSessionId: string;
  state: WorkflowStateSnapshot;
  stepSessions: WorkflowStepSessionRef[];
  nodePlan?: WorkflowNodeDescriptor[];
}

export type WorkflowProgressEvent = WorkflowEvent | WorkflowRunSnapshotEvent;
