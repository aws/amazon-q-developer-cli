import { describe, expect, it } from 'bun:test';
import type {
  WorkflowEvent,
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowStateSnapshot,
} from '../../types/workflow.js';
import type {
  WorkflowInspectResponse,
  WorkflowRunSummary,
} from '../../types/workflow-history.js';
import type { WorkflowMonitorNode } from '../../types/workflow-monitor.js';
import {
  createWorkflowStore,
  selectActiveWorkflow,
  selectLiveWorkflowCount,
  selectWorkflowNodeIndex,
} from '../workflow-store.js';
import {
  buildHistoricalWorkflowRun,
  buildWorkflowNodeConversations,
  workflowActivityCounts,
  workflowActivitySummary,
  workflowProgress,
} from '../workflow-view-model.js';

const step = (
  nodeId: string,
  overrides: Partial<WorkflowNodeDescriptor> = {}
): WorkflowNodeDescriptor => ({
  nodeId,
  type: 'step',
  agentName: nodeId,
  ...overrides,
});

const startEvent = (
  workflowId: string,
  nodes: WorkflowNodeDescriptor[] = [step('one'), step('two')]
): Extract<WorkflowEvent, { type: 'run_start' }> => ({
  type: 'run_start',
  workflowId,
  parentSessionId: 'parent-1',
  workflowName: `Workflow ${workflowId}`,
  inputs: {},
  nodeTree: nodes,
});

const snapshot = (
  workflowId: string,
  status: WorkflowStateSnapshot['status'] = 'completed'
): WorkflowStateSnapshot => ({
  workflowId,
  workflowName: `Workflow ${workflowId}`,
  status,
  inputs: {},
  artifacts: {},
  capturedOutputs: {},
  parentSessionId: 'parent-1',
  root: {
    nodeId: 'root',
    type: 'sequence',
    status,
    children: [
      {
        nodeId: 'one',
        type: 'step',
        status: 'completed',
        sessionId: 'session-one',
        agentName: 'coder',
      },
      {
        nodeId: 'two',
        type: 'step',
        status: status === 'paused' ? 'paused' : status,
        sessionId: 'session-two',
        agentName: 'reviewer',
        completionSignal: status === 'paused' ? 'need_input' : 'success',
      },
    ],
  },
});

const repeatPlan = (): WorkflowNodeDescriptor[] => [
  {
    nodeId: 'loop',
    type: 'repeat',
    maxIterations: 3,
    steps: [step('review')],
  },
];

const iterationPath = (iteration: number): readonly string[] => [
  'root',
  'loop',
  `iter-${iteration}`,
  'review',
];

const iterationStart = (
  workflowId: string,
  iteration: number
): Extract<WorkflowEvent, { type: 'node_start' }> => ({
  type: 'node_start',
  workflowId,
  parentSessionId: 'parent-1',
  nodeId: 'review',
  nodePath: iterationPath(iteration),
  nodeType: 'step',
  sessionId: `session-${iteration}`,
  iteration,
  agentName: 'reviewer',
});

/** A `node_start` for a repeat-body step, on the canonical `iter-N` path. */
const bodyStart = (
  workflowId: string,
  nodeId: string,
  iteration: number,
  ancestors: readonly string[] = []
): Extract<WorkflowEvent, { type: 'node_start' }> => ({
  type: 'node_start',
  workflowId,
  parentSessionId: 'parent-1',
  nodeId,
  nodePath: ['root', 'loop', `iter-${iteration}`, ...ancestors, nodeId],
  nodeType: 'step',
  sessionId: `${nodeId}-${iteration}`,
  iteration,
});

const iterationComplete = (
  workflowId: string,
  iteration: number,
  status: WorkflowNodeStatus = 'completed'
): Extract<WorkflowEvent, { type: 'node_complete' }> => ({
  type: 'node_complete',
  workflowId,
  parentSessionId: 'parent-1',
  nodeId: 'review',
  nodePath: iterationPath(iteration),
  status,
  sessionId: `session-${iteration}`,
  iteration,
  durationSecs: iteration + 1,
});

/** Runs a repeat plan through `count` body iterations, each with its own session. */
const startedIterations = (workflowId: string, count: number) => {
  const store = createWorkflowStore(() => 100);
  store.getState().applyEvent(startEvent(workflowId, repeatPlan()));
  for (let iteration = 0; iteration < count; iteration += 1) {
    store.getState().applyEvent(iterationStart(workflowId, iteration));
  }
  return store;
};

/**
 * The durable shape KAS persists for an unrolled repeat: a per-iteration
 * `sequence` wrapper around the body step, which is what makes the canonical
 * `iter-N` path segments agree with the live events above.
 */
const repeatSnapshotRoot = (
  status: WorkflowNodeStatus = 'completed'
): WorkflowNodeState => ({
  nodeId: 'root',
  type: 'sequence',
  status,
  children: [
    {
      nodeId: 'loop',
      type: 'repeat',
      status,
      iteration: 1,
      children: [0, 1].map((iteration) => ({
        nodeId: `loop#${iteration}`,
        type: 'sequence',
        status: 'completed',
        iteration,
        children: [
          {
            nodeId: 'review',
            type: 'step',
            status: 'completed',
            iteration,
            sessionId: `session-${iteration}`,
            startedAt: '2026-08-10T00:00:00.000Z',
            agentName: 'reviewer',
          },
        ],
      })),
    },
  ],
});

const repeatSnapshot = (workflowId: string): WorkflowStateSnapshot => ({
  workflowId,
  workflowName: `Workflow ${workflowId}`,
  status: 'completed',
  inputs: {},
  artifacts: {},
  capturedOutputs: {},
  parentSessionId: 'parent-1',
  root: repeatSnapshotRoot(),
});

const uniqueRowKeys = (nodes: readonly WorkflowMonitorNode[]): number =>
  new Set(nodes.map((node) => `${node.id}:${node.sessionId ?? ''}`)).size;

