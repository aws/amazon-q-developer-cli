import { describe, expect, it } from 'bun:test';
import type {
  WorkflowEvent,
  WorkflowNodeDescriptor,
  WorkflowRunSnapshotEvent,
  WorkflowStateSnapshot,
} from '../types/workflow.js';
import {
  WorkflowTelemetryTracker,
  classifyWorkflowTopology,
  workflowDimensions,
  workflowStepBucket,
} from './workflow-telemetry.js';

const step = (nodeId: string): WorkflowNodeDescriptor => ({
  nodeId,
  type: 'step',
});

function runStart(
  workflowId: string,
  nodeTree: WorkflowNodeDescriptor[] = [step('build')]
): Extract<WorkflowEvent, { type: 'run_start' }> {
  return {
    type: 'run_start',
    workflowId,
    workflowName: 'private-name',
    inputs: { secret: 'private-input' },
    nodeTree,
  };
}

function nodeStart(
  workflowId: string,
  nodeId = 'build'
): Extract<WorkflowEvent, { type: 'node_start' }> {
  return {
    type: 'node_start',
    workflowId,
    nodeId,
    nodePath: ['root', nodeId],
    nodeType: 'step',
    sessionId: `${nodeId}-session`,
  };
}

function nodeComplete(
  workflowId: string,
  nodeId = 'build',
  durationSecs = 7
): Extract<WorkflowEvent, { type: 'node_complete' }> {
  return {
    type: 'node_complete',
    workflowId,
    nodeId,
    nodePath: ['root', nodeId],
    sessionId: `${nodeId}-session`,
    status: 'completed',
    durationSecs,
    capturedOutput: 'private-output',
  };
}

function snapshot(
  workflowId: string,
  status: WorkflowStateSnapshot['status'] = 'completed'
): WorkflowStateSnapshot {
  return {
    workflowId,
    workflowName: 'private-name',
    status,
    inputs: {},
    artifacts: {},
    capturedOutputs: {},
    root: {
      nodeId: 'root',
      type: 'sequence',
      status,
      startedAt: '2026-07-29T10:00:00.000Z',
      endedAt: '2026-07-29T10:00:45.000Z',
      children: [
        {
          nodeId: 'build',
          type: 'step',
          status,
          sessionId: 'build-session',
        },
      ],
    },
  };
}

function terminal(
  workflowId: string,
  status: 'completed' | 'failed' | 'aborted' = 'completed'
): Extract<WorkflowEvent, { type: 'run_complete' }> {
  return {
    type: 'run_complete',
    workflowId,
    status,
    finalState: snapshot(workflowId, status),
  };
}

describe('workflow topology and declared step bucketing', () => {
  it('classifies static constructs without multiplying iterative steps', () => {
    expect(classifyWorkflowTopology([step('one')])).toBe('sequential');
    expect(
      classifyWorkflowTopology([
        {
          nodeId: 'fanout',
          type: 'parallel',
          branches: [step('one'), step('two')],
        },
      ])
    ).toBe('parallel');
    expect(
      classifyWorkflowTopology([
        {
          nodeId: 'loop',
          type: 'repeat',
          maxIterations: 200,
          steps: [step('one')],
        },
      ])
    ).toBe('iterative');
    expect(classifyWorkflowTopology([{ nodeId: 'poll', type: 'watch' }])).toBe(
      'watch'
    );
    expect(
      classifyWorkflowTopology([
        {
          nodeId: 'loop',
          type: 'repeat',
          steps: [{ nodeId: 'poll', type: 'watch' }],
        },
      ])
    ).toBe('mixed');
    expect(classifyWorkflowTopology([])).toBe('_other_');
  });

  it('counts executable step and watch leaves once', () => {
    const dimensions = workflowDimensions([
      {
        nodeId: 'root',
        type: 'sequence',
        steps: [
          step('one'),
          {
            nodeId: 'fanout',
            type: 'parallel',
            branches: [
              step('two'),
              step('three'),
              { nodeId: 'poll', type: 'watch' },
            ],
          },
        ],
      },
    ]);
    expect(dimensions).toEqual({ topology: 'mixed', stepBucket: '3_5' });
  });

  it.each([
    [undefined, '_other_'],
    [0, '_other_'],
    [1, '1'],
    [2, '2'],
    [3, '3_5'],
    [5, '3_5'],
    [6, '6_10'],
    [10, '6_10'],
    [11, '11_plus'],
  ] as const)('buckets %s as %s', (count, expected) => {
    expect(workflowStepBucket(count)).toBe(expected);
  });
});

