import { describe, expect, it, mock } from 'bun:test';
import { KAS_COMMANDS } from '../../../kas-commands';
import type { WorkflowInspectResponse } from '../../../types/workflow-history';
import type { WorkflowRunSummary } from '../../../types/workflow-history';
import { executeCommand } from '../../index';
import {
  createMockCommandContext,
  type CreateMockCtxOptions,
} from '../../__tests__/test-helpers';

function run(
  workflowId: string,
  parentSessionId: string,
  updatedAt: string
): WorkflowRunSummary {
  return {
    workflowId,
    name: workflowId,
    status: 'completed',
    createdAt: updatedAt,
    updatedAt,
    parentSessionId,
  };
}

function workflowControls() {
  return {
    resumeWorkflow: mock(async (workflowId: string) => ({
      workflowId,
      status: 'running' as const,
    })),
    cancelWorkflow: mock(async (_workflowId: string) => ({
      ok: true,
      previousStatus: 'running' as const,
    })),
  };
}

function createKasContext(kiro: CreateMockCtxOptions['kiro'] = {}) {
  const ctx = createMockCommandContext({
    kasCommands: KAS_COMMANDS,
    kiro,
  });
  ctx.agentEngine = 'kas';
  return ctx;
}

describe('/workflow KAS command', () => {
  it('opens session-scoped history newest first for a bare command', async () => {
    const older = run('older', 'session-1', '2026-07-18T10:00:00.000Z');
    const newer = run('newer', 'session-1', '2026-07-19T10:00:00.000Z');
    const listWorkflows = mock(async () => [
      older,
      run('other-session', 'session-2', '2026-07-20T10:00:00.000Z'),
      newer,
    ]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows,
    });

    expect(await executeCommand('/workflow', ctx)).toBe(true);

    expect(listWorkflows).toHaveBeenCalledTimes(1);
    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalledWith(true, [
      newer,
      older,
    ]);
  });

  it('reports an empty history without opening the history surface', async () => {
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows: mock(async () => []),
    });

    expect(await executeCommand('/workflow', ctx)).toBe(true);

    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No workflows in this session yet. Start one with /workflow run.',
      'warning',
      3000
    );
    expect(ctx._spies.setShowWorkflowHistory).not.toHaveBeenCalled();
  });

  it('forwards canonical list and run syntax through the KAS prompt path', async () => {
    for (const [input, expected] of [
      ['/workflow list', '/workflow list'],
      [
        '/workflow run release --branch main',
        '/workflow run release --branch main',
      ],
    ] as const) {
      const ctx = createKasContext();

      expect(await executeCommand(input, ctx)).toBe(true);
      expect(ctx._spies.sendMessage).toHaveBeenCalledWith(expected);
      expect(ctx.kiro.executeCommand).not.toHaveBeenCalled();
    }
  });

  it('uses typed public Kiro methods for workflow controls', async () => {
    const controls = workflowControls();
    const inspectWorkflow = mock(
      async (_workflowId: string): Promise<WorkflowInspectResponse> => ({
        workflowId: 'wf-status',
        state: {
          workflowId: 'wf-status',
          workflowName: 'release',
          status: 'paused',
          inputs: {},
          artifacts: {},
          capturedOutputs: {},
          root: {
            nodeId: 'root',
            type: 'sequence',
            status: 'paused',
          },
        },
      })
    );
    const ctx = createKasContext({
      ...controls,
      inspectWorkflow,
    });

    expect(await executeCommand('/workflow resume wf-resume', ctx)).toBe(true);
    expect(await executeCommand('/workflow status wf-status', ctx)).toBe(true);
    expect(await executeCommand('/workflow cancel wf-cancel', ctx)).toBe(true);

    expect(controls.resumeWorkflow).toHaveBeenCalledWith('wf-resume');
    expect(inspectWorkflow).toHaveBeenCalledWith('wf-status');
    expect(controls.cancelWorkflow).toHaveBeenCalledWith('wf-cancel');
  });
});

describe('workflow compatibility aliases', () => {
  it('normalizes exact aliases to the canonical command behavior', async () => {
    const controls = workflowControls();
    const listWorkflows = mock(async () => [
      run('historical', 'session-1', '2026-07-19T10:00:00.000Z'),
    ]);
    const inspectWorkflow = mock(
      async (_workflowId: string): Promise<WorkflowInspectResponse> => ({
        workflowId: 'wf-status',
        state: {
          workflowId: 'wf-status',
          workflowName: 'release',
          status: 'running',
          inputs: {},
          artifacts: {},
          capturedOutputs: {},
          root: {
            nodeId: 'root',
            type: 'sequence',
            status: 'running',
          },
        },
      })
    );
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows,
      inspectWorkflow,
      ...controls,
    });

    expect(await executeCommand('/workflows', ctx)).toBe(true);
    expect(await executeCommand('/workflow-run release', ctx)).toBe(true);
    expect(await executeCommand('/workflow-resume wf-resume', ctx)).toBe(true);
    expect(await executeCommand('/workflow-status wf-status', ctx)).toBe(true);
    expect(await executeCommand('/workflow-cancel wf-cancel', ctx)).toBe(true);

    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalled();
    expect(ctx._spies.sendMessage).toHaveBeenCalledWith(
      '/workflow run release'
    );
    expect(controls.resumeWorkflow).toHaveBeenCalledWith('wf-resume');
    expect(inspectWorkflow).toHaveBeenCalledWith('wf-status');
    expect(controls.cancelWorkflow).toHaveBeenCalledWith('wf-cancel');
  });

  it('does not prefix-match hidden aliases', async () => {
    for (const input of [
      '/workflows-extra',
      '/workflow-r release',
      '/workflow-res wf-1',
      '/workflow-s wf-1',
      '/workflow-c wf-1',
    ]) {
      const ctx = createKasContext();

      expect(await executeCommand(input, ctx)).toBe(false);
      expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
    }
  });
});