describe('workflow store', () => {
  it('flattens nested plans without losing render metadata', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(
      startEvent('nested', [
        {
          nodeId: 'sequence',
          type: 'sequence',
          steps: [
            step('build', {
              modelId: 'model-1',
              effortLevel: 'high',
            }),
            {
              nodeId: 'repeat',
              type: 'repeat',
              maxIterations: 3,
              steps: [step('review')],
            },
          ],
        },
      ])
    );

    const workflow = selectActiveWorkflow(store.getState());
    expect(
      workflow?.nodes.map(({ id, parentId, depth }) => ({
        id,
        parentId,
        depth,
      }))
    ).toEqual([
      { id: 'sequence', parentId: null, depth: 0 },
      { id: 'build', parentId: 'sequence', depth: 1 },
      { id: 'repeat', parentId: 'sequence', depth: 1 },
      { id: 'review', parentId: 'repeat', depth: 2 },
    ]);
    expect(workflow?.nodes[1]).toMatchObject({
      modelId: 'model-1',
      effortLevel: 'high',
    });
    expect(workflow?.nodes[2]?.maxIterations).toBe(3);
  });

  it('treats queued workflow steps as supersedable state', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('updated', [step('existing')]));
    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [step('applied')],
    });

    expect(
      store
        .getState()
        .workflows.get('updated')
        ?.nodes.map((node) => node.id)
    ).toEqual(['existing', 'applied']);
    expect(store.getState().workflows.get('updated')?.queuedNodeIds).toEqual([
      'applied',
    ]);

    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [],
      resolution: { outcome: 'applied' },
    });
    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [step('dropped')],
    });

    expect(
      store
        .getState()
        .workflows.get('updated')
        ?.nodes.map((node) => node.id)
    ).toEqual(['existing', 'applied', 'dropped']);

    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [],
      resolution: { outcome: 'dropped' },
    });

    const workflow = store.getState().workflows.get('updated');
    expect(workflow?.nodes.map((node) => node.id)).toEqual([
      'existing',
      'applied',
    ]);
    expect(workflow?.queuedNodeIds).toBeUndefined();
  });

  it('replaces an announced queued plan and retracts a rejected one', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('updated', [step('existing')]));
    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [step('first-plan')],
    });
    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [step('replacement-plan')],
    });

    expect(
      store
        .getState()
        .workflows.get('updated')
        ?.nodes.map((node) => node.id)
    ).toEqual(['existing', 'replacement-plan']);

    store.getState().applyEvent({
      type: 'steps_queued',
      workflowId: 'updated',
      pendingSteps: [],
      resolution: { outcome: 'rejected', reason: 'invalid plan' },
    });

    expect(
      store
        .getState()
        .workflows.get('updated')
        ?.nodes.map((node) => node.id)
    ).toEqual(['existing']);
  });

  it('keeps concurrent workflows and routes lifecycle events by id', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('first'));
    store.getState().applyEvent(startEvent('second'));
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'second',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      nodeType: 'step',
      sessionId: 'session-second',
      agentName: 'coder',
    });

    expect(store.getState().workflows).toHaveLength(2);
    expect(store.getState().activeWorkflowId).toBe('first');
    expect(store.getState().workflows.get('first')?.nodes[0]?.status).toBe(
      'pending'
    );
    expect(store.getState().workflows.get('second')?.nodes[0]).toMatchObject({
      status: 'running',
      sessionId: 'session-second',
    });
    expect(selectLiveWorkflowCount(store.getState())).toBe(2);
  });

  it('preserves selection independently for each workflow', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('first'));
    store.getState().applyEvent(startEvent('second'));
    store.getState().setSelectedNode(1);
    store.getState().setActiveWorkflow('second');
    store.getState().setSelectedNode(0);

    expect(selectWorkflowNodeIndex(store.getState())).toBe(0);
    store.getState().setActiveWorkflow('first');
    expect(selectWorkflowNodeIndex(store.getState())).toBe(1);
  });

  it('records exact child sessions and preserves progress on resume', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('resume'));
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'resume',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      nodeType: 'step',
      sessionId: 'session-one',
      agentName: 'coder',
    });
    store.getState().applyEvent({
      type: 'node_complete',
      workflowId: 'resume',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      status: 'completed',
      sessionId: 'session-one',
      durationSecs: 2,
    });
    store.getState().applyEvent(startEvent('resume'));

    const workflow = store.getState().workflows.get('resume');
    expect(workflow?.nodes[0]).toMatchObject({
      status: 'completed',
      sessionId: 'session-one',
      durationSecs: 2,
    });
    expect(workflow?.stepSessions).toEqual([
      expect.objectContaining({
        nodeId: 'one',
        sessionId: 'session-one',
        status: 'completed',
      }),
    ]);
  });

  it('replaces a retried node session without accepting stale completion', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('retry'));
    for (const sessionId of ['session-old', 'session-new']) {
      store.getState().applyEvent({
        type: 'node_start',
        workflowId: 'retry',
        parentSessionId: 'parent-1',
        nodeId: 'one',
        nodePath: ['root', 'one'],
        nodeType: 'step',
        sessionId,
        agentName: 'coder',
      });
    }

    store.getState().applyEvent({
      type: 'node_complete',
      workflowId: 'retry',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      status: 'failed',
      sessionId: 'session-old',
    });

    const workflow = store.getState().workflows.get('retry');
    expect(workflow?.nodes[0]).toMatchObject({
      status: 'running',
      sessionId: 'session-new',
    });
    expect(workflow?.stepSessions.map((session) => session.sessionId)).toEqual([
      'session-old',
      'session-new',
    ]);
  });

  it('atomically restores a live snapshot with canonical repeat paths', () => {
    const store = createWorkflowStore(() => 500);
    const state: WorkflowStateSnapshot = {
      workflowId: 'restored',
      workflowName: 'Restored workflow',
      status: 'paused',
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      createdAt: 'not-a-date',
      parentSessionId: 'parent-1',
      pauseReason: 'Waiting for review',
      root: {
        nodeId: 'root',
        type: 'repeat',
        status: 'paused',
        children: [
          {
            nodeId: 'review#2',
            type: 'step',
            status: 'paused',
            sessionId: 'session-review-2',
            iteration: 2,
            agentName: 'reviewer',
            completionSignal: 'need_input',
          },
        ],
      },
    };

    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restored',
      parentSessionId: 'parent-1',
      state,
      stepSessions: [],
    });

    const restored = store.getState().workflows.get('restored');
    expect(restored).toMatchObject({
      workflowId: 'restored',
      parentSessionId: 'parent-1',
      name: 'Restored workflow',
      status: 'paused',
      startedAt: 500,
      completedAt: null,
      pauseReason: 'Waiting for review',
    });
    expect(restored?.nodes[1]).toMatchObject({
      id: 'review#2',
      nodePath: ['root', 'iter-2'],
      sessionId: 'session-review-2',
      status: 'paused',
    });
    expect(restored?.stepSessions).toEqual([
      expect.objectContaining({
        nodeId: 'review#2',
        nodePath: ['root', 'iter-2'],
        sessionId: 'session-review-2',
        status: 'paused',
      }),
    ]);
    expect(store.getState().activeWorkflowId).toBe('restored');
    expect(selectWorkflowNodeIndex(store.getState())).toBe(1);
  });

  it('does not move selection while node input owns focus', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('input'));
    store.getState().setSelectedNode(0);
    store.getState().setInputState(true);
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'input',
      parentSessionId: 'parent-1',
      nodeId: 'two',
      nodePath: ['root', 'two'],
      nodeType: 'step',
      sessionId: 'session-two',
    });

    expect(selectWorkflowNodeIndex(store.getState())).toBe(0);
    expect(store.getState().selectionLocked).toBe(true);
  });

  it('reconciles final state and archives terminal runs', () => {
    const store = createWorkflowStore(() => 200);
    store.getState().applyEvent(startEvent('done'));
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'done',
      parentSessionId: 'parent-1',
      status: 'completed',
      finalState: snapshot('done'),
    });

    expect(store.getState().workflows.has('done')).toBe(false);
    expect(store.getState().archivedWorkflows.get('done')).toMatchObject({
      status: 'completed',
      completedAt: 200,
    });
    expect(
      store.getState().archivedWorkflows.get('done')?.stepSessions
    ).toHaveLength(2);
  });

  it('retains a terminal tab until every workflow surface closes', () => {
    const store = createWorkflowStore(() => 200);
    store.getState().applyEvent(startEvent('done'));
    store.getState().setWorkflowSurfaceOpen('monitor', true);
    store.getState().setWorkflowSurfaceOpen('tray', true);
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'done',
      parentSessionId: 'parent-1',
      status: 'completed',
      finalState: snapshot('done'),
    });

    expect(store.getState().workflows.has('done')).toBe(true);
    store.getState().setWorkflowSurfaceOpen('monitor', false);
    expect(store.getState().workflows.has('done')).toBe(true);
    store.getState().setWorkflowSurfaceOpen('tray', false);
    expect(store.getState().workflows.has('done')).toBe(false);
    expect(store.getState().archivedWorkflows.has('done')).toBe(true);
  });

  it('keeps paused runs live and focuses the input node', () => {
    const store = createWorkflowStore(() => 200);
    store.getState().applyEvent(startEvent('paused'));
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'paused',
      parentSessionId: 'parent-1',
      status: 'paused',
      finalState: snapshot('paused', 'paused'),
    });

    expect(store.getState().activeWorkflowId).toBe('paused');
    expect(selectWorkflowNodeIndex(store.getState())).toBe(1);
    expect(store.getState().workflows.get('paused')?.nodes[1]).toMatchObject({
      status: 'paused',
      completionSignal: 'need_input',
    });
  });

  it('reopens archived workflows without discarding their conversations', () => {
    const store = createWorkflowStore(() => 200);
    store.getState().applyEvent(startEvent('history'));
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'history',
      parentSessionId: 'parent-1',
      status: 'completed',
      finalState: snapshot('history'),
    });
    const archived = store.getState().archivedWorkflows.get('history');
    expect(archived).toBeDefined();

    store.getState().openHistoricalWorkflow(archived!);
    const workflow = selectActiveWorkflow(store.getState());
    const conversations = buildWorkflowNodeConversations(workflow!);
    expect(conversations.map((item) => item.target.sessionId)).toEqual([
      'session-one',
      'session-two',
    ]);
    expect(conversations[0]?.nodeStatus).toBe('completed');
  });

  it('owns workflow history state outside the app store', () => {
    const store = createWorkflowStore();
    const run: WorkflowRunSummary = {
      workflowId: 'history',
      name: 'Historical workflow',
      status: 'completed',
      createdAt: '2026-07-19T10:00:00.000Z',
      updatedAt: '2026-07-19T10:01:00.000Z',
      parentSessionId: 'parent-1',
    };

    store.getState().openWorkflowHistory([run]);
    expect(store.getState().history).toEqual({
      isOpen: true,
      runs: [run],
    });

    store.getState().setHistoryRunStatus(run.workflowId, 'paused');
    expect(store.getState().history.runs[0]).toEqual({
      ...run,
      status: 'paused',
    });

    store.getState().closeWorkflowHistory();
    expect(store.getState().history).toEqual({
      isOpen: false,
      runs: [{ ...run, status: 'paused' }],
    });
  });

  it('builds a historical run from runtime state when nodePlan is absent', () => {
    const run: WorkflowRunSummary = {
      workflowId: 'history',
      name: 'Summary name',
      status: 'completed',
      createdAt: '2026-07-19T10:00:00.000Z',
      updatedAt: '2026-07-19T10:01:00.000Z',
      startedAt: '2026-07-19T10:00:05.000Z',
      endedAt: '2026-07-19T10:00:45.000Z',
      parentSessionId: 'parent-1',
    };
    const inspected: WorkflowInspectResponse = {
      workflowId: run.workflowId,
      state: snapshot(run.workflowId),
    };

    const historical = buildHistoricalWorkflowRun(run, inspected);

    expect(historical).toMatchObject({
      workflowId: 'history',
      name: 'Workflow history',
      status: 'completed',
      parentSessionId: 'parent-1',
      startedAt: Date.parse(run.startedAt!),
      completedAt: Date.parse(run.endedAt!),
    });
    expect(
      historical.nodes.map(({ id, parentId, depth, sessionId }) => ({
        id,
        parentId,
        depth,
        sessionId,
      }))
    ).toEqual([
      { id: 'root', parentId: null, depth: 0, sessionId: undefined },
      { id: 'one', parentId: 'root', depth: 1, sessionId: 'session-one' },
      { id: 'two', parentId: 'root', depth: 1, sessionId: 'session-two' },
    ]);
    expect(historical.nodes[1]?.nodePath).toEqual(['root', 'one']);
    expect(historical.stepSessions).toHaveLength(2);
  });

  it('tracks pause intent and clears it when the backend confirms pause', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('pause-request'));
    store.getState().setPauseRequested('pause-request', true);
    expect(
      store.getState().pauseRequestedWorkflowIds.has('pause-request')
    ).toBe(true);

    store.getState().applyEvent({
      type: 'paused',
      workflowId: 'pause-request',
      parentSessionId: 'parent-1',
      pauseReason: 'requested',
    });
    expect(
      store.getState().pauseRequestedWorkflowIds.has('pause-request')
    ).toBe(false);
  });

  it('keeps independent clamped split ratios for both layouts', () => {
    const store = createWorkflowStore();
    store.getState().setMonitorSplitRatio('side-by-side', 0.9);
    store.getState().toggleMonitorLayout();
    store.getState().setMonitorSplitRatio('stacked', 0.1);

    expect(store.getState().monitorLayout).toBe('stacked');
    expect(store.getState().monitorSplitRatios).toEqual({
      'side-by-side': 0.8,
      stacked: 0.2,
    });
  });

  it('projects pending replacement steps returned by inspect', () => {
    const run: WorkflowRunSummary = {
      workflowId: 'history',
      name: 'Summary name',
      status: 'running',
      createdAt: '2026-07-19T10:00:00.000Z',
      updatedAt: '2026-07-19T10:01:00.000Z',
      parentSessionId: 'parent-1',
    };
    const historical = buildHistoricalWorkflowRun(run, {
      workflowId: run.workflowId,
      state: snapshot(run.workflowId, 'running'),
      pendingSteps: [step('queued-review')],
    });

    expect(historical.nodes.at(-1)?.id).toBe('queued-review');
    expect(historical.queuedNodeIds).toEqual(['queued-review']);
  });

  it('derives progress and activity counts without mutating store state', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('running'));
    store.getState().applyEvent(startEvent('paused'));
    store.getState().applyEvent({
      type: 'paused',
      workflowId: 'paused',
      parentSessionId: 'parent-1',
      pauseReason: 'input',
    });
    expect(
      workflowActivitySummary(store.getState().workflows.values())
    ).toEqual({
      running: 1,
      paused: 1,
      completedSteps: 0,
      totalSteps: 4,
    });

    store.getState().applyEvent({
      type: 'node_complete',
      workflowId: 'running',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      status: 'completed',
    });

    expect(
      workflowProgress(store.getState().workflows.get('running')!.nodes)
    ).toEqual({ completed: 1, total: 2 });
    expect(workflowActivityCounts(store.getState().workflows.values())).toEqual(
      { running: 1, paused: 1 }
    );
    expect(
      workflowActivitySummary(store.getState().workflows.values())
    ).toEqual({
      running: 1,
      paused: 1,
      completedSteps: 1,
      totalSteps: 4,
    });
  });

  it('keeps a not-yet-started node pending when node_paused names it', () => {
    // A boundary park names the node KAS is about to start, so flipping it to
    // paused would claim work that never began.
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('boundary'));
    store.getState().applyEvent({
      type: 'node_paused',
      workflowId: 'boundary',
      parentSessionId: 'parent-1',
      nodeId: 'two',
      nodePath: ['root', 'two'],
      reason: 'paused before step two',
    });

    const node = store.getState().workflows.get('boundary')!.nodes[1]!;
    expect(node.status).toBe('pending');
    expect(node.pauseReason).toBe('paused before step two');
  });

  it('keeps a boundary-parked node pending through the run_complete snapshot', () => {
    // A boundary park is three events, not one: node_paused, `paused`, then
    // run_complete carrying a snapshot that records the never-started node as
    // `paused`. Reconciling that verbatim would undo the node_paused guard.
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('boundary-settle'));
    store.getState().applyEvent({
      type: 'node_complete',
      workflowId: 'boundary-settle',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      status: 'completed',
    });
    store.getState().applyEvent({
      type: 'node_paused',
      workflowId: 'boundary-settle',
      parentSessionId: 'parent-1',
      nodeId: 'two',
      nodePath: ['root', 'two'],
      reason: "Workflow paused before node 'two'.",
    });
    store.getState().applyEvent({
      type: 'paused',
      workflowId: 'boundary-settle',
      parentSessionId: 'parent-1',
      pauseReason: "Workflow paused before node 'two'.",
      initiator: 'user',
    });
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'boundary-settle',
      parentSessionId: 'parent-1',
      status: 'paused',
      finalState: {
        workflowId: 'boundary-settle',
        workflowName: 'Workflow boundary-settle',
        status: 'paused',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        pauseReason: "Workflow paused before node 'two'.",
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'running',
          children: [
            {
              nodeId: 'one',
              type: 'step',
              status: 'completed',
              sessionId: 'session-one',
              startedAt: '2026-08-10T00:00:00.000Z',
              endedAt: '2026-08-10T00:00:01.000Z',
            },
            // No `startedAt` — the park happened before this node ran.
            { nodeId: 'two', type: 'step', status: 'paused' },
          ],
        },
      },
    });

    const nodes = store.getState().workflows.get('boundary-settle')!.nodes;
    expect(nodes[0]!.status).toBe('completed');
    expect(nodes[1]!.status).toBe('pending');
    expect(nodes[1]!.pauseReason).toBe("Workflow paused before node 'two'.");
  });

  it('adopts paused from the snapshot for a node that had started', () => {
    // The mirror case: a mid-flight park did run, so its `paused` is authoritative.
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('mid-settle'));
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'mid-settle',
      parentSessionId: 'parent-1',
      nodeId: 'two',
      nodePath: ['root', 'two'],
      nodeType: 'step',
      sessionId: 'session-two',
    });
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'mid-settle',
      parentSessionId: 'parent-1',
      status: 'paused',
      finalState: {
        workflowId: 'mid-settle',
        workflowName: 'Workflow mid-settle',
        status: 'paused',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'running',
          children: [
            { nodeId: 'one', type: 'step', status: 'completed' },
            {
              nodeId: 'two',
              type: 'step',
              status: 'paused',
              sessionId: 'session-two',
              startedAt: '2026-08-10T00:00:00.000Z',
            },
          ],
        },
      },
    });

    const nodes = store.getState().workflows.get('mid-settle')!.nodes;
    expect(nodes[1]!.status).toBe('paused');
  });

  it('marks a started node paused when node_paused names it', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('mid-step'));
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'mid-step',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      nodeType: 'step',
    });
    store.getState().applyEvent({
      type: 'node_paused',
      workflowId: 'mid-step',
      parentSessionId: 'parent-1',
      nodeId: 'one',
      nodePath: ['root', 'one'],
      reason: 'awaiting answer',
    });

    expect(store.getState().workflows.get('mid-step')!.nodes[0]).toMatchObject({
      status: 'paused',
      pauseReason: 'awaiting answer',
    });
  });

  it('records who stopped the run so the banner can attribute it', () => {
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('attributed'));
    store.getState().applyEvent({
      type: 'paused',
      workflowId: 'attributed',
      parentSessionId: 'parent-1',
      pauseReason: 'paused by request',
      initiator: 'user',
      initiatorReason: 'switching branches',
    });

    expect(store.getState().workflows.get('attributed')).toMatchObject({
      stopInitiator: 'user',
      stopReason: 'switching branches',
    });

    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'attributed',
      parentSessionId: 'parent-1',
      status: 'aborted',
      finalState: snapshot('attributed', 'aborted'),
    });

    // The terminal event carries no attribution, so the pause's has to survive.
    expect(store.getState().archivedWorkflows.get('attributed')).toMatchObject({
      status: 'aborted',
      stopInitiator: 'user',
      stopReason: 'switching branches',
    });
  });

  it('carries stop attribution through a session restore', () => {
    // A restored run keeps its "Stopped by you." banner rather than regressing to
    // the mechanical pause reason the attribution exists to replace.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restored-stop',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restored-stop', 'paused'),
        pauseReason: "Workflow paused before node 'two'.",
        stopInitiator: 'user',
        stopReason: 'switching branches',
      },
      stepSessions: [],
    });

    expect(store.getState().workflows.get('restored-stop')).toMatchObject({
      status: 'paused',
      stopInitiator: 'user',
      stopReason: 'switching branches',
    });
  });

  it('drops pause attribution once a step runs again', () => {
    // Without clearing the stale stop, a later autonomous failure inherits
    // `'user'` and reads "Stopped by you." on a run the user did not stop.
    const store = createWorkflowStore();
    store.getState().applyEvent(startEvent('resumed'));
    store.getState().applyEvent({
      type: 'paused',
      workflowId: 'resumed',
      parentSessionId: 'parent-1',
      pauseReason: 'paused by request',
      initiator: 'user',
      initiatorReason: 'switching branches',
    });
    store.getState().applyEvent({
      type: 'node_start',
      workflowId: 'resumed',
      parentSessionId: 'parent-1',
      nodeId: 'two',
      nodePath: ['root', 'two'],
      nodeType: 'step',
      sessionId: 'session-two',
    });

    expect(store.getState().workflows.get('resumed')).toMatchObject({
      status: 'running',
      stopInitiator: undefined,
      stopReason: undefined,
      pauseReason: undefined,
    });

    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'resumed',
      parentSessionId: 'parent-1',
      status: 'failed',
      finalState: snapshot('resumed', 'failed'),
    });

    expect(store.getState().archivedWorkflows.get('resumed')).toMatchObject({
      status: 'failed',
      stopInitiator: undefined,
    });
  });

  it('parks only the matching loop iteration, not a pending sibling', () => {
    // `node_paused` carries no `iteration`, so an unrolled repeat matches more
    // than one row: decided per node, or one pending sibling suppresses the park
    // on the instance that actually stopped.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'loop-park',
      parentSessionId: 'parent-1',
      state: {
        workflowId: 'loop-park',
        workflowName: 'Loop park',
        status: 'running',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        root: {
          nodeId: 'root',
          type: 'repeat',
          status: 'running',
          children: [
            {
              nodeId: 'review',
              type: 'step',
              status: 'running',
              iteration: 1,
              sessionId: 'session-review-1',
            },
            { nodeId: 'review', type: 'step', status: 'pending', iteration: 2 },
          ],
        },
      },
      stepSessions: [],
    });
    store.getState().applyEvent({
      type: 'node_paused',
      workflowId: 'loop-park',
      parentSessionId: 'parent-1',
      nodeId: 'review',
      nodePath: ['root', 'iter-1'],
      reason: 'awaiting answer',
    });

    const nodes = store.getState().workflows.get('loop-park')!.nodes;
    const parked = nodes.find((node) => node.iteration === 1);
    const sibling = nodes.find((node) => node.iteration === 2);
    expect(parked).toMatchObject({
      status: 'paused',
      pauseReason: 'awaiting answer',
    });
    expect(sibling?.status).toBe('pending');
  });

  it('keeps a boundary-parked step pending when a restore carries no plan', () => {
    // `nodePlan` is optional, and without it nodes come from `flattenState`, which
    // copies the snapshot status verbatim — so this path needs the same guard
    // `reconcileNodes` applies or the phantom-paused step is back on screen.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restore-park',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restore-park', 'paused'),
        pauseReason: "Workflow paused before node 'two'.",
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'paused',
          children: [
            {
              nodeId: 'one',
              type: 'step',
              status: 'completed',
              startedAt: '2026-08-10T00:00:00.000Z',
              sessionId: 'session-one',
            },
            // Parked before it ever ran: no startedAt, no sessionId.
            { nodeId: 'two', type: 'step', status: 'paused' },
          ],
        },
      },
      stepSessions: [],
    });

    const nodes = store.getState().workflows.get('restore-park')!.nodes;
    expect(nodes.find((node) => node.id === 'two')?.status).toBe('pending');
    // A step that genuinely parked mid-flight still restores as paused.
    expect(nodes.find((node) => node.id === 'one')?.status).toBe('completed');
  });

  it('restores a parked step with its question when only the run carries one', () => {
    // `WorkflowNodeState` has no `pauseReason`, so the run-level one is all a
    // reload can offer — without it `s respond` outlives its question.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restore-question',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restore-question', 'paused'),
        pauseReason: 'Ship the release candidate?',
      },
      stepSessions: [],
    });

    const parked = store
      .getState()
      .workflows.get('restore-question')!
      .nodes.find((node) => node.id === 'two');
    expect(parked).toMatchObject({
      status: 'paused',
      pauseReason: 'Ship the release candidate?',
    });
  });

  it('leaves a parked container at the status the snapshot gives it', () => {
    // A container never owns a step session, so the guard's `!sessionId` half is
    // free for one — without the step filter a paused `parallel` is dragged back
    // to `pending`, above children that have already finished.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restore-container',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restore-container', 'paused'),
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'paused',
          children: [
            {
              nodeId: 'group',
              type: 'parallel',
              status: 'paused',
              children: [
                {
                  nodeId: 'one',
                  type: 'step',
                  status: 'completed',
                  startedAt: '2026-08-10T00:00:00.000Z',
                  sessionId: 'session-one',
                },
              ],
            },
          ],
        },
      },
      stepSessions: [],
    });

    const nodes = store.getState().workflows.get('restore-container')!.nodes;
    expect(nodes.find((node) => node.id === 'root')?.status).toBe('paused');
    expect(nodes.find((node) => node.id === 'group')?.status).toBe('paused');
    expect(nodes.find((node) => node.id === 'one')?.status).toBe('completed');
  });

  it('does not repeat one run-level question across every parked node', () => {
    // One reason can describe only one park: fanning it onto two branches claims
    // both wait on the same question, and onto a container it offers a reply the
    // composer refuses for want of a conversation.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restore-fanout',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restore-fanout', 'paused'),
        pauseReason: 'Ship the release candidate?',
        root: {
          nodeId: 'root',
          type: 'parallel',
          status: 'paused',
          startedAt: '2026-08-10T00:00:00.000Z',
          children: [
            {
              nodeId: 'left',
              type: 'step',
              status: 'paused',
              startedAt: '2026-08-10T00:00:00.000Z',
              sessionId: 'session-left',
            },
            {
              nodeId: 'right',
              type: 'step',
              status: 'paused',
              startedAt: '2026-08-10T00:00:00.000Z',
              sessionId: 'session-right',
            },
          ],
        },
      },
      stepSessions: [],
    });

    const nodes = store.getState().workflows.get('restore-fanout')!.nodes;
    // Ambiguous which branch the reason belongs to, so neither claims it.
    expect(
      nodes.find((node) => node.id === 'left')?.pauseReason
    ).toBeUndefined();
    expect(
      nodes.find((node) => node.id === 'right')?.pauseReason
    ).toBeUndefined();
    // A container could never answer it in the first place.
    expect(
      nodes.find((node) => node.id === 'root')?.pauseReason
    ).toBeUndefined();
  });

  it('gives the run-level question only to a parked step that can answer it', () => {
    // The unambiguous case: one paused step owning a session, so the composer has
    // somewhere to send the answer.
    const store = createWorkflowStore();
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'restore-container-only',
      parentSessionId: 'parent-1',
      state: {
        ...snapshot('restore-container-only', 'paused'),
        pauseReason: 'Ship the release candidate?',
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'paused',
          startedAt: '2026-08-10T00:00:00.000Z',
          children: [
            {
              nodeId: 'only',
              type: 'step',
              status: 'paused',
              startedAt: '2026-08-10T00:00:00.000Z',
              sessionId: 'session-only',
            },
          ],
        },
      },
      stepSessions: [],
    });

    const nodes = store
      .getState()
      .workflows.get('restore-container-only')!.nodes;
    expect(nodes.find((node) => node.id === 'only')?.pauseReason).toBe(
      'Ship the release candidate?'
    );
    expect(
      nodes.find((node) => node.id === 'root')?.pauseReason
    ).toBeUndefined();
  });

  it('gives every started repeat iteration its own row and session', () => {
    const store = startedIterations('loop-rows', 2);

    const workflow = store.getState().workflows.get('loop-rows')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual([
      'loop',
      'review',
      'review',
    ]);
    expect(workflow.nodes[1]).toMatchObject({
      id: 'review',
      type: 'step',
      status: 'running',
      label: 'review',
      parentId: 'loop',
      depth: 1,
      iteration: 0,
      sessionId: 'session-0',
      nodePath: ['root', 'loop', 'iter-0', 'review'],
      agentName: 'reviewer',
      maxIterations: 3,
    });
    expect(workflow.nodes[2]).toMatchObject({
      id: 'review',
      type: 'step',
      status: 'running',
      label: 'review',
      parentId: 'loop',
      depth: 1,
      iteration: 1,
      sessionId: 'session-1',
      nodePath: ['root', 'loop', 'iter-1', 'review'],
      agentName: 'reviewer',
      maxIterations: 3,
    });
    expect(workflow.stepSessions.map((session) => session.sessionId)).toEqual([
      'session-0',
      'session-1',
    ]);
  });

  it('keeps appended iterations beside their own sibling group', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(
      startEvent('loop-order', [
        {
          nodeId: 'loop',
          type: 'repeat',
          maxIterations: 2,
          steps: [step('build'), step('review')],
        },
      ])
    );
    for (const iteration of [0, 1]) {
      for (const nodeId of ['build', 'review']) {
        store.getState().applyEvent(bodyStart('loop-order', nodeId, iteration));
      }
    }

    expect(
      store
        .getState()
        .workflows.get('loop-order')!
        .nodes.map((node) => `${node.id}:${node.iteration ?? '-'}`)
    ).toEqual(['loop:-', 'build:0', 'build:1', 'review:0', 'review:1']);
  });

  it('selects the repeat iteration that actually started', () => {
    const store = startedIterations('loop-select', 2);

    const nodes = store.getState().workflows.get('loop-select')!.nodes;
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBe(2);
    expect(nodes[selected]).toMatchObject({
      iteration: 1,
      sessionId: 'session-1',
    });
  });

  it('does not move selection to a new iteration while input owns focus', () => {
    const store = startedIterations('loop-locked', 1);
    store.getState().setSelectedNode(1);
    store.getState().setInputState(true);
    store.getState().applyEvent(iterationStart('loop-locked', 1));

    expect(selectWorkflowNodeIndex(store.getState())).toBe(1);
    expect(store.getState().workflows.get('loop-locked')!.nodes).toHaveLength(
      3
    );
  });

  it('keeps a locked selection on its own row when an earlier sibling appends', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(
      startEvent('loop-lock-shift', [
        {
          nodeId: 'loop',
          type: 'repeat',
          maxIterations: 2,
          steps: [step('build'), step('review')],
        },
      ])
    );
    for (const nodeId of ['build', 'review']) {
      store.getState().applyEvent(bodyStart('loop-lock-shift', nodeId, 0));
    }
    // review:0 owns the composer, and build:1 inserts ahead of it.
    store.getState().setSelectedNode(2);
    store.getState().setInputState(true);
    store.getState().applyEvent(bodyStart('loop-lock-shift', 'build', 1));

    const nodes = store.getState().workflows.get('loop-lock-shift')!.nodes;
    expect(nodes.map((node) => `${node.id}:${node.iteration ?? '-'}`)).toEqual([
      'loop:-',
      'build:0',
      'build:1',
      'review:0',
    ]);
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBe(3);
    expect(nodes[selected]).toMatchObject({
      id: 'review',
      iteration: 0,
      sessionId: 'review-0',
    });
  });

  it('keeps one row for a container that iterates inside a repeat body', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(
      startEvent('loop-nested', [
        {
          nodeId: 'loop',
          type: 'repeat',
          maxIterations: 2,
          steps: [
            {
              nodeId: 'fan',
              type: 'parallel',
              branches: [step('build')],
            },
          ],
        },
      ])
    );
    for (const iteration of [0, 1]) {
      store.getState().applyEvent({
        type: 'node_start',
        workflowId: 'loop-nested',
        parentSessionId: 'parent-1',
        nodeId: 'fan',
        nodePath: ['root', 'loop', `iter-${iteration}`, 'fan'],
        nodeType: 'parallel',
        iteration,
      });
      if (iteration === 1) {
        const selected = selectWorkflowNodeIndex(store.getState());
        expect(selected).toBe(2);
        expect(
          store.getState().workflows.get('loop-nested')!.nodes[selected]
        ).toBeDefined();
      }
      store
        .getState()
        .applyEvent(bodyStart('loop-nested', 'build', iteration, ['fan']));
    }

    const nodes = store.getState().workflows.get('loop-nested')!.nodes;
    expect(
      nodes.map(({ id, parentId, depth, iteration }) => ({
        id,
        parentId,
        depth,
        iteration,
      }))
    ).toEqual([
      { id: 'loop', parentId: null, depth: 0, iteration: undefined },
      { id: 'fan', parentId: 'loop', depth: 1, iteration: 0 },
      { id: 'build', parentId: 'fan', depth: 2, iteration: 0 },
      { id: 'build', parentId: 'fan', depth: 2, iteration: 1 },
    ]);
  });

  it('records a watch outcome against the polled instance only', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'watch-rows',
      parentSessionId: 'parent-1',
      state: {
        workflowId: 'watch-rows',
        workflowName: 'Workflow watch-rows',
        status: 'running',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'running',
          children: [
            {
              nodeId: 'loop',
              type: 'repeat',
              status: 'running',
              iteration: 1,
              children: [0, 1].map((iteration) => ({
                nodeId: 'inbox',
                type: 'watch',
                status: 'running',
                iteration,
              })),
            },
          ],
        },
      },
      stepSessions: [],
    });
    store.getState().applyEvent({
      type: 'watch_poll',
      workflowId: 'watch-rows',
      parentSessionId: 'parent-1',
      nodeId: 'inbox',
      nodePath: ['root', 'loop', 'iter-1'],
      outcome: 'new-activity',
      at: '2026-08-10T00:00:00.000Z',
    });

    const watches = store
      .getState()
      .workflows.get('watch-rows')!
      .nodes.filter((node) => node.id === 'inbox');
    expect(watches).toHaveLength(2);
    expect(watches.map((node) => node.watchOutcome)).toEqual([
      undefined,
      'new-activity',
    ]);
  });

  it('records a watch outcome on a repeat-nested watch past the first iteration', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(
      startEvent('watch-loop', [
        {
          nodeId: 'loop',
          type: 'repeat',
          maxIterations: 3,
          steps: [{ nodeId: 'inbox', type: 'watch' }],
        },
      ])
    );
    for (const iteration of [0, 1]) {
      store.getState().applyEvent({
        type: 'node_start',
        workflowId: 'watch-loop',
        parentSessionId: 'parent-1',
        nodeId: 'inbox',
        nodePath: ['root', 'loop', `iter-${iteration}`],
        nodeType: 'watch',
        iteration,
      });
    }
    store.getState().applyEvent({
      type: 'watch_poll',
      workflowId: 'watch-loop',
      parentSessionId: 'parent-1',
      nodeId: 'inbox',
      nodePath: ['root', 'loop', 'iter-1'],
      outcome: 'new-activity',
      at: '2026-08-10T00:00:00.000Z',
    });

    const watches = store
      .getState()
      .workflows.get('watch-loop')!
      .nodes.filter((node) => node.id === 'inbox');
    expect(watches).toHaveLength(1);
    expect(watches[0]!.watchOutcome).toBe('new-activity');
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBe(1);
    expect(
      store.getState().workflows.get('watch-loop')!.nodes[selected]
    ).toBeDefined();
  });

  it('binds a relaxed watch outcome to the newest unrolled instance only', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'watch-ahead',
      parentSessionId: 'parent-1',
      state: {
        workflowId: 'watch-ahead',
        workflowName: 'Workflow watch-ahead',
        status: 'running',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'running',
          children: [
            {
              nodeId: 'loop',
              type: 'repeat',
              status: 'running',
              iteration: 1,
              children: [0, 1].map((iteration) => ({
                nodeId: 'inbox',
                type: 'watch',
                status: 'running',
                iteration,
              })),
            },
          ],
        },
      },
      stepSessions: [],
    });
    // An iteration the snapshot has no row for: the poll must not stamp both.
    store.getState().applyEvent({
      type: 'watch_poll',
      workflowId: 'watch-ahead',
      parentSessionId: 'parent-1',
      nodeId: 'inbox',
      nodePath: ['root', 'loop', 'iter-2'],
      outcome: 'new-activity',
      at: '2026-08-10T00:00:00.000Z',
    });

    const watches = store
      .getState()
      .workflows.get('watch-ahead')!
      .nodes.filter((node) => node.id === 'inbox');
    expect(watches.map((node) => node.watchOutcome)).toEqual([
      undefined,
      'new-activity',
    ]);
  });

  it('settles each repeat iteration on its own completion', () => {
    const store = startedIterations('loop-complete', 2);
    store.getState().applyEvent(iterationComplete('loop-complete', 0));

    let nodes = store.getState().workflows.get('loop-complete')!.nodes;
    expect(nodes[1]).toMatchObject({
      iteration: 0,
      sessionId: 'session-0',
      status: 'completed',
      durationSecs: 1,
    });
    expect(nodes[2]).toMatchObject({
      iteration: 1,
      sessionId: 'session-1',
      status: 'running',
    });
    expect(nodes[2]?.durationSecs).toBeUndefined();

    store
      .getState()
      .applyEvent(iterationComplete('loop-complete', 1, 'failed'));

    const workflow = store.getState().workflows.get('loop-complete')!;
    nodes = workflow.nodes;
    expect(nodes[1]).toMatchObject({
      status: 'completed',
      durationSecs: 1,
    });
    expect(nodes[2]).toMatchObject({
      status: 'failed',
      durationSecs: 2,
    });
    expect(
      workflow.stepSessions.map(({ sessionId, status }) => ({
        sessionId,
        status,
      }))
    ).toEqual([
      { sessionId: 'session-0', status: 'completed' },
      { sessionId: 'session-1', status: 'failed' },
    ]);
  });

  it('parks only the repeat iteration that node_paused names', () => {
    const store = startedIterations('loop-pause', 2);
    store.getState().applyEvent({
      type: 'node_paused',
      workflowId: 'loop-pause',
      parentSessionId: 'parent-1',
      nodeId: 'review',
      nodePath: iterationPath(1),
      sessionId: 'session-1',
      reason: 'awaiting answer',
    });

    const nodes = store.getState().workflows.get('loop-pause')!.nodes;
    expect(nodes[1]).toMatchObject({
      sessionId: 'session-0',
      status: 'running',
    });
    expect(nodes[1]?.pauseReason).toBeUndefined();
    expect(nodes[2]).toMatchObject({
      sessionId: 'session-1',
      status: 'paused',
      pauseReason: 'awaiting answer',
    });
  });

  it('reconciles live iteration rows without duplicating them', () => {
    const store = startedIterations('loop-archive', 2);
    store.getState().applyEvent({
      type: 'run_complete',
      workflowId: 'loop-archive',
      parentSessionId: 'parent-1',
      status: 'completed',
      finalState: repeatSnapshot('loop-archive'),
    });

    const archived = store.getState().archivedWorkflows.get('loop-archive')!;
    expect(archived.nodes).toHaveLength(3);
    const reviews = archived.nodes.filter((node) => node.id === 'review');
    expect(
      reviews.map(({ sessionId, status, iteration }) => ({
        sessionId,
        status,
        iteration,
      }))
    ).toEqual([
      { sessionId: 'session-0', status: 'completed', iteration: 0 },
      { sessionId: 'session-1', status: 'completed', iteration: 1 },
    ]);
    expect(archived.stepSessions).toHaveLength(2);
  });

  it('rebuilds the plan shape when a snapshot lands on live iteration rows', () => {
    const store = startedIterations('loop-snapshot', 2);
    store.getState().setSelectedNode(2);
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'loop-snapshot',
      parentSessionId: 'parent-1',
      state: repeatSnapshot('loop-snapshot'),
      stepSessions: [],
      nodePlan: repeatPlan(),
    });

    const workflow = store.getState().workflows.get('loop-snapshot')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual(['loop', 'review']);
    expect(workflow.nodes[1]?.sessionId).toBe('session-1');
    expect(uniqueRowKeys(workflow.nodes)).toBe(workflow.nodes.length);
    // A retained index from the longer live list must not point past the end.
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBeLessThanOrEqual(1);
    expect(workflow.nodes[selected]).toBeDefined();
  });

  it('carries the newest iteration and clamps selection on run replay', () => {
    const store = startedIterations('loop-replay', 2);
    store.getState().setSelectedNode(2);
    store.getState().applyEvent(startEvent('loop-replay', repeatPlan()));

    const workflow = store.getState().workflows.get('loop-replay')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual(['loop', 'review']);
    expect(workflow.nodes[1]).toMatchObject({
      status: 'running',
      sessionId: 'session-1',
      iteration: 1,
    });
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBe(1);
    expect(workflow.nodes[selected]).toBeDefined();
  });

  it('keeps locked input on its row when a rebuild leaves the index in range', () => {
    const plan = [...repeatPlan(), step('publish')];
    for (const rebuild of ['run_start', 'run_snapshot'] as const) {
      const workflowId = `loop-retain-${rebuild}`;
      const store = createWorkflowStore(() => 100);
      store.getState().applyEvent(startEvent(workflowId, plan));
      for (const iteration of [0, 1]) {
        store.getState().applyEvent(iterationStart(workflowId, iteration));
      }
      store.getState().setSelectedNode(2);
      store.getState().setInputState(true);

      if (rebuild === 'run_start') {
        store.getState().applyEvent(startEvent(workflowId, plan));
      } else {
        const root = repeatSnapshotRoot('running');
        store.getState().applyEvent({
          type: 'run_snapshot',
          workflowId,
          parentSessionId: 'parent-1',
          state: {
            ...repeatSnapshot(workflowId),
            status: 'running',
            root: {
              ...root,
              children: [
                ...root.children!,
                { nodeId: 'publish', type: 'step', status: 'pending' },
              ],
            },
          },
          stepSessions: [],
          nodePlan: plan,
        });
      }

      const workflow = store.getState().workflows.get(workflowId)!;
      expect(workflow.nodes.map((node) => node.id)).toEqual([
        'loop',
        'review',
        'publish',
      ]);
      const selected = selectWorkflowNodeIndex(store.getState());
      expect(selected).toBe(1);
      expect(workflow.nodes[selected]).toMatchObject({
        id: 'review',
        sessionId: 'session-1',
        iteration: 1,
      });
    }
  });

  it('retains a structural selection when a rebuild moves its index', () => {
    const store = createWorkflowStore(() => 100);
    const plan: WorkflowNodeDescriptor[] = [
      {
        nodeId: 'loop',
        type: 'repeat',
        maxIterations: 2,
        steps: [step('review')],
      },
      step('publish'),
    ];
    store.getState().applyEvent(startEvent('loop-clamp', plan));
    for (const iteration of [0, 1]) {
      store.getState().applyEvent(iterationStart('loop-clamp', iteration));
    }
    // The trailing plan row is selected, and the rebuild moves it left.
    store.getState().setSelectedNode(3);
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'loop-clamp',
      parentSessionId: 'parent-1',
      state: {
        workflowId: 'loop-clamp',
        workflowName: 'Workflow loop-clamp',
        status: 'paused',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'parent-1',
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'paused',
          children: [
            {
              nodeId: 'loop',
              type: 'repeat',
              status: 'paused',
              iteration: 1,
              children: [
                {
                  nodeId: 'review',
                  type: 'step',
                  status: 'paused',
                  iteration: 1,
                  sessionId: 'session-1',
                  startedAt: '2026-08-10T00:00:00.000Z',
                  completionSignal: 'need_input',
                },
              ],
            },
            { nodeId: 'publish', type: 'step', status: 'pending' },
          ],
        },
      },
      stepSessions: [],
      nodePlan: plan,
    });

    const workflow = store.getState().workflows.get('loop-clamp')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual([
      'loop',
      'review',
      'publish',
    ]);
    const selected = selectWorkflowNodeIndex(store.getState());
    expect(selected).toBe(2);
    expect(workflow.nodes[selected]).toMatchObject({
      id: 'publish',
      status: 'pending',
    });
  });

  it('unrolls one row per state iteration when a snapshot carries no plan', () => {
    const store = startedIterations('loop-unroll', 2);
    store.getState().applyEvent({
      type: 'run_snapshot',
      workflowId: 'loop-unroll',
      parentSessionId: 'parent-1',
      state: repeatSnapshot('loop-unroll'),
      stepSessions: [],
    });

    const workflow = store.getState().workflows.get('loop-unroll')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual([
      'root',
      'loop',
      'loop#0',
      'review',
      'loop#1',
      'review',
    ]);
    expect(
      workflow.nodes
        .filter((node) => node.id === 'review')
        .map((node) => node.sessionId)
    ).toEqual(['session-0', 'session-1']);
    expect(uniqueRowKeys(workflow.nodes)).toBe(workflow.nodes.length);
  });

  it('rebinds a restarted non-loop step instead of appending a row', () => {
    const store = createWorkflowStore(() => 100);
    store.getState().applyEvent(startEvent('plain'));
    for (const sessionId of ['session-first', 'session-second']) {
      store.getState().applyEvent({
        type: 'node_start',
        workflowId: 'plain',
        parentSessionId: 'parent-1',
        nodeId: 'two',
        nodePath: ['root', 'two'],
        nodeType: 'step',
        sessionId,
      });
    }

    const workflow = store.getState().workflows.get('plain')!;
    expect(workflow.nodes.map((node) => node.id)).toEqual(['one', 'two']);
    expect(workflow.nodes[1]).toMatchObject({
      status: 'running',
      sessionId: 'session-second',
      iteration: undefined,
      maxIterations: undefined,
    });
    expect(selectWorkflowNodeIndex(store.getState())).toBe(1);
  });
});
