/**
 * Local workflow contracts for the KAS `_kiro/workflow/*` extensions.
 *
 * The released `@kiro/acp-type-covenant` consumed by the TUI does not export
 * these types yet. Keep this module wire-compatible with the covenant and
 * remove it once the package export is available.
 */

export type WorkflowStatus =
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'aborted';

export type WorkflowNodeStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'aborted'
  | 'skipped';

export type WorkflowNodeType =
  | 'step'
  | 'sequence'
  | 'repeat'
  | 'parallel'
  | 'watch';

export type WorkflowCompletionSignal = 'success' | 'need_input' | 'error';

export interface WorkflowStopCondition {
  containsText?: string;
  fileCheck?: {
    path: string;
    jsonPath: string;
    value: unknown;
  };
  completionSignal?: WorkflowCompletionSignal;
}

/** Static plan node sent by run_start, inspect, and list-recipe calls. */
export interface WorkflowNodeDescriptor {
  nodeId: string;
  type: WorkflowNodeType;
  agentName?: string;
  modelId?: string;
  effortLevel?: string;
  steps?: WorkflowNodeDescriptor[];
  branches?: WorkflowNodeDescriptor[];
  maxIterations?: number;
  stopCondition?: WorkflowStopCondition;
  stopWhen?: string;
  onMaxIterations?: 'abort' | 'continue' | 'pause';
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
  inputs?: Record<string, string>;
  artifacts?: Record<string, string>;
  capturedOutputs?: Record<string, string>;
  root?: WorkflowNodeState;
  pauseReason?: string;
  parentSessionId?: string;
  workspacePath?: string;
  additionalDirectories?: string[];
  createdAt?: string;
}

interface WorkflowEventBase {
  workflowId: string;
  /** Parent chat that owns this run. Present on current KAS lifecycle events. */
  parentSessionId?: string;
}

export type WorkflowEvent =
  | (WorkflowEventBase & {
      type: 'run_start';
      workflowName?: string;
      inputs?: Record<string, string>;
      nodeTree?: WorkflowNodeDescriptor[];
    })
  | (WorkflowEventBase & {
      type: 'node_start';
      nodeId: string;
      nodePath?: readonly string[];
      /** The wire payload calls this field `type`; normalized to avoid a clash. */
      nodeType?: WorkflowNodeType;
      agentName?: string;
      prompt?: string;
      sessionId?: string;
      iteration?: number;
      branchId?: string;
    })
  | (WorkflowEventBase & {
      type: 'node_complete';
      nodeId: string;
      nodePath?: readonly string[];
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
      nodePath?: readonly string[];
      sessionId?: string;
      iteration?: number;
      branchId?: string;
      reason: string;
    })
  | (WorkflowEventBase & {
      /** Compatibility event emitted by older workflow runtimes. */
      type: 'need_input';
      nodeId: string;
      nodePath?: readonly string[];
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
      nodePath?: readonly string[];
      outcome: string;
      at?: string;
    })
  | (WorkflowEventBase & {
      type: 'paused';
      pauseReason: string;
    })
  | (WorkflowEventBase & {
      type: 'run_complete';
      status: WorkflowStatus;
      finalState?: WorkflowStateSnapshot;
    })
  | (WorkflowEventBase & {
      type: 'steps_queued';
      pendingSteps?: WorkflowNodeDescriptor[];
    });
