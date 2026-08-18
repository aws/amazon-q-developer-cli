import { afterEach, describe, expect, it } from 'bun:test';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  notifyWorkflowEvent,
  setupWorkflowHandshake,
  WORKFLOW_PARENT_SESSION_ID,
} from './shared/workflow-harness';

const WORKFLOW_ID = 'workflow-controls';

async function waitForRequest(
  tc: AcpTestCase,
  method: string,
  count = 1
): Promise<unknown> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const requests = tc.mock.receivedRequests(method);
    if (requests.length >= count) return requests[count - 1]!.params;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`Timed out waiting for ${method} request ${count}`);
}

async function waitForLineIndex(
  tc: AcpTestCase,
  text: string,
  predicate: (line: number) => boolean
): Promise<number> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const line = tc.getSnapshot().findIndex((row) => row.includes(text));
    if (line >= 0 && predicate(line)) return line;
    await tc.sleepMs(25);
  }
  throw new Error(`Timed out waiting for "${text}" to move`);
}

describe('workflow monitor controls over ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('routes monitor navigation and lifecycle shortcuts to the owning run', async () => {
    tc = createWorkflowTestCase('workflow-monitor-controls');
    setupWorkflowHandshake(tc);
    tc.mock.on('_kiro/workflow/pause', () => ({ paused: true }));
    tc.mock.on('_kiro/workflow/resume', () => ({
      workflowId: WORKFLOW_ID,
      status: 'running',
    }));
    tc.mock.on('_kiro/workflow/retry', () => ({
      workflowId: WORKFLOW_ID,
      status: 'running',
      retriedNodeIds: ['second'],
    }));
    tc.mock.on('_kiro/workflow/cancel', () => ({
      ok: true,
      previousStatus: 'running',
    }));
    await launchWorkflowCase(tc);

    notifyWorkflowEvent(tc, {
      type: 'run_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      workflowName: 'Control validation',
      inputs: {},
      nodeTree: [
        { nodeId: 'first', type: 'step', agentName: 'first-agent' },
        { nodeId: 'second', type: 'step', agentName: 'second-agent' },
      ],
    });
    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'first',
      nodePath: [WORKFLOW_ID, 'first'],
      nodeType: 'step',
      agentName: 'first-agent',
      sessionId: 'first-session',
    });
    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'second',
      nodePath: [WORKFLOW_ID, 'second'],
      nodeType: 'step',
      agentName: 'second-agent',
      sessionId: 'second-session',
    });
    await tc.waitForVisibleText('Control validation', 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW OUTPUT [second-agent]', 5_000);

    await tc.sendKeys('\x1b[A');
    await tc.waitForVisibleText('WORKFLOW OUTPUT [first-agent]', 5_000);
    await tc.sendKeys('\x1b[B');
    await tc.waitForVisibleText('WORKFLOW OUTPUT [second-agent]', 5_000);

    expect(tc.getSnapshotFormatted()).toContain('l stack');
    await tc.sendKeys('l');
    await tc.waitForVisibleText('l split', 5_000);
    expect(tc.getSnapshotFormatted()).toContain('mouse:on');
    await tc.sendKeys('m');
    await tc.waitForVisibleText('mouse:off', 5_000);
    expect(tc.getSnapshotFormatted()).toContain('[ ] resize');
    const outputLine = await waitForLineIndex(
      tc,
      'WORKFLOW OUTPUT [second-agent]',
      () => true
    );
    await tc.sendKeys(']');
    const expandedDagOutputLine = await waitForLineIndex(
      tc,
      'WORKFLOW OUTPUT [second-agent]',
      (line) => line > outputLine
    );
    await tc.sendKeys('[');
    await waitForLineIndex(
      tc,
      'WORKFLOW OUTPUT [second-agent]',
      (line) => line < expandedDagOutputLine
    );

    await tc.sendKeys('p');
    expect(await waitForRequest(tc, '_kiro/workflow/pause')).toMatchObject({
      workflowId: WORKFLOW_ID,
      initiator: 'user',
    });
    notifyWorkflowEvent(tc, {
      type: 'paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      pauseReason: 'Paused by control test',
      initiator: 'user',
    });
    await tc.waitForVisibleText('r resume', 5_000);

    await tc.sendKeys('r');
    expect(await waitForRequest(tc, '_kiro/workflow/resume')).toMatchObject({
      workflowId: WORKFLOW_ID,
      initiator: 'user',
    });
    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: 'second',
      nodePath: [WORKFLOW_ID, 'second'],
      nodeType: 'step',
      agentName: 'second-agent',
      sessionId: 'second-session',
    });
    await tc.waitForVisibleText('p pause', 5_000);

    await tc.sendKeys('\t');
    await tc.waitForVisibleText('AGENT MONITOR', 5_000);
    await tc.sendKeys('\t');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);

    await tc.sendKeys('\x18');
    await tc.waitForVisibleText('ctrl+x stop workflow', 5_000);
    expect(tc.mock.receivedRequests('_kiro/workflow/cancel')).toHaveLength(0);
    await tc.sendKeys('\x18');
    expect(await waitForRequest(tc, '_kiro/workflow/cancel')).toMatchObject({
      workflowId: WORKFLOW_ID,
      targetStatus: 'aborted',
      initiator: 'user',
    });

    notifyWorkflowEvent(tc, {
      type: 'run_complete',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      status: 'failed',
      finalState: {
        workflowId: WORKFLOW_ID,
        workflowName: 'Control validation',
        status: 'failed',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: WORKFLOW_PARENT_SESSION_ID,
        root: {
          nodeId: WORKFLOW_ID,
          type: 'sequence',
          status: 'failed',
          children: [
            {
              nodeId: 'first',
              type: 'step',
              status: 'completed',
              agentName: 'first-agent',
              sessionId: 'first-session',
            },
            {
              nodeId: 'second',
              type: 'step',
              status: 'failed',
              agentName: 'second-agent',
              sessionId: 'second-session',
              failureReason: 'Validation failed',
            },
          ],
        },
      },
    });
    await tc.waitForVisibleText('r retry step', 5_000);
    await tc.sendKeys('r');
    expect(await waitForRequest(tc, '_kiro/workflow/retry')).toEqual({
      workflowId: WORKFLOW_ID,
      nodeId: 'second',
    });

    await tc.sendKeys('q');
    await tc.waitForVisibleText('ask a question', 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
  }, 30_000);
});
