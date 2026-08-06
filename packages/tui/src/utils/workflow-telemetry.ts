import type {
  WorkflowEvent,
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowNodeType,
  WorkflowProgressEvent,
  WorkflowRestoreResult,
  WorkflowRunSnapshotEvent,
  WorkflowStateSnapshot,
} from '../types/workflow.js';
import { isLiveWorkflowStatus } from '../types/workflow-status.js';

export type WorkflowRunEvent =
  | 'started'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'aborted';
export type WorkflowOutcome = 'completed' | 'failed' | 'aborted' | '_other_';
export type WorkflowTopology =
  | 'sequential'
  | 'parallel'
  | 'iterative'
  | 'watch'
  | 'mixed'
  | '_other_';
export type WorkflowStepBucket =
  | '1'
  | '2'
  | '3_5'
  | '6_10'
  | '11_plus'
  | '_other_';
export type WorkflowControlAction =
  | 'pause'
  | 'resume'
  | 'cancel'
  | 'retry'
  | 'message';
export type WorkflowControlResult = 'success' | 'failed';
export type WorkflowRestoreMetricResult = WorkflowRestoreResult;

interface WorkflowDimensions {
  topology: WorkflowTopology;
  stepBucket: WorkflowStepBucket;
}

export type WorkflowTelemetryObservation =
  | ({
      type: 'run';
      event: WorkflowRunEvent;
    } & WorkflowDimensions)
  | ({
      type: 'run_duration';
      durationSeconds: number;
      outcome: WorkflowOutcome;
    } & WorkflowDimensions)
  | {
      type: 'node';
      nodeType: WorkflowNodeType;
      outcome: WorkflowNodeStatus;
    }
  | {
      type: 'node_duration';
      durationSeconds: number;
      nodeType: WorkflowNodeType;
      outcome: WorkflowNodeStatus;
    }
  | {
      type: 'concurrent';
      activeRuns: number;
    };

type WorkflowTreeNode = {
  nodeId: string;
  type: WorkflowNodeType;
  steps?: readonly WorkflowNodeDescriptor[];
  branches?: readonly WorkflowNodeDescriptor[];
  children?: readonly WorkflowNodeState[];
  maxIterations?: number;
};

interface NodeIdentity {
  nodeId: string;
  nodePath?: readonly string[];
  sessionId?: string;
  iteration?: number;
  branchId?: string;
}

interface NodeAttempt extends NodeIdentity {
  nodeType?: WorkflowNodeType;
  active: boolean;
  restoredTerminal: boolean;
  completionFingerprints: Set<string>;
}

interface RunState {
  status: 'running' | 'paused';
  dimensions: WorkflowDimensions;
  nodeTypes: Map<string, WorkflowNodeType>;
  nodeAttempts: NodeAttempt[];
  startObserved: boolean;
}

const UNKNOWN_DIMENSIONS: WorkflowDimensions = {
  topology: '_other_',
  stepBucket: '_other_',
};
const MAX_RECENT_TERMINALS = 128;
const MAX_NODE_TYPES_PER_RUN = 1024;
const MAX_NODE_ATTEMPTS_PER_RUN = 256;
const MAX_COMPLETION_FINGERPRINTS_PER_ATTEMPT = 8;

function children(node: WorkflowTreeNode): readonly WorkflowTreeNode[] {
  return node.steps ?? node.branches ?? node.children ?? [];
}

function analyzeTree(nodes: readonly WorkflowTreeNode[]): {
  leafSteps: number;
  constructs: Set<'parallel' | 'repeat' | 'watch'>;
} {
  let leafSteps = 0;
  const constructs = new Set<'parallel' | 'repeat' | 'watch'>();

  const visit = (node: WorkflowTreeNode, depth: number): void => {
    if (depth > 100) return;
    if (node.type === 'step') leafSteps += 1;
    if (node.type === 'watch') {
      leafSteps += 1;
      constructs.add('watch');
    } else if (node.type === 'parallel' || node.type === 'repeat') {
      constructs.add(node.type);
    }
    for (const child of children(node)) visit(child, depth + 1);
  };

  for (const node of nodes) visit(node, 0);
  return { leafSteps, constructs };
}

export function workflowStepBucket(
  declaredLeafSteps: number | undefined
): WorkflowStepBucket {
  if (
    declaredLeafSteps === undefined ||
    !Number.isInteger(declaredLeafSteps) ||
    declaredLeafSteps < 1
  ) {
    return '_other_';
  }
  if (declaredLeafSteps === 1) return '1';
  if (declaredLeafSteps === 2) return '2';
  if (declaredLeafSteps <= 5) return '3_5';
  if (declaredLeafSteps <= 10) return '6_10';
  return '11_plus';
}

