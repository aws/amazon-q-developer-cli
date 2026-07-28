import { describe, expect, it } from 'bun:test';
import type {
  WorkflowEvent,
  WorkflowNodeDescriptor,
  WorkflowStateSnapshot,
} from '../../types/workflow.js';
import {
  createWorkflowStore,
  selectActiveWorkflow,
  selectLiveWorkflowCount,
  selectWorkflowNodeIndex,
} from '../workflow-store.js';
import {
  buildWorkflowNodeConversations,
  workflowActivityCounts,
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
  });
});
