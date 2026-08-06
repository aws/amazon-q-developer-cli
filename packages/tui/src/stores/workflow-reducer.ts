import type {
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowProgressEvent,
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
      status: state.status,
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
      pauseReason:
        state.completionSignal === 'need_input'
          ? 'Waiting for your input'
          : undefined,
    })
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
      status: state.status,
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
      pauseReason:
        state.completionSignal === 'need_input'
          ? (node.pauseReason ?? 'Waiting for your input')
          : node.pauseReason,
    };
  });
}

function carryForwardNodes(
  fresh: readonly WorkflowMonitorNode[],
  previous: readonly WorkflowMonitorNode[]
): WorkflowMonitorNode[] {
  return fresh.map((node) => {
    const prior = previous.find((candidate) => candidate.id === node.id);
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

export function reduceWorkflowEvent(
  state: WorkflowCollectionState,
  event: WorkflowProgressEvent,
  now: number
): WorkflowCollectionState {
  if (event.type === 'run_snapshot') {
    const previous = state.workflows.get(event.workflowId);
    const nodes = buildWorkflowNodesFromState(event.nodePlan, event.state.root);
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
    };
    const selectedIndex = previous ? undefined : restoredSelectionIndex(nodes);
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
    if (!previous) selectedNodeIndices.set(event.workflowId, 0);
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
      const targetNode = run.nodes.find((node) => nodeMatches(node, identity));
      const inheritedMaxIterations =
        event.iteration !== undefined && targetNode?.maxIterations === undefined
          ? ancestorMaxIterations(run.nodes, targetNode)
          : undefined;
      const nodes = patchNodes(run.nodes, identity, {
        status: 'running',
        sessionId: event.sessionId,
        nodePath: event.nodePath,
        agentName: event.agentName,
        iteration: event.iteration,
        branchId: event.branchId,
        ...(inheritedMaxIterations !== undefined
          ? { maxIterations: inheritedMaxIterations }
          : {}),
      });
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
      const selectedIndex = state.selectionLocked
        ? undefined
        : nodes.findIndex(
            (node) => node.id === event.nodeId && node.type === event.nodeType
          );
      return replaceRun(
        state,
        { ...run, nodes, stepSessions },
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
      return replaceRun(state, {
        ...run,
        nodes: patchNodes(run.nodes, identity, {
          status: 'paused',
          pauseReason: event.reason,
        }),
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
    case 'watch_poll':
      return replaceRun(state, {
        ...run,
        nodes: patchNodes(
          run.nodes,
          { nodeId: event.nodeId },
          { watchOutcome: event.outcome }
        ),
      });
    case 'paused':
      return replaceRun(
        {
          ...state,
          pauseRequestedWorkflowIds: withoutId(
            state.pauseRequestedWorkflowIds,
            event.workflowId
          ),
        },
        { ...run, status: 'paused', pauseReason: event.pauseReason }
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
      };
      if (paused) {
        const selectedIndex = state.selectionLocked
          ? undefined
          : nodes.findIndex(
              (node) =>
                node.status === 'paused' &&
                node.completionSignal === 'need_input'
            );
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