describe('WorkflowTelemetryTracker', () => {
  it('deduplicates lifecycle notifications and permits a retried node instance', () => {
    const tracker = new WorkflowTelemetryTracker();
    expect(tracker.observe(runStart('wf'), true)).toEqual([
      {
        type: 'run',
        event: 'started',
        topology: 'sequential',
        stepBucket: '1',
      },
      { type: 'concurrent', activeRuns: 1 },
    ]);
    expect(tracker.observe(runStart('wf'), true)).toEqual([]);

    expect(tracker.observe(nodeStart('wf'), true)).toEqual([]);
    const firstCompletion = tracker.observe(nodeComplete('wf'), true);
    expect(firstCompletion).toEqual([
      {
        type: 'node',
        nodeType: 'step',
        outcome: 'completed',
      },
      {
        type: 'node_duration',
        durationSeconds: 7,
        nodeType: 'step',
        outcome: 'completed',
      },
    ]);
    expect(tracker.observe(nodeComplete('wf'), true)).toEqual([]);

    expect(tracker.observe(nodeStart('wf'), true)).toEqual([]);
    expect(tracker.observe(nodeComplete('wf'), true)).toEqual(firstCompletion);
  });

  it('hydrates restored metadata while suppressing replayed starts and nodes', () => {
    const tracker = new WorkflowTelemetryTracker();
    const restored: WorkflowRunSnapshotEvent = {
      type: 'run_snapshot',
      workflowId: 'wf',
      parentSessionId: 'parent',
      state: {
        ...snapshot('wf', 'running'),
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'running',
          children: [
            {
              nodeId: 'done',
              type: 'step',
              status: 'completed',
              sessionId: 'done-session',
            },
            {
              nodeId: 'live',
              type: 'step',
              status: 'running',
              sessionId: 'live-session',
            },
          ],
        },
      },
      stepSessions: [],
      nodePlan: [step('done'), step('live')],
    };

    expect(tracker.observe(restored, false)).toEqual([]);
    expect(tracker.activeRunCount()).toBe(1);
    expect(
      tracker.observe(runStart('wf', [step('done'), step('live')]), false)
    ).toEqual([]);
    expect(
      tracker.observe(
        {
          ...nodeComplete('wf', 'done'),
          sessionId: 'done-session',
        },
        true
      )
    ).toEqual([]);
    expect(
      tracker.observe(
        {
          ...nodeComplete('wf', 'live'),
          sessionId: 'live-session',
        },
        true
      )
    ).toEqual([
      {
        type: 'node',
        nodeType: 'step',
        outcome: 'completed',
      },
      {
        type: 'node_duration',
        durationSeconds: 7,
        nodeType: 'step',
        outcome: 'completed',
      },
    ]);
  });

  it('omits node telemetry when type metadata is unavailable', () => {
    const tracker = new WorkflowTelemetryTracker();
    expect(tracker.observe(nodeComplete('wf'), true)).toEqual([]);
    expect(tracker.activeRunCount()).toBe(1);
  });

  it('derives terminal duration from snapshot timestamps and cleans up state', () => {
    const tracker = new WorkflowTelemetryTracker();
    tracker.observe(runStart('wf'), true);

    expect(tracker.observe(terminal('wf'), true)).toEqual([
      {
        type: 'run',
        event: 'completed',
        topology: 'sequential',
        stepBucket: '1',
      },
      {
        type: 'run_duration',
        durationSeconds: 45,
        outcome: 'completed',
        topology: 'sequential',
        stepBucket: '1',
      },
      { type: 'concurrent', activeRuns: 0 },
    ]);
    expect(tracker.activeRunCount()).toBe(0);
    expect(tracker.observe(terminal('wf'), true)).toEqual([]);
  });

  it('samples running and paused runs after live run transitions', () => {
    const tracker = new WorkflowTelemetryTracker();
    tracker.observe(runStart('one'), true);
    expect(tracker.observe(runStart('two'), true).at(-1)).toEqual({
      type: 'concurrent',
      activeRuns: 2,
    });
    expect(
      tracker.observe(
        {
          type: 'paused',
          workflowId: 'one',
          pauseReason: 'private-reason',
        },
        true
      )
    ).toEqual([
      {
        type: 'run',
        event: 'paused',
        topology: 'sequential',
        stepBucket: '1',
      },
      { type: 'concurrent', activeRuns: 2 },
    ]);
    expect(tracker.observe(terminal('two', 'aborted'), true).at(-1)).toEqual({
      type: 'concurrent',
      activeRuns: 1,
    });
  });

  it('clears retained run and terminal deduplication state on reset', () => {
    const tracker = new WorkflowTelemetryTracker();
    tracker.observe(runStart('wf'), true);
    tracker.observe(terminal('wf'), true);
    expect(tracker.observe(terminal('wf'), true)).toEqual([]);

    tracker.observe(runStart('active'), true);
    tracker.reset();

    expect(tracker.activeRunCount()).toBe(0);
    expect(tracker.observe(terminal('wf'), true)).not.toEqual([]);
  });
});
