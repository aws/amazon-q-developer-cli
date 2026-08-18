import { afterEach, describe, expect, it } from 'bun:test';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  setupWorkflowHandshake,
  workflowRunSummary,
  WORKFLOW_PARENT_SESSION_ID,
} from './shared/workflow-harness';

const WORKFLOW_ID = 'workflow-history-paused';

async function submit(tc: AcpTestCase, command: string): Promise<void> {
  await tc.sendKeys(command);
  await tc.pressEnter();
}

async function waitForRequestCount(
  tc: AcpTestCase,
  method: string,
  count: number
): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (tc.mock.receivedRequests(method).length >= count) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`Timed out waiting for ${method} request ${count}`);
}

describe('/workflow over ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('stays hidden when empty, resumes directly, and resumes from history', async () => {
    tc = createWorkflowTestCase('workflow-command-history');
    let runs = [] as ReturnType<typeof workflowRunSummary>[];
    setupWorkflowHandshake(tc, () => runs);
    tc.mock.on('_kiro/workflow/resume', () => ({
      workflowId: WORKFLOW_ID,
      status: 'running',
    }));
    tc.mock.on('_kiro/workflow/inspect', () => ({
      workflowId: WORKFLOW_ID,
      state: {
        workflowId: WORKFLOW_ID,
        workflowName: 'Paused workflow',
        status: 'running',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: WORKFLOW_PARENT_SESSION_ID,
        root: {
          nodeId: 'historical-step',
          type: 'step',
          status: 'running',
          agentName: 'historical-agent',
        },
      },
    }));
    await launchWorkflowCase(tc);

    expect(tc.getSnapshotFormatted()).not.toContain('ctrl+g monitor');
    await submit(tc, '/workflow');
    await tc.waitForVisibleText('No workflows in this session yet', 5_000);

    runs = [workflowRunSummary(WORKFLOW_ID, 'Paused workflow', 'paused')];
    await submit(tc, `/workflow resume ${WORKFLOW_ID}`);
    await tc.waitForVisibleText('Workflow is running', 5_000);
    await waitForRequestCount(tc, '_kiro/workflow/resume', 1);
    expect(
      tc.mock.receivedRequests('_kiro/workflow/resume')[0]?.params
    ).toMatchObject({
      workflowId: WORKFLOW_ID,
      initiator: 'user',
    });

    await submit(tc, '/workflow');
    await tc.waitForVisibleText('this session', 5_000);
    await tc.waitForVisibleText('Paused workflow', 5_000);
    await tc.waitForVisibleText('r resume', 5_000);

    await tc.sendKeys('r');
    await waitForRequestCount(tc, '_kiro/workflow/resume', 2);
    await tc.waitForVisibleText('running', 5_000);

    await tc.sendKeys('\x1b[C');
    await waitForRequestCount(tc, '_kiro/workflow/inspect', 1);
    await tc.waitForVisibleText('WORKFLOW OUTPUT', 5_000);
    await tc.waitForVisibleText('Paused workflow', 5_000);
    expect(tc.getSnapshotFormatted()).toContain('historical-agent');

    await tc.pressEscape();
    await tc.waitForVisibleText('ask a question', 5_000);
  }, 30_000);
});