export function classifyWorkflowTopology(
  nodes: readonly WorkflowTreeNode[]
): WorkflowTopology {
  const { leafSteps, constructs } = analyzeTree(nodes);
  if (leafSteps === 0) return '_other_';
  if (constructs.size === 0) return 'sequential';
  if (constructs.size > 1) return 'mixed';
  if (constructs.has('parallel')) return 'parallel';
  if (constructs.has('repeat')) return 'iterative';
  return 'watch';
}

export function workflowDimensions(
  nodes: readonly WorkflowTreeNode[]
): WorkflowDimensions {
  const { leafSteps } = analyzeTree(nodes);
  return {
    topology: classifyWorkflowTopology(nodes),
    stepBucket: workflowStepBucket(leafSteps),
  };
}

function indexNodeTypes(
  nodes: readonly WorkflowTreeNode[]
): Map<string, WorkflowNodeType> {
  const result = new Map<string, WorkflowNodeType>();
  const visit = (node: WorkflowTreeNode, depth: number): void => {
    if (depth > 100) return;
    rememberNodeType(result, node.nodeId, node.type);
    for (const child of children(node)) visit(child, depth + 1);
  };
  for (const node of nodes) visit(node, 0);
  return result;
}

function rememberNodeType(
  nodeTypes: Map<string, WorkflowNodeType>,
  nodeId: string,
  nodeType: WorkflowNodeType
): void {
  nodeTypes.delete(nodeId);
  nodeTypes.set(nodeId, nodeType);
  if (nodeTypes.size <= MAX_NODE_TYPES_PER_RUN) return;
  const oldest = nodeTypes.keys().next().value;
  if (oldest !== undefined) nodeTypes.delete(oldest);
}

function rememberNodeAttempt(
  attempts: NodeAttempt[],
  attempt: NodeAttempt
): void {
  const existingIndex = attempts.indexOf(attempt);
  if (existingIndex >= 0) attempts.splice(existingIndex, 1);
  attempts.push(attempt);
  if (attempts.length > MAX_NODE_ATTEMPTS_PER_RUN) attempts.shift();
}

function rememberCompletionFingerprint(
  fingerprints: Set<string>,
  fingerprint: string
): void {
  fingerprints.add(fingerprint);
  if (fingerprints.size <= MAX_COMPLETION_FINGERPRINTS_PER_ATTEMPT) return;
  const oldest = fingerprints.values().next().value;
  if (oldest !== undefined) fingerprints.delete(oldest);
}

function identityFromEvent(
  event: Extract<WorkflowEvent, { type: 'node_start' | 'node_complete' }>
): NodeIdentity {
  return {
    nodeId: event.nodeId,
    nodePath: event.nodePath,
    sessionId: event.sessionId,
    iteration: event.iteration,
    branchId: event.branchId,
  };
}

function identitiesMatch(left: NodeIdentity, right: NodeIdentity): boolean {
  if (left.nodeId !== right.nodeId) return false;
  if (
    left.sessionId !== undefined &&
    right.sessionId !== undefined &&
    left.sessionId !== right.sessionId
  ) {
    return false;
  }
  if (
    left.nodePath !== undefined &&
    right.nodePath !== undefined &&
    (left.nodePath.length !== right.nodePath.length ||
      left.nodePath.some(
        (segment, index) => segment !== right.nodePath?.[index]
      ))
  ) {
    return false;
  }
  if (
    left.iteration !== undefined &&
    right.iteration !== undefined &&
    left.iteration !== right.iteration
  ) {
    return false;
  }
  if (
    left.branchId !== undefined &&
    right.branchId !== undefined &&
    left.branchId !== right.branchId
  ) {
    return false;
  }
  return true;
}

function durationFromSnapshot(
  snapshot: WorkflowStateSnapshot
): number | undefined {
  if (!snapshot.root.startedAt || !snapshot.root.endedAt) return undefined;
  const startedAt = Date.parse(snapshot.root.startedAt);
  const endedAt = Date.parse(snapshot.root.endedAt);
  if (
    !Number.isFinite(startedAt) ||
    !Number.isFinite(endedAt) ||
    endedAt < startedAt
  ) {
    return undefined;
  }
  return (endedAt - startedAt) / 1000;
}

function terminalFingerprint(
  event: Extract<WorkflowEvent, { type: 'run_complete' }>
): string {
  return [
    event.workflowId,
    event.status,
    event.finalState.root.startedAt ?? '',
    event.finalState.root.endedAt ?? '',
  ].join(':');
}

