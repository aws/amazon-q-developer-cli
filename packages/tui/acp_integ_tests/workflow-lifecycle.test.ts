import { afterEach, describe, expect, it } from 'bun:test';
import type {
  LoadSessionResponse,
  PromptResponse,
  SessionNotification,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  notifyWorkflowEvent,
  setupWorkflowHandshake,
  WORKFLOW_PARENT_SESSION_ID,
} from './shared/workflow-harness';

const WORKFLOW_ID = 'workflow-repeat-review';
const WORKFLOW_NAME = 'Iterative review';
const LOOP_ID = 'review-loop';
const STEP_ID = 'review';

function workflowPath(iteration: number): string[] {
  return [WORKFLOW_ID, LOOP_ID, `iter-${iteration}`, STEP_ID];
}

function notifyRunStart(tc: AcpTestCase): void {
  notifyWorkflowEvent(tc, {
    type: 'run_start',
    workflowId: WORKFLOW_ID,
    parentSessionId: WORKFLOW_PARENT_SESSION_ID,
    workflowName: WORKFLOW_NAME,
    inputs: {},
    nodeTree: [
      {
        nodeId: LOOP_ID,
        type: 'repeat',
        maxIterations: 3,
        steps: [
          {
            nodeId: STEP_ID,
            type: 'step',
            agentName: 'reviewer',
          },
        ],
      },
    ],
  });
}

function notifyStepStart(
  tc: AcpTestCase,
  iteration: number,
  sessionId: string
): void {
  notifyWorkflowEvent(tc, {
    type: 'node_start',
    workflowId: WORKFLOW_ID,
    parentSessionId: WORKFLOW_PARENT_SESSION_ID,
    nodeId: STEP_ID,
    nodePath: workflowPath(iteration),
    nodeType: 'step',
    agentName: 'reviewer',
    sessionId,
    iteration,
  });
}

