import type {
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowNodeType,
  WorkflowProgressEvent,
  WorkflowStateSnapshot,
  WorkflowStatus,
} from '../types/workflow.js';
import type {
  WorkflowCollectionState,
  WorkflowMonitorNode,
  WorkflowMonitorSession,
  WorkflowRunView,
} from '../types/workflow-monitor.js';
import {
  isActiveWorkflowNodeStatus,
  isTerminalWorkflowStatus,
} from '../types/workflow-status.js';
import {
  workflowNodePathsEqual,
  workflowNodePathWithoutIterations,
  workflowStateEntries,
} from '../utils/workflow-node-path.js';

interface NodeIdentity {
  nodeId: string;
  sessionId?: string;
  nodePath?: readonly string[];
  iteration?: number;
  branchId?: string;
}

function flattenPlan(
  nodes: readonly WorkflowNodeDescriptor[],
  parentId: string | null = null,
  depth = 0
): WorkflowMonitorNode[] {
  const flattened: WorkflowMonitorNode[] = [];
  for (const node of nodes) {
    flattened.push({
      id: node.nodeId,
      type: node.type,
      status: 'pending',
      label: node.agentName ?? node.nodeId,
      parentId,
      depth,
      agentName: node.agentName,
      modelId: node.modelId,
      effortLevel: node.effortLevel,
      maxIterations: node.maxIterations,
    });
    if (node.steps) {
      flattened.push(...flattenPlan(node.steps, node.nodeId, depth + 1));
    }
    if (node.branches) {
      flattened.push(...flattenPlan(node.branches, node.nodeId, depth + 1));
    }
  }
  return flattened;
}

export function appendQueuedPlan(
  nodes: readonly WorkflowMonitorNode[],
  pendingSteps: readonly WorkflowNodeDescriptor[]
): { nodes: WorkflowMonitorNode[]; queuedNodeIds: string[] } {
  const existingIds = new Set(nodes.map((node) => node.id));
  const appended = flattenPlan(pendingSteps).filter(
    (node) => !existingIds.has(node.id)
  );
  return {
    nodes: [...nodes, ...appended],
    queuedNodeIds: appended.map((node) => node.id),
  };
}

function flattenState(root: WorkflowNodeState): WorkflowMonitorNode[] {
  return workflowStateEntries(root).map(
    ({ state, parentId, nodePath, depth }) => ({
      id: state.nodeId,
      type: state.type,
      status: pausedBeforeStart(state) ? 'pending' : state.status,
      label: state.agentName ?? state.nodeId,
      parentId,
      depth,
      nodePath,
      sessionId: state.sessionId,
      agentName: state.agentName,
      modelId: state.modelId,
      effortLevel: state.effortLevel,
      iteration: state.iteration,
      branchId: state.branchId,
      failureReason: state.failureReason,
      capturedOutput: state.capturedOutput,
      completionSignal: state.completionSignal,
      completionSignalSource: state.completionSignalSource,
    })
  );
}

/**
 * A boundary park marks the node the run stopped *before* as `paused` even
 * though it never ran. KAS stamps `startedAt` and opens the step session only
 * after that pause check, so an unstarted park is uniquely `paused` with
 * neither — a mid-flight park always carries at least one. Steps only: a
 * container never owns a session, so for one the test would collapse to
 * `paused && !startedAt` and drag it back to `pending`.
 */
function pausedBeforeStart(state: WorkflowNodeState): boolean {
  return (
    state.type === 'step' &&
    state.status === 'paused' &&
    state.startedAt === undefined &&
    state.sessionId === undefined
  );
}

/**
 * `WorkflowNodeState` has no `pauseReason`, so after a reload the run-level one
 * is the only park text left — without it the footer offers `s respond` for a
 * question that is nowhere on screen. The run record holds exactly one reason,
 * so only one node can claim it: a `paused` step owning a session, which is also
 * the only node the composer will send an answer into. Ambiguous matches take
 * none, since a missing reason reads as unknown where a wrong one reads as fact.
 */