function restoredAttempts(root: WorkflowNodeState): NodeAttempt[] {
  const attempts: NodeAttempt[] = [];
  const visit = (node: WorkflowNodeState): void => {
    rememberNodeAttempt(attempts, {
      nodeId: node.nodeId,
      sessionId: node.sessionId,
      iteration: node.iteration,
      branchId: node.branchId,
      nodeType: node.type,
      active: node.status === 'running' || node.status === 'paused',
      restoredTerminal:
        node.status === 'completed' ||
        node.status === 'failed' ||
        node.status === 'aborted' ||
        node.status === 'skipped',
      completionFingerprints: new Set(),
    });
    for (const child of node.children ?? []) visit(child);
  };
  visit(root);
  return attempts;
}

export class WorkflowTelemetryTracker {
  private readonly runs = new Map<string, RunState>();
  private readonly recentTerminals = new Set<string>();

  observe(
    event: WorkflowProgressEvent,
    live: boolean
  ): WorkflowTelemetryObservation[] {
    if (event.type === 'run_snapshot') {
      this.restoreSnapshot(event);
      return [];
    }
    return live ? this.observeLive(event) : [];
  }

  activeRunCount(): number {
    return this.runs.size;
  }

  reset(): void {
    this.runs.clear();
    this.recentTerminals.clear();
  }

  private restoreSnapshot(event: WorkflowRunSnapshotEvent): void {
    if (!isLiveWorkflowStatus(event.state.status)) {
      this.runs.delete(event.workflowId);
      return;
    }
    const tree: readonly WorkflowTreeNode[] =
      event.nodePlan && event.nodePlan.length > 0
        ? event.nodePlan
        : [event.state.root];
    this.runs.set(event.workflowId, {
      status: event.state.status,
      dimensions: workflowDimensions(tree),
      nodeTypes: indexNodeTypes(tree),
      nodeAttempts: restoredAttempts(event.state.root),
      startObserved: true,
    });
  }

  private observeLive(event: WorkflowEvent): WorkflowTelemetryObservation[] {
    switch (event.type) {
      case 'run_start':
        return this.observeRunStart(event);
      case 'paused':
        return this.observePaused(event.workflowId);
      case 'run_complete': {
        if (event.status === 'paused') {
          return this.observePaused(event.workflowId, event.finalState);
        }
        return this.observeTerminal(event);
      }
      case 'node_start':
        this.observeNodeStart(event);
        return [];
      case 'node_complete':
        return this.observeNodeComplete(event);
      default:
        return [];
    }
  }

  private observeRunStart(
    event: Extract<WorkflowEvent, { type: 'run_start' }>
  ): WorkflowTelemetryObservation[] {
    this.removeTerminalFingerprints(event.workflowId);
    const dimensions = workflowDimensions(event.nodeTree);
    const existing = this.runs.get(event.workflowId);
    if (existing) {
      existing.status = 'running';
      existing.dimensions = dimensions;
      existing.nodeTypes = indexNodeTypes(event.nodeTree);
      if (existing.startObserved) return [];
      existing.startObserved = true;
    } else {
      this.runs.set(event.workflowId, {
        status: 'running',
        dimensions,
        nodeTypes: indexNodeTypes(event.nodeTree),
        nodeAttempts: [],
        startObserved: true,
      });
    }
    return [
      { type: 'run', event: 'started', ...dimensions },
      this.concurrentObservation(),
    ];
  }

  private observePaused(
    workflowId: string,
    snapshot?: WorkflowStateSnapshot
  ): WorkflowTelemetryObservation[] {
    const run = this.ensureRun(workflowId, snapshot);
    if (run.status === 'paused') return [];
    run.status = 'paused';
    return [
      { type: 'run', event: 'paused', ...run.dimensions },
      this.concurrentObservation(),
    ];
  }

  private observeNodeStart(
    event: Extract<WorkflowEvent, { type: 'node_start' }>
  ): void {
    const run = this.ensureRun(event.workflowId);
    run.status = 'running';
    rememberNodeType(run.nodeTypes, event.nodeId, event.nodeType);
    const identity = identityFromEvent(event);
    const existing = [...run.nodeAttempts]
      .reverse()
      .find((attempt) => identitiesMatch(attempt, identity));
    if (existing?.active) return;
    if (existing) {
      Object.assign(existing, identity, {
        nodeType: event.nodeType,
        active: true,
        restoredTerminal: false,
      });
      existing.completionFingerprints.clear();
      rememberNodeAttempt(run.nodeAttempts, existing);
      return;
    }
    rememberNodeAttempt(run.nodeAttempts, {
      ...identity,
      nodeType: event.nodeType,
      active: true,
      restoredTerminal: false,
      completionFingerprints: new Set(),
    });
  }