describe('workflow lifecycle over ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('renders a repeated child journey through pause, resume, and completion', async () => {
    tc = createWorkflowTestCase('workflow-repeat-lifecycle');
    setupWorkflowHandshake(tc);
    tc.mock.on<unknown, LoadSessionResponse>('session/load', () => ({
      modes: {
        currentModeId: 'vibe',
        availableModes: [],
      },
    }));
    let resolveResponsePrompt!: (response: PromptResponse) => void;
    const responsePrompt = new Promise<PromptResponse>((resolve) => {
      resolveResponsePrompt = resolve;
    });
    tc.mock.on('session/prompt', () => responsePrompt);
    let resolveSteer!: (params: unknown) => void;
    const steerRequest = new Promise<unknown>((resolve) => {
      resolveSteer = resolve;
    });
    tc.mock.on('_session/steer', (params) => {
      resolveSteer(params);
      return {};
    });

    await launchWorkflowCase(tc);

    notifyRunStart(tc);
    await tc.waitForVisibleText(WORKFLOW_NAME, 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);

    notifyWorkflowEvent(tc, {
      type: 'node_start',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: LOOP_ID,
      nodePath: [WORKFLOW_ID, LOOP_ID],
      nodeType: 'repeat',
    });
    notifyStepStart(tc, 0, 'review-session-0');
    await tc.waitForVisibleText('1/3', 5_000);

    tc.mock.notify('session/update', {
      sessionId: 'review-session-0',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'First review is running.' },
      },
    } satisfies SessionNotification);
    await tc.waitForVisibleText('First review is running.', 5_000);

    await tc.sendKeys('s');
    await tc.waitForVisibleText('Steer', 5_000);
    await tc.sendKeys('Check the retry path');
    await tc.pressEnter();
    expect(await steerRequest).toEqual({
      sessionId: 'review-session-0',
      message: 'Check the retry path',
    });

    tc.mock.notify('session/update', {
      sessionId: 'review-session-0',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'review-write',
        title: 'Write review notes',
        kind: 'edit',
        rawInput: { path: 'review.md' },
      },
    } satisfies SessionNotification);
    const permissionResponse = tc.mock.request('session/request_permission', {
      sessionId: 'review-session-0',
      toolCall: { toolCallId: 'review-write' },
      options: [
        {
          kind: 'allow_once',
          name: 'Allow once',
          optionId: 'allow-once',
        },
        {
          kind: 'reject_once',
          name: 'Deny',
          optionId: 'deny',
        },
      ],
      _meta: { kiro: { toolId: 'review-write' } },
    });
    await tc.waitForVisibleText('requires approval', 5_000);
    await tc.pressEnter();
    expect(await permissionResponse).toMatchObject({
      outcome: { outcome: 'selected', optionId: 'allow-once' },
    });

    notifyWorkflowEvent(tc, {
      type: 'node_complete',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: STEP_ID,
      nodePath: workflowPath(0),
      status: 'completed',
      capturedOutput: 'First review complete',
    });
    notifyWorkflowEvent(tc, {
      type: 'loop_iteration',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      loopId: LOOP_ID,
      iteration: 0,
      stopConditionMet: false,
    });
    notifyWorkflowEvent(tc, {
      type: 'node_paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: STEP_ID,
      nodePath: workflowPath(1),
      reason: 'Waiting for review direction',
    });
    notifyWorkflowEvent(tc, {
      type: 'paused',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      pauseReason: 'Waiting for review direction',
    });
    await tc.waitForVisibleText('Waiting for review direction', 5_000);

    await tc.sendKeys('s');
    await tc.waitForVisibleText('reviewer - iteration 0', 5_000);
    await tc.sendKeys('Apply the requested direction');
    await tc.pressEnter();
    await tc.waitForVisibleText('Thinking...', 5_000);
    expect(tc.mock.receivedRequests('session/load')).toHaveLength(1);
    expect(tc.mock.receivedRequests('session/prompt')).toHaveLength(1);
    tc.mock.notify('session/update', {
      sessionId: 'review-session-0',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'Direction accepted.' },
      },
    } satisfies SessionNotification);
    await tc.waitForVisibleText('Direction accepted.', 5_000);
    resolveResponsePrompt({ stopReason: 'end_turn' });

    notifyRunStart(tc);
    notifyStepStart(tc, 1, 'review-session-1');
    notifyWorkflowEvent(tc, {
      type: 'loop_iteration',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      loopId: LOOP_ID,
      iteration: 1,
      stopConditionMet: true,
    });
    await tc.waitForVisibleText('2/3', 5_000);

    const resumedSnapshot = tc.getSnapshotFormatted();
    expect(resumedSnapshot).toContain('[repeat] review-loop');
    expect(resumedSnapshot).toContain('reviewer');
    expect(resumedSnapshot.match(/1\/3/g)).toHaveLength(1);
    expect(resumedSnapshot.match(/2\/3/g)).toHaveLength(2);

    notifyWorkflowEvent(tc, {
      type: 'node_complete',
      workflowId: WORKFLOW_ID,
      parentSessionId: WORKFLOW_PARENT_SESSION_ID,
      nodeId: STEP_ID,
      nodePath: workflowPath(1),
      status: 'completed',
      capturedOutput: 'Second review complete',
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
              nodeId: LOOP_ID,
              type: 'repeat',
              status: 'completed',
              children: [
                {
                  nodeId: 'review-iteration-1',
                  type: 'sequence',
                  status: 'completed',
                  iteration: 1,
                  children: [
                    {
                      nodeId: STEP_ID,
                      type: 'step',
                      status: 'completed',
                      agentName: 'reviewer',
                      sessionId: 'review-session-1',
                      capturedOutput: 'Second review complete',
                    },
                  ],
                },
              ],
            },
          ],
        },
      },
    });
    await tc.waitForVisibleText('completed', 5_000);

    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('ask a question', 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
    expect(tc.getSnapshotFormatted()).toContain(WORKFLOW_NAME);
    expect(tc.getSnapshotFormatted()).toContain('completed');
  }, 30_000);
});