function restoreParkReason(
  nodes: readonly WorkflowMonitorNode[],
  state: WorkflowStateSnapshot
): WorkflowMonitorNode[] {
  const reason = state.pauseReason?.trim();
  if (state.status !== 'paused' || !reason) return [...nodes];
  const candidates = nodes.filter(
    (node) =>
      node.type === 'step' &&
      node.status === 'paused' &&
      node.sessionId !== undefined &&
      node.pauseReason === undefined
  );
  if (candidates.length !== 1) return [...nodes];
  const parked = candidates[0];
  return nodes.map((node) =>
    node === parked ? { ...node, pauseReason: reason } : node
  );
}

function reconcileNodes(
  nodes: readonly WorkflowMonitorNode[],
  root: WorkflowNodeState
): WorkflowMonitorNode[] {
  const entries = workflowStateEntries(root);
  return nodes.map((node) => {
    const match =
      entries.find(
        ({ state }) =>
          node.sessionId !== undefined && state.sessionId === node.sessionId
      ) ??
      entries.find(({ nodePath }) =>
        workflowNodePathsEqual(node.nodePath, nodePath)
      ) ??
      [...entries].reverse().find(({ state }) => state.nodeId === node.id);
    if (!match) return node;
    const state = match.state;
    return {
      ...node,
      status: pausedBeforeStart(state) ? 'pending' : state.status,
      sessionId: state.sessionId ?? node.sessionId,
      nodePath: match.nodePath,
      agentName: state.agentName ?? node.agentName,
      modelId: state.modelId ?? node.modelId,
      effortLevel: state.effortLevel ?? node.effortLevel,
      iteration: state.iteration ?? node.iteration,
      branchId: state.branchId ?? node.branchId,
      failureReason: state.failureReason ?? node.failureReason,
      capturedOutput: state.capturedOutput ?? node.capturedOutput,
      completionSignal: state.completionSignal ?? node.completionSignal,
      completionSignalSource:
        state.completionSignalSource ?? node.completionSignalSource,
      // Carried, never invented, and dropped once the step is no longer parked.
      pauseReason:
        state.status === 'paused' || state.status === 'failed'
          ? node.pauseReason
          : undefined,
    };
  });
}

function carryForwardNodes(
  fresh: readonly WorkflowMonitorNode[],
  previous: readonly WorkflowMonitorNode[]
): WorkflowMonitorNode[] {
  return fresh.map((node) => {
    const prior = previous.findLast((candidate) => candidate.id === node.id);
    if (!prior) return node;
    return {
      ...node,
      status: prior.status,
      sessionId: prior.sessionId,
      nodePath: prior.nodePath,
      agentName: prior.agentName ?? node.agentName,
      modelId: prior.modelId ?? node.modelId,
      effortLevel: prior.effortLevel ?? node.effortLevel,
      iteration: prior.iteration,
      branchId: prior.branchId,
      durationSecs: prior.durationSecs,
      failureReason: prior.failureReason,
      capturedOutput: prior.capturedOutput,
      completionSignal: prior.completionSignal,
      pauseReason: prior.pauseReason,
      maxIterations: prior.maxIterations ?? node.maxIterations,
    };
  });
}

// X12: iteration/child rows inherit maxIterations from their nearest ancestor
// (the repeat/loop container carries the real N; child rows only get an
// `iteration`), so the ↻n/N denominator resolves instead of showing ↻n/?.
function ancestorMaxIterations(
  nodes: readonly WorkflowMonitorNode[],
  node: WorkflowMonitorNode | undefined
): number | undefined {
  let parentId = node?.parentId ?? null;
  const seen = new Set<string>();
  while (parentId !== null && !seen.has(parentId)) {
    seen.add(parentId);
    const parent = nodes.find((candidate) => candidate.id === parentId);
    if (parent?.maxIterations !== undefined) return parent.maxIterations;
    parentId = parent?.parentId ?? null;
  }
  return undefined;
}