  private observeNodeComplete(
    event: Extract<WorkflowEvent, { type: 'node_complete' }>
  ): WorkflowTelemetryObservation[] {
    const run = this.ensureRun(event.workflowId);
    const identity = identityFromEvent(event);
    let attempt = [...run.nodeAttempts]
      .reverse()
      .find((candidate) => identitiesMatch(candidate, identity));
    if (!attempt) {
      attempt = {
        ...identity,
        nodeType: run.nodeTypes.get(event.nodeId),
        active: false,
        restoredTerminal: false,
        completionFingerprints: new Set(),
      };
      rememberNodeAttempt(run.nodeAttempts, attempt);
    }
    if (attempt.restoredTerminal) return [];

    const duration =
      event.durationSecs !== undefined &&
      Number.isFinite(event.durationSecs) &&
      event.durationSecs >= 0
        ? event.durationSecs
        : undefined;
    const fingerprint = `${event.status}:${duration ?? 'missing'}`;
    if (attempt.completionFingerprints.has(fingerprint)) return [];
    rememberCompletionFingerprint(attempt.completionFingerprints, fingerprint);
    attempt.active = false;
    rememberNodeAttempt(run.nodeAttempts, attempt);

    const nodeType = attempt.nodeType ?? run.nodeTypes.get(event.nodeId);
    if (!nodeType) return [];
    const observations: WorkflowTelemetryObservation[] = [
      { type: 'node', nodeType, outcome: event.status },
    ];
    if (duration !== undefined) {
      observations.push({
        type: 'node_duration',
        durationSeconds: duration,
        nodeType,
        outcome: event.status,
      });
    }
    return observations;
  }

  private observeTerminal(
    event: Extract<WorkflowEvent, { type: 'run_complete' }>
  ): WorkflowTelemetryObservation[] {
    const status =
      event.status === 'completed' ||
      event.status === 'failed' ||
      event.status === 'aborted'
        ? event.status
        : undefined;
    if (!status) return [];
    const fingerprint = terminalFingerprint(event);
    if (this.recentTerminals.has(fingerprint)) return [];

    const snapshot = event.finalState;
    const run = this.ensureRun(event.workflowId, snapshot);
    const observations: WorkflowTelemetryObservation[] = [
      { type: 'run', event: status, ...run.dimensions },
    ];
    const duration = snapshot ? durationFromSnapshot(snapshot) : undefined;
    if (duration !== undefined) {
      observations.push({
        type: 'run_duration',
        durationSeconds: duration,
        outcome: status,
        ...run.dimensions,
      });
    }

    this.runs.delete(event.workflowId);
    this.rememberTerminal(fingerprint);
    observations.push(this.concurrentObservation());
    return observations;
  }

  private ensureRun(
    workflowId: string,
    snapshot?: WorkflowStateSnapshot
  ): RunState {
    const existing = this.runs.get(workflowId);
    if (existing) {
      if (
        snapshot &&
        existing.dimensions.topology === '_other_' &&
        existing.dimensions.stepBucket === '_other_'
      ) {
        const tree: readonly WorkflowTreeNode[] = [snapshot.root];
        existing.dimensions = workflowDimensions(tree);
        existing.nodeTypes = indexNodeTypes(tree);
      }
      return existing;
    }
    const tree: readonly WorkflowTreeNode[] = snapshot ? [snapshot.root] : [];
    const created: RunState = {
      status:
        snapshot && isLiveWorkflowStatus(snapshot.status)
          ? snapshot.status
          : 'running',
      dimensions: snapshot ? workflowDimensions(tree) : UNKNOWN_DIMENSIONS,
      nodeTypes: snapshot ? indexNodeTypes(tree) : new Map(),
      nodeAttempts: snapshot ? restoredAttempts(snapshot.root) : [],
      startObserved: false,
    };
    this.runs.set(workflowId, created);
    return created;
  }

  private concurrentObservation(): WorkflowTelemetryObservation {
    return { type: 'concurrent', activeRuns: this.runs.size };
  }

  private rememberTerminal(fingerprint: string): void {
    this.recentTerminals.add(fingerprint);
    if (this.recentTerminals.size <= MAX_RECENT_TERMINALS) return;
    const oldest = this.recentTerminals.values().next().value;
    if (oldest !== undefined) this.recentTerminals.delete(oldest);
  }

  private removeTerminalFingerprints(workflowId: string): void {
    const prefix = `${workflowId}:`;
    for (const fingerprint of this.recentTerminals) {
      if (fingerprint.startsWith(prefix)) {
        this.recentTerminals.delete(fingerprint);
      }
    }
  }
}
