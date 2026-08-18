import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  LoadSessionRequest,
  LoadSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';
import {
  createWorkflowTestCase,
  launchWorkflowCase,
  setupWorkflowHandshake,
  workflowRunSummary,
} from './shared/workflow-harness';

const SESSION_ID = 'workflow-resumed-parent';
const WORKFLOW_ID = 'workflow-restored-active';
const WORKFLOW_NAME = 'Restored active workflow';

async function submit(tc: AcpTestCase, command: string): Promise<void> {
  await tc.sendKeys(command);
  await tc.pressEnter();
}

describe('workflow restoration over ACP', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    tc = null;
    cwd = null;
  });

  it('shows active workflows after resuming their parent session', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-workflow-resume-')));
    const run = {
      ...workflowRunSummary(WORKFLOW_ID, WORKFLOW_NAME, 'running'),
      parentSessionId: SESSION_ID,
    };
    tc = createWorkflowTestCase('workflow-resume-active', {
      args: ['--resume'],
      cwd,
      mockKasSessionListResult: [
        {
          sessionId: SESSION_ID,
          cwd,
          title: 'Workflow parent',
          updatedAt: new Date().toISOString(),
        },
      ],
    });
    setupWorkflowHandshake(tc, () => [run]);
    tc.mock.on<LoadSessionRequest, LoadSessionResponse>(
      'session/load',
      (request) => {
        expect(request.sessionId).toBe(SESSION_ID);
        return { modes: defaultKasModes() };
      }
    );
    tc.mock.on('_kiro/workflow/load', () => ({
      workflowId: WORKFLOW_ID,
      state: {
        workflowId: WORKFLOW_ID,
        workflowName: WORKFLOW_NAME,
        status: 'running',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: SESSION_ID,
        root: {
          nodeId: WORKFLOW_ID,
          type: 'sequence',
          status: 'running',
          children: [
            {
              nodeId: 'active-step',
              type: 'step',
              status: 'running',
              agentName: 'restored-agent',
              sessionId: 'restored-child-session',
            },
          ],
        },
      },
      stepSessions: [
        {
          nodeId: 'active-step',
          nodePath: [WORKFLOW_ID, 'active-step'],
          sessionId: 'restored-child-session',
        },
      ],
    }));

    await launchWorkflowCase(tc);
    await tc.waitForStore(
      (state) => state.sessionId === SESSION_ID && state.isInitialized,
      10_000
    );
    await tc.waitForVisibleText('Restored active w', 5_000);
    await tc.waitForVisibleText('ctrl+g monitor', 5_000);
    expect(tc.mock.receivedRequests('_kiro/workflow/list')).toHaveLength(1);
    expect(tc.mock.receivedRequests('_kiro/workflow/load')).toHaveLength(1);

    await submit(tc, '/workflow');
    await tc.waitForVisibleText('this session', 5_000);
    await tc.waitForVisibleText(WORKFLOW_NAME, 5_000);

    await tc.pressEscape();
    await tc.waitForVisibleText('ask a question', 5_000);
    await tc.sendKeys('\x07');
    await tc.waitForVisibleText('WORKFLOW(S)', 5_000);
    await tc.waitForVisibleText('restored-agent', 5_000);
    await tc.waitForVisibleText('Waiting for agent output...', 5_000);
  }, 30_000);
});
