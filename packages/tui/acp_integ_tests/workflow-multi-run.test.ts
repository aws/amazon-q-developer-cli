import { afterEach, describe, expect, it } from 'bun:test';
import type { SessionNotification } from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  notifyWorkflowEvent,
  setupWorkflowHandshake,
  WORKFLOW_PARENT_SESSION_ID,
} from './shared/workflow-harness';

function startWorkflow(
  tc: AcpTestCase,
  workflowId: string,
  workflowName: string,
  agentName: string,
  sessionId: string
): void {
  notifyWorkflowEvent(tc, {
    type: 'run_start',
    workflowId,
    parentSessionId: WORKFLOW_PARENT_SESSION_ID,
    workflowName,
    inputs: {},
    nodeTree: [{ nodeId: 'work', type: 'step', agentName }],
  });
  notifyWorkflowEvent(tc, {
    type: 'node_start',
    workflowId,
    parentSessionId: WORKFLOW_PARENT_SESSION_ID,
    nodeId: 'work',
    nodePath: [workflowId, 'work'],
    nodeType: 'step',
    agentName,
    sessionId,
  });
}

function notifyChild(
  tc: AcpTestCase,
  sessionId: string,
  update: SessionNotification['update']
): void {
  tc.mock.notify('session/update', { sessionId, update });
}

describe('multiple workflows over ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('keeps child output, tools, and permissions with the owning workflow', async () => {
    tc = createWorkflowTestCase('workflow-multi-run-routing');
    setupWorkflowHandshake(tc);
    await launchWorkflowCase(tc);

    startWorkflow(
      tc,
      'workflow-alpha',
      'Alpha workflow',
      'alpha-agent',
      'alpha-session'
    );
    startWorkflow(
      tc,
      'workflow-beta',
      'Beta workflow',
      'beta-agent',
      'beta-session'
    );
    await tc.waitForVisibleText('Alpha workflow', 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);

    notifyChild(tc, 'alpha-session', {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text: 'ALPHA_OUTPUT_ONLY' },
    });
    notifyChild(tc, 'alpha-session', {
      sessionUpdate: 'tool_call',
      toolCallId: 'alpha-write',
      title: 'Write alpha.md',
      kind: 'edit',
      rawInput: { path: 'alpha.md' },
    });
    notifyChild(tc, 'beta-session', {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text: 'BETA_OUTPUT_ONLY' },
    });
    notifyChild(tc, 'beta-session', {
      sessionUpdate: 'tool_call',
      toolCallId: 'beta-read',
      title: 'Read beta.md',
      kind: 'read',
      rawInput: { path: 'beta.md' },
    });

    await tc.waitForVisibleText('ALPHA_OUTPUT_ONLY', 5_000);
    await tc.waitForVisibleText('alpha.md', 5_000);
    let snapshot = tc.getSnapshotFormatted();
    expect(snapshot).not.toContain('BETA_OUTPUT_ONLY');
    expect(snapshot).not.toContain('beta.md');

    await tc.sendKeys('\x1b[C');
    await tc.waitForVisibleText('Beta workflow', 5_000);
    await tc.waitForVisibleText('BETA_OUTPUT_ONLY', 5_000);
    await tc.waitForVisibleText('beta.md', 5_000);
    snapshot = tc.getSnapshotFormatted();
    expect(snapshot).not.toContain('ALPHA_OUTPUT_ONLY');
    expect(snapshot).not.toContain('alpha.md');

    const permissionResponse = tc.mock.request('session/request_permission', {
      sessionId: 'alpha-session',
      toolCall: { toolCallId: 'alpha-write' },
      options: [
        {
          kind: 'allow_once',
          name: 'Allow once',
          optionId: 'allow-alpha',
        },
        {
          kind: 'reject_once',
          name: 'Deny',
          optionId: 'deny-alpha',
        },
      ],
      _meta: { kiro: { toolId: 'alpha-write' } },
    });
    await tc.waitForStore((state) => state.approvalQueue.length === 1, 5_000);
    expect(tc.getSnapshotFormatted()).not.toContain('requires approval');

    await tc.sendKeys('\x1b[D');
    await tc.waitForVisibleText('Alpha workflow', 5_000);
    await tc.waitForVisibleText('requires approval', 5_000);
    expect(tc.getSnapshotFormatted()).toContain('alpha.md');
    await tc.pressEnter();
    expect(await permissionResponse).toMatchObject({
      outcome: { outcome: 'selected', optionId: 'allow-alpha' },
    });

    await tc.sendKeys('2');
    await tc.waitForVisibleText('Beta workflow', 5_000);
    await tc.sendKeys('1');
    await tc.waitForVisibleText('Alpha workflow', 5_000);

    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('ask a question', 5_000);
    snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Alpha workflow');
    expect(snapshot).toContain('Beta workflow');
    expect(snapshot).toContain('ctrl+g monitor');

    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
    expect(tc.getSnapshotFormatted()).toContain('Alpha workflow');
  }, 30_000);
});
