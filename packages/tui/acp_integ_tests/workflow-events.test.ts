import { afterEach, describe, expect, it } from 'bun:test';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  notifyWorkflowEvent,
  setupWorkflowHandshake,
  WORKFLOW_PARENT_SESSION_ID,
} from './shared/workflow-harness';

const WORKFLOW_ID = 'workflow-event-matrix';
const WORKFLOW_NAME = 'Lifecycle matrix';

/** The steps-pane part of the watch row, so the neighbouring pane cannot satisfy a row assertion. */
function watchRow(tc: AcpTestCase): string {
  const line =
    tc.getSnapshot().find((row) => row.includes('[watch] signal')) ?? '';
  return line.split('│')[0] ?? '';
}

describe('workflow ACP event rendering', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('renders every lifecycle event and workflow node type', async () => {
    tc = createWorkflowTestCase('workflow-event-matrix', {
      terminalSize: { width: 140, height: 42 },
    });
    setupWorkflowHandshake(tc);
    await launchWorkflowCase(tc);

    const initialSnapshot = tc.getSnapshotFormatted();
    expect(initialSnapshot).not.toContain('ctrl+g monitor');
    expect(initialSnapshot).not.toContain(WORKFLOW_NAME);

    notifyWorkflowEvent(tc, {
      type: 'run_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      workflowName: WORKFLOW_NAME,
      inputs: {},
      nodeTree: [
        {
          nodeId: 'pipeline',
          type: 'sequence',
          steps: [
            { nodeId: 'plan', type: 'step', agentName: 'planner' },
            {
              nodeId: 'fanout',
              type: 'parallel',
              joinPolicy: 'all',
              branches: [
                { nodeId: 'build', type: 'step', agentName: 'builder' },
                { nodeId: 'test', type: 'step', agentName: 'tester' },
              ],
            },
            {
              nodeId: 'review-cycle',
              type: 'repeat',
              maxIterations: 3,
              steps: [
                { nodeId: 'review', type: 'step', agentName: 'reviewer' },
              ],
            },
            { nodeId: 'signal', type: 'watch' },
          ],
        },
      ],
    });
    await tc.waitForVisibleText(WORKFLOW_NAME, 5_000);
    expect(tc.getSnapshotFormatted()).toContain('ctrl+g monitor');

    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
    const planSnapshot = tc.getSnapshotFormatted();
    for (const label of [
      '[sequence] pipeline',
      'planner',
      '[parallel] fanout',
      'builder',
      'tester',
      '[repeat] review-cycle',
      'reviewer',
      '[watch] signal',
    ]) {
      expect(planSnapshot).toContain(label);
    }

    notifyWorkflowEvent(tc, {
      type: 'steps_queued',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      pendingSteps: [
        {
          nodeId: 'release',
          type: 'step',
          agentName: 'release-manager',
        },
      ],
    });
    await tc.waitForVisibleText('release-manager', 5_000);

    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'signal',
      nodePath: [WORKFLOW_ID, 'pipeline', 'signal'],
      nodeType: 'watch',
    });
    notifyWorkflowEvent(tc, {
      type: 'watch_poll',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'signal',
      nodePath: [WORKFLOW_ID, 'pipeline', 'signal'],
      outcome: 'new-activity',
      at: '2026-08-15T10:02:00.000Z',
    });
    await tc.waitForVisibleText('new activity', 5_000);
    expect(watchRow(tc)).toContain('new activity');

    notifyWorkflowEvent(tc, {
      type: 'node_paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'signal',
      nodePath: [WORKFLOW_ID, 'pipeline', 'signal'],
      reason: 'Approve deploy?',
    });
    await tc.waitForVisibleText('Approve deploy?', 5_000);
    // The node now carries a poll outcome and a park reason at once.
    expect(watchRow(tc)).not.toContain('new activity');

    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'signal',
      nodePath: [WORKFLOW_ID, 'pipeline', 'signal'],
      nodeType: 'watch',
    });
    notifyWorkflowEvent(tc, {
      type: 'watch_poll',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'signal',
      nodePath: [WORKFLOW_ID, 'pipeline', 'signal'],
      outcome: 'terminal-state',
      at: '2026-08-15T10:03:00.000Z',
    });
    await tc.waitForVisibleText('terminal state', 5_000);
    expect(watchRow(tc)).not.toContain('Approve deploy?');

    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'review',
      nodePath: [WORKFLOW_ID, 'pipeline', 'review-cycle', 'iter-1', 'review'],
      nodeType: 'step',
      agentName: 'reviewer',
      iteration: 1,
    });
    notifyWorkflowEvent(tc, {
      type: 'loop_iteration',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      loopId: 'review-cycle',
      iteration: 1,
      stopConditionMet: false,
    });
    await tc.waitForVisibleText('2/3', 5_000);

    notifyWorkflowEvent(tc, {
      type: 'node_complete',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'review',
      nodePath: [WORKFLOW_ID, 'pipeline', 'review-cycle', 'iter-1', 'review'],
      status: 'completed',
      capturedOutput: 'Review complete',
    });
    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: [WORKFLOW_ID, 'pipeline', 'fanout', 'build'],
      nodeType: 'step',
      agentName: 'builder',
    });
    notifyWorkflowEvent(tc, {
      type: 'node_paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: [WORKFLOW_ID, 'pipeline', 'fanout', 'build'],
      reason: 'Waiting for build approval',
    });
    notifyWorkflowEvent(tc, {
      type: 'paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      pauseReason: 'Waiting for build approval',
    });
    await tc.waitForVisibleText('Waiting for build approval', 5_000);
    await tc.waitForVisibleText('Lifecycle matrix - paused', 5_000);

    await tc.pressEscape();
    await tc.waitForVisibleText('ask a question', 5_000);
    const pausedSnapshot = tc.getSnapshotFormatted();
    expect(pausedSnapshot).toContain(WORKFLOW_NAME);
    expect(pausedSnapshot).toContain('paused');

    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
    notifyWorkflowEvent(tc, {
      type: 'steps_queued',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      pendingSteps: [],
      resolution: { outcome: 'dropped' },
    });
    notifyWorkflowEvent(tc, {
      type: 'run_complete',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      status: 'completed',
      finalState: {
        workflowId: WORKFLOW_ID,
        workflowName: WORKFLOW_NAME,
        status: 'completed',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: WORKFLOW_PARENT_SESSION_ID,
        root: {
          nodeId: WORKFLOW_ID,
          type: 'sequence',
          status: 'completed',
          children: [
            {
              nodeId: 'pipeline',
              type: 'sequence',
              status: 'completed',
            },
          ],
        },
      },
    });
    await tc.waitForVisibleText('completed', 5_000);
    expect(tc.getSnapshotFormatted()).not.toContain('release-manager');
  }, 30_000);
});