function nodeMatches(
  node: WorkflowMonitorNode,
  identity: NodeIdentity
): boolean {
  if (node.id !== identity.nodeId) return false;
  if (
    identity.sessionId !== undefined &&
    node.sessionId !== undefined &&
    identity.sessionId !== node.sessionId
  ) {
    return false;
  }
  if (
    identity.nodePath !== undefined &&
    node.nodePath !== undefined &&
    !workflowNodePathsEqual(identity.nodePath, node.nodePath)
  ) {
    return false;
  }
  if (
    identity.iteration !== undefined &&
    node.iteration !== undefined &&
    identity.iteration !== node.iteration
  ) {
    return false;
  }
  if (
    identity.branchId !== undefined &&
    node.branchId !== undefined &&
    identity.branchId !== node.branchId
  ) {
    return false;
  }
  return true;
}

function patchNodes(
  nodes: readonly WorkflowMonitorNode[],
  identity: NodeIdentity,
  patch: Partial<WorkflowMonitorNode>
): WorkflowMonitorNode[] {
  return nodes.map((node) =>
    nodeMatches(node, identity) ? { ...node, ...patch } : node
  );
}

/** Same id, same path once the repeat iteration that produced each is ignored. */
function nodeMatchesAcrossIterations(
  node: WorkflowMonitorNode,
  identity: NodeIdentity
): boolean {
  if (node.id !== identity.nodeId) return false;
  if (identity.nodePath === undefined || node.nodePath === undefined) {
    return true;
  }
  return workflowNodePathsEqual(
    workflowNodePathWithoutIterations(identity.nodePath),
    workflowNodePathWithoutIterations(node.nodePath)
  );
}

function patchLastNodeAcrossIterations(
  nodes: readonly WorkflowMonitorNode[],
  identity: NodeIdentity,
  patch: Partial<WorkflowMonitorNode>
): WorkflowMonitorNode[] {
  const target = nodes.reduce(
    (last, node, index) =>
      nodeMatchesAcrossIterations(node, identity) ? index : last,
    -1
  );
  if (target < 0) return [...nodes];
  const next = [...nodes];
  next[target] = { ...nodes[target]!, ...patch };
  return next;
}

type WorkflowNodeStartEvent = Extract<
  WorkflowProgressEvent,
  { type: 'node_start' }
>;

/** KAS names a repeat wrapper `<nodeId>#N`; the plan only knows `<nodeId>`. */
function baseNodeId(nodeId: string): string {
  return nodeId.replace(/#\d+$/, '');
}

/**
 * The row a starting instance clones its plan-derived shape from. The type gate
 * is load-bearing: `#N` stripping alone would let an instance clone a row of a
 * different kind that happens to share its base id.
 */
function startedNodeTemplate(
  nodes: readonly WorkflowMonitorNode[],
  nodeId: string,
  nodeType: WorkflowNodeType
): { node: WorkflowMonitorNode; index: number } | undefined {
  const exact = nodes.findIndex(
    (node) => node.id === nodeId && node.type === nodeType
  );
  const index =
    exact >= 0
      ? exact
      : nodes.findIndex(
          (node) =>
            baseNodeId(node.id) === baseNodeId(nodeId) && node.type === nodeType
        );
  const node = index >= 0 ? nodes[index] : undefined;
  return node ? { node, index } : undefined;
}

/**
 * First index past the template's own subtree and any sibling instance of it, so
 * a new iteration lands beside its siblings rather than at the end of the list.
 */
function instanceBlockEnd(
  nodes: readonly WorkflowMonitorNode[],
  template: WorkflowMonitorNode,
  templateIndex: number
): number {
  let end = templateIndex + 1;
  while (end < nodes.length) {
    const candidate = nodes[end]!;
    const isSiblingInstance =
      candidate.depth === template.depth &&
      baseNodeId(candidate.id) === baseNodeId(template.id);
    if (candidate.depth <= template.depth && !isSiblingInstance) break;
    end += 1;
  }
  return end;
}

/**
 * Binds a starting node to exactly one row, appending one when nothing matches —
 * otherwise a loop iteration past the first patches nothing and the monitor
 * stays pinned to the previous iteration's finished session. The identity omits
 * `sessionId`, so a retry of the same instance re-binds its row.
 */
function upsertStartedNode(
  nodes: readonly WorkflowMonitorNode[],
  identity: NodeIdentity,
  event: WorkflowNodeStartEvent
): { nodes: WorkflowMonitorNode[]; index: number; appended: boolean } {
  const patch: Partial<WorkflowMonitorNode> = {
    status: 'running',
    sessionId: event.sessionId,
    nodePath: event.nodePath,
    agentName: event.agentName,
    iteration: event.iteration,
    branchId: event.branchId,
  };
  const matched = nodes.findIndex((node) => nodeMatches(node, identity));
  if (matched >= 0) {
    const target = nodes[matched]!;
    const inherited =
      event.iteration !== undefined && target.maxIterations === undefined
        ? ancestorMaxIterations(nodes, target)
        : undefined;
    const next = [...nodes];
    next[matched] = {
      ...target,
      ...patch,
      ...(inherited !== undefined ? { maxIterations: inherited } : {}),
    };
    return { nodes: next, index: matched, appended: false };
  }
  // Only a step earns a row of its own. Rows are parented by plan id, which
  // cannot tell two instances of one container apart, so a second container row
  // would draw as a leaf with every iteration's children under the first.
  if (event.nodeType !== 'step') {
    return { nodes: [...nodes], index: -1, appended: false };
  }
  const template = startedNodeTemplate(nodes, event.nodeId, event.nodeType);
  if (!template) return { nodes: [...nodes], index: -1, appended: false };
  const insertAt = instanceBlockEnd(nodes, template.node, template.index);
  const appended: WorkflowMonitorNode = {
    ...patch,
    id: event.nodeId,
    type: template.node.type,
    status: 'running',
    label: template.node.label,
    parentId: template.node.parentId,
    depth: template.node.depth,
    modelId: template.node.modelId,
    effortLevel: template.node.effortLevel,
    maxIterations:
      template.node.maxIterations ??
      ancestorMaxIterations(nodes, template.node),
  };
  return {
    nodes: [...nodes.slice(0, insertAt), appended, ...nodes.slice(insertAt)],
    index: insertAt,
    appended: true,
  };
}

function sessionMatches(
  session: WorkflowMonitorSession,
  identity: NodeIdentity
): boolean {
  if (session.nodeId !== identity.nodeId) return false;
  if (identity.sessionId !== undefined) {
    return session.sessionId === identity.sessionId;
  }
  if (
    identity.nodePath !== undefined &&
    !workflowNodePathsEqual(identity.nodePath, session.nodePath)
  ) {
    return false;
  }
  if (
    identity.iteration !== undefined &&
    session.iteration !== undefined &&
    identity.iteration !== session.iteration
  ) {
    return false;
  }
  if (
    identity.branchId !== undefined &&
    session.branchId !== undefined &&
    identity.branchId !== session.branchId
  ) {
    return false;
  }
  return true;
}

function patchLatestSession(
  sessions: readonly WorkflowMonitorSession[],
  identity: NodeIdentity,
  patch: Partial<WorkflowMonitorSession>
): WorkflowMonitorSession[] {
  const next = sessions.map((session) => ({ ...session }));
  for (let index = next.length - 1; index >= 0; index -= 1) {
    const session = next[index];
    if (session && sessionMatches(session, identity)) {
      next[index] = { ...session, ...patch };
      break;
    }
  }
  return next;
}

function mergeSessions(
  current: readonly WorkflowMonitorSession[],
  incoming: readonly WorkflowMonitorSession[]
): WorkflowMonitorSession[] {
  const bySession = new Map(
    current.map((session) => [session.sessionId, { ...session }] as const)
  );
  for (const session of incoming) {
    bySession.set(session.sessionId, {
      ...bySession.get(session.sessionId),
      ...session,
    });
  }
  return [...bySession.values()];
}

export function collectWorkflowSessions(
  root: WorkflowNodeState | undefined
): WorkflowMonitorSession[] {
  if (!root) return [];
  return workflowStateEntries(root).flatMap(({ state, nodePath }) =>
    state.type === 'step' && state.sessionId !== undefined
      ? [
          {
            nodeId: state.nodeId,
            nodePath,
            sessionId: state.sessionId,
            iteration: state.iteration,
            branchId: state.branchId,
            status: state.status,
            agentName: state.agentName,
          },
        ]
      : []
  );
}

function terminalizeNodes(
  nodes: readonly WorkflowMonitorNode[],
  runStatus: WorkflowStatus
): WorkflowMonitorNode[] {
  const fallback: WorkflowNodeStatus =
    runStatus === 'completed' ? 'completed' : 'aborted';
  return nodes.map((node) =>
    isActiveWorkflowNodeStatus(node.status)
      ? { ...node, status: fallback }
      : node
  );
}

function replaceRun(
  state: WorkflowCollectionState,
  run: WorkflowRunView,
  selectedIndex?: number
): WorkflowCollectionState {
  const workflows = new Map(state.workflows).set(run.workflowId, run);
  const selectedNodeIndices =
    selectedIndex === undefined
      ? state.selectedNodeIndices
      : new Map(state.selectedNodeIndices).set(run.workflowId, selectedIndex);
  return { ...state, workflows, selectedNodeIndices };
}

function removeLiveRun(
  state: WorkflowCollectionState,
  workflowId: string
): WorkflowCollectionState {
  const workflows = new Map(state.workflows);
  workflows.delete(workflowId);
  const selectedNodeIndices = new Map(state.selectedNodeIndices);
  selectedNodeIndices.delete(workflowId);
  const activeWorkflowId =
    state.activeWorkflowId === workflowId
      ? (workflows.keys().next().value ?? null)
      : state.activeWorkflowId;
  return { ...state, workflows, selectedNodeIndices, activeWorkflowId };
}

function completeRun(
  state: WorkflowCollectionState,
  run: WorkflowRunView
): WorkflowCollectionState {
  const archivedWorkflows = new Map(state.archivedWorkflows).set(
    run.workflowId,
    run
  );
  const retained = replaceRun(state, run);
  if (state.openWorkflowSurfaces.size > 0) {
    return {
      ...retained,
      archivedWorkflows,
      pauseRequestedWorkflowIds: withoutId(
        retained.pauseRequestedWorkflowIds,
        run.workflowId
      ),
    };
  }
  return {
    ...removeLiveRun(retained, run.workflowId),
    archivedWorkflows,
    pauseRequestedWorkflowIds: withoutId(
      retained.pauseRequestedWorkflowIds,
      run.workflowId
    ),
  };
}

function withoutId(
  values: ReadonlySet<string>,
  id: string
): ReadonlySet<string> {
  if (!values.has(id)) return values;
  const next = new Set(values);
  next.delete(id);
  return next;
}

function parseTimestampOr(value: string | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function restoredSelectionIndex(nodes: readonly WorkflowMonitorNode[]): number {
  const needsInput = nodes.findIndex(
    (node) =>
      node.sessionId !== undefined &&
      node.status === 'paused' &&
      node.completionSignal === 'need_input'
  );
  if (needsInput >= 0) return needsInput;

  const activeSession = nodes.findIndex(
    (node) =>
      node.sessionId !== undefined &&
      (node.status === 'running' || node.status === 'paused')
  );
  if (activeSession >= 0) return activeSession;

  return Math.max(
    0,
    nodes.findIndex(
      (node) => node.status === 'running' || node.status === 'paused'
    )
  );
}

function matchingRetainedNodeIndex(
  selected: WorkflowMonitorNode,
  nodes: readonly WorkflowMonitorNode[]
): number {
  if (selected.sessionId !== undefined) {
    const sessionMatch = nodes.findIndex(
      (node) => node.sessionId === selected.sessionId
    );
    if (sessionMatch >= 0) return sessionMatch;
  }
  const selectedPath = selected.nodePath;
  if (selectedPath !== undefined) {
    const pathMatch = nodes.findIndex(
      (node) =>
        node.nodePath !== undefined &&
        workflowNodePathsEqual(node.nodePath, selectedPath)
    );
    if (pathMatch >= 0) return pathMatch;
  }
  if (selected.iteration !== undefined || selected.branchId !== undefined) {
    const instanceMatch = nodes.findIndex(
      (node) =>
        node.id === selected.id &&
        node.type === selected.type &&
        node.iteration === selected.iteration &&
        node.branchId === selected.branchId
    );
    if (instanceMatch >= 0) return instanceMatch;
  }
  if (
    selected.sessionId !== undefined ||
    selected.nodePath !== undefined ||
    selected.iteration !== undefined ||
    selected.branchId !== undefined
  ) {
    return -1;
  }
  return nodes.findIndex(
    (node) =>
      node.id === selected.id &&
      node.type === selected.type &&
      node.parentId === selected.parentId &&
      node.depth === selected.depth
  );
}

function retainedSelectionIndex(
  state: WorkflowCollectionState,
  workflowId: string,
  nodes: readonly WorkflowMonitorNode[]
): number | undefined {
  const retained = state.selectedNodeIndices.get(workflowId);
  if (retained === undefined) return undefined;
  const selected = state.workflows.get(workflowId)?.nodes[retained];
  if (selected !== undefined) {
    const matched = matchingRetainedNodeIndex(selected, nodes);
    if (matched >= 0) return matched === retained ? undefined : matched;
  }
  const fallback = restoredSelectionIndex(nodes);
  return fallback < nodes.length ? fallback : undefined;
}

export function reduceWorkflowEvent(
  state: WorkflowCollectionState,
  event: WorkflowProgressEvent,
  now: number
): WorkflowCollectionState {
  if (event.type === 'run_snapshot') {
    const previous = state.workflows.get(event.workflowId);
    const nodes = restoreParkReason(
      buildWorkflowNodesFromState(event.nodePlan, event.state.root),
      event.state
    );
    const stepSessions = mergeSessions(
      event.stepSessions,
      collectWorkflowSessions(event.state.root)
    );
    const restored: WorkflowRunView = {
      workflowId: event.workflowId,
      parentSessionId: event.parentSessionId,
      name: event.state.workflowName,
      status: event.state.status,
      nodes,
      stepSessions,
      startedAt:
        previous?.startedAt ?? parseTimestampOr(event.state.createdAt, now),
      completedAt: null,
      pauseReason: event.state.pauseReason,
      stopInitiator: event.state.stopInitiator,
      stopReason: event.state.stopReason,
    };
    const selectedIndex = previous
      ? retainedSelectionIndex(state, event.workflowId, nodes)
      : restoredSelectionIndex(nodes);
    const restoredState = replaceRun(state, restored, selectedIndex);
    const archivedWorkflows = new Map(restoredState.archivedWorkflows);
    archivedWorkflows.delete(event.workflowId);
    return {
      ...restoredState,
      archivedWorkflows,
      activeWorkflowId: restoredState.activeWorkflowId ?? event.workflowId,
      pauseRequestedWorkflowIds: withoutId(
        restoredState.pauseRequestedWorkflowIds,
        event.workflowId
      ),
    };
  }

  if (event.type === 'run_start') {
    const previous = state.workflows.get(event.workflowId);
    const freshNodes = flattenPlan(event.nodeTree);
    const run: WorkflowRunView = {
      workflowId: event.workflowId,
      parentSessionId: event.parentSessionId ?? previous?.parentSessionId,
      name: event.workflowName,
      status: 'running',
      nodes: previous
        ? carryForwardNodes(freshNodes, previous.nodes)
        : freshNodes,
      stepSessions: previous?.stepSessions ?? [],
      startedAt: previous?.startedAt ?? now,
      completedAt: null,
    };
    const workflows = new Map(state.workflows).set(event.workflowId, run);
    const archivedWorkflows = new Map(state.archivedWorkflows);
    archivedWorkflows.delete(event.workflowId);
    const selectedNodeIndices = new Map(state.selectedNodeIndices);
    if (!previous) {
      selectedNodeIndices.set(event.workflowId, 0);
    } else {
      const selectedIndex = retainedSelectionIndex(
        state,
        event.workflowId,
        run.nodes
      );
      if (selectedIndex !== undefined) {
        selectedNodeIndices.set(event.workflowId, selectedIndex);
      }
    }
    const current = state.activeWorkflowId
      ? state.workflows.get(state.activeWorkflowId)
      : undefined;
    const activeWorkflowId =
      state.activeWorkflowId === null ||
      (!previous && isTerminalWorkflowStatus(current?.status))
        ? event.workflowId
        : state.activeWorkflowId;
    return {
      ...state,
      workflows,
      archivedWorkflows,
      selectedNodeIndices,
      activeWorkflowId,
      pauseRequestedWorkflowIds: withoutId(
        state.pauseRequestedWorkflowIds,
        event.workflowId
      ),
    };
  }

  const run = state.workflows.get(event.workflowId);
  if (!run) return state;

  switch (event.type) {
    case 'node_start': {
      const identity: NodeIdentity = {
        nodeId: event.nodeId,
        nodePath: event.nodePath,
        iteration: event.iteration,
        branchId: event.branchId,
      };
      const started = upsertStartedNode(run.nodes, identity, event);
      const nodes = started.nodes;
      const stepSessions = event.sessionId
        ? mergeSessions(run.stepSessions, [
            {
              nodeId: event.nodeId,
              nodePath: event.nodePath,
              sessionId: event.sessionId,
              iteration: event.iteration,
              branchId: event.branchId,
              status: 'running',
              agentName: event.agentName,
            },
          ])
        : run.stepSessions;
      const retained = state.selectedNodeIndices.get(event.workflowId);
      // An insertion shifts every row at or after it down one, so a locked
      // selection has to travel with the row it named or the composer would
      // start answering a different session mid-message.
      const lockedIndex =
        started.appended && retained !== undefined && started.index <= retained
          ? retained + 1
          : undefined;
      const selectedIndex = state.selectionLocked ? lockedIndex : started.index;
      // Clearing the attribution keeps a later autonomous abort from inheriting
      // `'user'` and reading "Stopped by you."
      const resumed =
        run.status === 'paused'
          ? {
              status: 'running' as const,
              pauseReason: undefined,
              stopInitiator: undefined,
              stopReason: undefined,
            }
          : {};
      return replaceRun(
        state,
        { ...run, ...resumed, nodes, stepSessions },
        selectedIndex !== undefined && selectedIndex >= 0
          ? selectedIndex
          : undefined
      );
    }
    case 'node_complete': {
      const identity: NodeIdentity = {
        nodeId: event.nodeId,
        sessionId: event.sessionId,
        nodePath: event.nodePath,
        iteration: event.iteration,
        branchId: event.branchId,
      };
      return replaceRun(state, {
        ...run,
        nodes: patchNodes(run.nodes, identity, {
          status: event.status,
          durationSecs: event.durationSecs,
          failureReason: event.failureReason,
          capturedOutput: event.capturedOutput,
        }),
        stepSessions: patchLatestSession(run.stepSessions, identity, {
          status: event.status,
        }),
      });
    }
    case 'node_paused': {
      const identity: NodeIdentity = {
        nodeId: event.nodeId,
        sessionId: event.sessionId,
        nodePath: event.nodePath,
        iteration: event.iteration,
        branchId: event.branchId,
      };
      // A boundary park names the node the run stopped *before*, which hasn't
      // started, so record the reason but leave a pending node pending. Decided
      // per node because `node_paused` carries no `iteration`: in an unrolled
      // loop one pending sibling must not suppress the park on the instance
      // that really stopped.
      return replaceRun(state, {
        ...run,
        nodes: run.nodes.map((node) =>
          nodeMatches(node, identity)
            ? {
                ...node,
                ...(node.status === 'pending'
                  ? {}
                  : { status: 'paused' as const }),
                pauseReason: event.reason,
              }
            : node
        ),
        stepSessions: patchLatestSession(run.stepSessions, identity, {
          status: 'paused',
        }),
      });
    }
    case 'loop_iteration':
      return replaceRun(state, {
        ...run,
        nodes: patchNodes(
          run.nodes,
          { nodeId: event.loopId },
          { iteration: event.iteration }
        ),
      });
    case 'watch_poll': {
      const identity: NodeIdentity = {
        nodeId: event.nodeId,
        nodePath: event.nodePath,
      };
      const patch = { watchOutcome: event.outcome };
      // Only a step earns a row per iteration, so a repeat-nested watch keeps
      // the path of the iteration that first started it while every later poll
      // carries its own — without the relaxed retry those outcomes vanish. The
      // retry binds one row, the newest, so an outcome never lands on a row an
      // earlier iteration already reported.
      const nodes = run.nodes.some((node) => nodeMatches(node, identity))
        ? patchNodes(run.nodes, identity, patch)
        : patchLastNodeAcrossIterations(run.nodes, identity, patch);
      return replaceRun(state, { ...run, nodes });
    }
    case 'paused':
      return replaceRun(
        {
          ...state,
          pauseRequestedWorkflowIds: withoutId(
            state.pauseRequestedWorkflowIds,
            event.workflowId
          ),
        },
        {
          ...run,
          status: 'paused',
          pauseReason: event.pauseReason,
          stopInitiator: event.initiator,
          stopReason: event.initiatorReason,
        }
      );
    case 'run_complete': {
      const snapshot = event.finalState;
      const reconciled = snapshot
        ? reconcileNodes(run.nodes, snapshot.root)
        : run.nodes;
      const paused = event.status === 'paused';
      const nodes = paused
        ? reconciled
        : terminalizeNodes(reconciled, event.status);
      const sessions = mergeSessions(
        run.stepSessions,
        collectWorkflowSessions(snapshot?.root)
      ).map((session) => {
        if (
          paused ||
          (session.status !== undefined &&
            !isActiveWorkflowNodeStatus(session.status))
        ) {
          return session;
        }
        return {
          ...session,
          status:
            event.status === 'completed'
              ? ('completed' as const)
              : ('aborted' as const),
        };
      });
      const completed: WorkflowRunView = {
        ...run,
        status: event.status,
        nodes,
        queuedNodeIds: undefined,
        stepSessions: sessions,
        completedAt: paused ? run.completedAt : now,
        pauseReason:
          snapshot?.pauseReason ?? (paused ? run.pauseReason : undefined),
        stopInitiator:
          event.initiator ?? snapshot?.stopInitiator ?? run.stopInitiator,
        stopReason:
          event.initiatorReason ?? snapshot?.stopReason ?? run.stopReason,
      };
      if (paused) {
        // Any parked step, `need_input` or not: a completion-gated park is
        // equally waiting on the user.
        const selectedIndex = state.selectionLocked
          ? undefined
          : nodes.findIndex((node) => node.status === 'paused');
        return replaceRun(
          {
            ...state,
            pauseRequestedWorkflowIds: withoutId(
              state.pauseRequestedWorkflowIds,
              event.workflowId
            ),
          },
          completed,
          selectedIndex !== undefined && selectedIndex >= 0
            ? selectedIndex
            : undefined
        );
      }
      return completeRun(state, completed);
    }
    case 'steps_queued': {
      const previousQueuedIds = new Set(run.queuedNodeIds ?? []);
      const stableNodes = run.nodes.filter(
        (node) => !previousQueuedIds.has(node.id)
      );
      if (event.resolution !== undefined) {
        const retainAppliedNodes = event.resolution.outcome === 'applied';
        return replaceRun(state, {
          ...run,
          nodes: retainAppliedNodes ? run.nodes : stableNodes,
          queuedNodeIds: undefined,
        });
      }
      const queuedPlan = appendQueuedPlan(stableNodes, event.pendingSteps);
      return replaceRun(state, {
        ...run,
        nodes: queuedPlan.nodes,
        queuedNodeIds: queuedPlan.queuedNodeIds,
      });
    }
    default:
      return state;
  }
}

export function buildWorkflowNodesFromState(
  plan: readonly WorkflowNodeDescriptor[] | undefined,
  root: WorkflowNodeState | undefined
): WorkflowMonitorNode[] {
  if (root && (!plan || plan.length === 0)) return flattenState(root);
  const nodes = flattenPlan(plan ?? []);
  return root ? reconcileNodes(nodes, root) : nodes;
}

export function pruneTerminalWorkflowRuns(
  state: WorkflowCollectionState
): WorkflowCollectionState {
  const workflows = new Map(
    [...state.workflows].filter(
      ([, workflow]) => !isTerminalWorkflowStatus(workflow.status)
    )
  );
  const selectedNodeIndices = new Map(
    [...state.selectedNodeIndices].filter(([id]) => workflows.has(id))
  );
  const activeWorkflowId =
    state.activeWorkflowId && workflows.has(state.activeWorkflowId)
      ? state.activeWorkflowId
      : (workflows.keys().next().value ?? null);
  return { ...state, workflows, selectedNodeIndices, activeWorkflowId };
}
