import { describe, expect, it, mock } from 'bun:test';
import { KAS_COMMANDS } from '../../../kas-commands';
import type { WorkflowInspectResponse } from '../../../types/workflow-history';
import type { WorkflowRunSummary } from '../../../types/workflow-history';
import type {
  WorkflowCreateRequest,
  WorkflowRecipeDescriptor,
} from '../../../types/workflow-launch';
import { encodeWorkflowRecipeAction } from '../../../types/workflow-command.js';
import { executeCommand, executeCommandWithArg } from '../../index';
import {
  createMockCommandContext,
  type CreateMockCtxOptions,
} from '../../__tests__/test-helpers';

function run(
  workflowId: string,
  parentSessionId: string,
  updatedAt: string,
  status: WorkflowRunSummary['status'] = 'completed'
): WorkflowRunSummary {
  return {
    workflowId,
    name: workflowId,
    status,
    createdAt: updatedAt,
    updatedAt,
    parentSessionId,
  };
}

function workflowControls() {
  return {
    pauseWorkflow: mock(async (_workflowId: string) => ({
      paused: true as const,
    })),
    resumeWorkflow: mock(async (workflowId: string) => ({
      workflowId,
      status: 'running' as const,
    })),
    retryWorkflow: mock(async (workflowId: string, nodeId?: string) => ({
      workflowId,
      status: 'running' as const,
      retriedNodeIds: [nodeId ?? 'failed-step'],
    })),
    cancelWorkflow: mock(async (_workflowId: string) => ({
      ok: true,
      previousStatus: 'running' as const,
    })),
  };
}

function workflowRecipes(recipes: WorkflowRecipeDescriptor[]) {
  const listWorkflowRecipes = mock(async () => recipes);
  const createWorkflow = mock(async (_request: WorkflowCreateRequest) => ({
    workflowId: 'workflow-created',
    initialState: {
      workflowId: 'workflow-created',
      workflowName: 'created',
      status: 'running' as const,
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      root: {
        nodeId: 'root',
        type: 'sequence' as const,
        status: 'running' as const,
      },
    },
  }));
  const invokeWorkflow = mock(async (workflowId: string) => ({
    workflowId,
    status: 'running' as const,
  }));
  return { listWorkflowRecipes, createWorkflow, invokeWorkflow };
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

  it('uses list as an explicit alias for workflow history', async () => {
    const historical = run(
      'historical',
      'session-1',
      '2026-07-19T10:00:00.000Z'
    );
    const listWorkflows = mock(async () => [historical]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows,
    });

    expect(await executeCommand('/workflow list', ctx)).toBe(true);

    expect(listWorkflows).toHaveBeenCalledTimes(1);
    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalledWith(true, [
      historical,
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

  it('lists only retryable workflows for /workflow retry', async () => {
    const failed = run(
      'failed',
      'session-1',
      '2026-07-19T10:00:00.000Z',
      'failed'
    );
    const aborted = run(
      'aborted',
      'session-1',
      '2026-07-20T10:00:00.000Z',
      'aborted'
    );
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows: mock(async () => [
        failed,
        aborted,
        run('completed', 'session-1', '2026-07-21T10:00:00.000Z'),
        run('other-session', 'session-2', '2026-07-22T10:00:00.000Z', 'failed'),
      ]),
    });

    expect(await executeCommand('/workflow retry', ctx)).toBe(true);

    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalledWith(true, [
      aborted,
      failed,
    ]);
  });

  it('reports when there are no retryable workflows', async () => {
    const ctx = createKasContext({
      sessionId: 'session-1',
      listWorkflows: mock(async () => [
        run('completed', 'session-1', '2026-07-21T10:00:00.000Z'),
      ]),
    });

    expect(await executeCommand('/workflow retry', ctx)).toBe(true);

    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No failed or aborted workflows to retry.',
      'warning',
      3000
    );
    expect(ctx._spies.setShowWorkflowHistory).not.toHaveBeenCalled();
  });

  it('supplements backend history with local runs from this session', async () => {
    const remote = run('remote', 'session-1', '2026-07-19T10:00:00.000Z');
    const local = run('local', 'session-1', '2026-07-19T11:00:00.000Z');
    const ctx = createMockCommandContext({
      kasCommands: KAS_COMMANDS,
      kiro: {
        sessionId: 'session-1',
        listWorkflows: mock(async () => [remote]),
      },
      localWorkflowRuns: [
        local,
        run('other-session', 'session-2', '2026-07-19T12:00:00.000Z'),
      ],
    });
    ctx.agentEngine = 'kas';

    expect(await executeCommand('/workflow', ctx)).toBe(true);

    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalledWith(true, [
      local,
      remote,
    ]);
  });

  it('falls back to local history when the backend list RPC fails', async () => {
    const local = run('local', 'session-1', '2026-07-19T11:00:00.000Z');
    const ctx = createMockCommandContext({
      kasCommands: KAS_COMMANDS,
      kiro: {
        sessionId: 'session-1',
        listWorkflows: mock(async () => {
          throw new Error('method not found');
        }),
      },
      localWorkflowRuns: [local],
    });
    ctx.agentEngine = 'kas';

    expect(await executeCommand('/workflow', ctx)).toBe(true);

    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalledWith(true, [
      local,
    ]);
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('opens a recipe picker from the typed workflow run command', async () => {
    const workflow = workflowRecipes([
      {
        name: 'release',
        description: 'Build and validate a release',
        source: 'bundled://release',
        builtIn: true,
      },
    ]);
    const ctx = createKasContext(workflow);

    expect(await executeCommand('/workflow run', ctx)).toBe(true);

    expect(workflow.listWorkflowRecipes).toHaveBeenCalledTimes(1);
    expect(ctx._spies.setActiveCommand).toHaveBeenCalledTimes(1);
    const picker = ctx._spies.setActiveCommand!.mock.calls[0]![0] as {
      options: Array<{ label: string; value: string }>;
    };
    expect(picker.options).toHaveLength(1);
    expect(picker.options[0]?.label).toBe('release');
    expect(picker.options[0]?.value).not.toBe('release');
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
  });

  it('creates and invokes a bundled recipe for the active parent session', async () => {
    const workflow = workflowRecipes([
      {
        name: 'release',
        source: 'bundled://release',
        builtIn: true,
      },
    ]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      ...workflow,
    });

    expect(
      await executeCommand(
        '/workflow run release deploy staging --branch main',
        ctx
      )
    ).toBe(true);

    expect(workflow.createWorkflow).toHaveBeenCalledWith({
      source: { type: 'path', workflowPath: 'bundled://release' },
      inputs: { prompt: 'deploy staging', branch: 'main' },
      parentSessionId: 'session-1',
    });
    expect(workflow.invokeWorkflow).toHaveBeenCalledWith('workflow-created');
    expect(ctx._spies.announceWorkflowLifecycle).toHaveBeenCalledWith({
      workflowId: 'workflow-created',
      workflowName: 'release',
      status: 'started',
    });
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
  });

  it('runs the exact recipe selected from the picker', async () => {
    const workflow = workflowRecipes([
      {
        name: 'workspace-release',
        source: 'bundled://release',
        builtIn: true,
      },
    ]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      ...workflow,
    });

    await executeCommand('/workflow run', ctx);
    const picker = ctx._spies.setActiveCommand!.mock.calls[0]![0] as {
      options: Array<{ value: string }>;
    };
    await executeCommandWithArg('workflow', picker.options[0]!.value, ctx);

    expect(workflow.createWorkflow).toHaveBeenCalledWith({
      source: { type: 'path', workflowPath: 'bundled://release' },
      inputs: {},
      parentSessionId: 'session-1',
    });
    expect(workflow.invokeWorkflow).toHaveBeenCalledWith('workflow-created');
  });

  it('collects declared recipe inputs before launching from the picker', async () => {
    const recipe: WorkflowRecipeDescriptor = {
      name: 'release',
      description: 'Build and validate a release',
      source: 'bundled://release',
      builtIn: true,
      inputs: { target: 'prompt', branch: 'string' },
    };
    const workflow = workflowRecipes([recipe]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      ...workflow,
    });

    await executeCommand('/workflow run', ctx);
    const picker = ctx._spies.setActiveCommand!.mock.calls[0]![0] as {
      options: Array<{ value: string }>;
    };
    await executeCommandWithArg('workflow', picker.options[0]!.value, ctx);

    expect(ctx._spies.setActiveCommand).toHaveBeenLastCalledWith({
      command: expect.objectContaining({ name: '/workflow' }),
      options: [],
      panel: {
        type: 'workflow-recipe-inputs',
        recipe,
        initialValues: {},
      },
    });
    expect(workflow.createWorkflow).not.toHaveBeenCalled();

    await executeCommandWithArg(
      'workflow',
      encodeWorkflowRecipeAction({
        type: 'run',
        recipe,
        values: { target: 'staging', branch: 'main' },
      }),
      ctx
    );

    expect(workflow.createWorkflow).toHaveBeenCalledWith({
      source: { type: 'path', workflowPath: 'bundled://release' },
      inputs: { target: 'staging', branch: 'main' },
      parentSessionId: 'session-1',
    });
    expect(workflow.invokeWorkflow).toHaveBeenCalledWith('workflow-created');
  });

  it('preserves partial command-line inputs in the input form', async () => {
    const workflow = workflowRecipes([
      {
        name: 'release',
        source: 'bundled://release',
        inputs: { target: 'prompt', branch: 'string' },
      },
    ]);
    const ctx = createKasContext(workflow);

    expect(
      await executeCommand('/workflow run release --branch main', ctx)
    ).toBe(true);

    expect(ctx._spies.setActiveCommand).toHaveBeenCalledWith({
      command: expect.objectContaining({ name: '/workflow' }),
      options: [],
      panel: {
        type: 'workflow-recipe-inputs',
        recipe: {
          name: 'release',
          source: 'bundled://release',
          inputs: { target: 'prompt', branch: 'string' },
        },
        initialValues: { branch: 'main' },
      },
    });
    expect(workflow.createWorkflow).not.toHaveBeenCalled();
  });

  it('surfaces an invalid recipe selected from the picker', async () => {
    const workflow = workflowRecipes([
      {
        name: 'broken',
        source: '/workspace/.kiro/workflows/broken.workflow.json',
        validationError: 'Step build is missing an agent',
      },
    ]);
    const ctx = createKasContext(workflow);

    await executeCommand('/workflow run', ctx);
    const picker = ctx._spies.setActiveCommand!.mock.calls[0]![0] as {
      options: Array<{ value: string }>;
    };
    await executeCommandWithArg('workflow', picker.options[0]!.value, ctx);

    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Workflow recipe "broken" is invalid: Step build is missing an agent',
      'error',
      5000
    );
    expect(workflow.createWorkflow).not.toHaveBeenCalled();
    expect(ctx._spies.setShowWorkflowHistory).not.toHaveBeenCalled();
  });

  it('passes workspace recipe paths back to KAS for validation and loading', async () => {
    const source = '/workspace/.kiro/workflows/workspace.workflow.json';
    const workflow = workflowRecipes([
      { name: 'workspace', source, builtIn: false },
    ]);
    const ctx = createKasContext({
      sessionId: 'session-1',
      ...workflow,
    });

    expect(
      await executeCommand(
        "/workflow run workspace --prompt='hello world' --branch release\\ candidate",
        ctx
      )
    ).toBe(true);

    expect(workflow.createWorkflow).toHaveBeenCalledWith({
      source: { type: 'path', workflowPath: source },
      inputs: { prompt: 'hello world', branch: 'release candidate' },
      parentSessionId: 'session-1',
    });
  });

  it('surfaces MethodNotFound without falling back to chat', async () => {
    const listWorkflowRecipes = mock(async () => {
      throw new Error('Method not found');
    });
    const ctx = createKasContext({ listWorkflowRecipes });

    expect(await executeCommand('/workflow run release', ctx)).toBe(true);

    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Method not found',
      'error',
      5000
    );
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
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

    expect(await executeCommand('/workflow pause wf-pause', ctx)).toBe(true);
    expect(await executeCommand('/workflow resume wf-resume', ctx)).toBe(true);
    expect(await executeCommand('/workflow status wf-status', ctx)).toBe(true);
    expect(await executeCommand('/workflow cancel wf-cancel', ctx)).toBe(true);

    expect(controls.pauseWorkflow).toHaveBeenCalledWith('wf-pause');
    expect(controls.resumeWorkflow).toHaveBeenCalledWith('wf-resume');
    expect(inspectWorkflow).toHaveBeenCalledWith('wf-status');
    expect(controls.cancelWorkflow).toHaveBeenCalledWith('wf-cancel');
  });

  it('retries a workflow or one workflow node through the typed Kiro API', async () => {
    const controls = workflowControls();
    const ctx = createKasContext(controls);

    expect(await executeCommand('/workflow retry wf-retry', ctx)).toBe(true);
    expect(controls.retryWorkflow).toHaveBeenNthCalledWith(
      1,
      'wf-retry',
      undefined
    );
    expect(ctx._spies.showAlert).toHaveBeenLastCalledWith(
      'Retrying 1 workflow step; status is running.',
      'success',
      3000
    );

    expect(
      await executeCommand('/workflow retry wf-retry failed-node', ctx)
    ).toBe(true);
    expect(controls.retryWorkflow).toHaveBeenNthCalledWith(
      2,
      'wf-retry',
      'failed-node'
    );
  });

  it('rejects extra workflow retry arguments', async () => {
    const controls = workflowControls();
    const ctx = createKasContext(controls);

    expect(
      await executeCommand('/workflow retry wf-retry node extra', ctx)
    ).toBe(true);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Usage: /workflow retry <workflowId> [nodeId]',
      'error',
      3000
    );
    expect(controls.retryWorkflow).not.toHaveBeenCalled();
  });

  it('requires a workflow id for every control subcommand', async () => {
    for (const subcommand of ['pause', 'resume', 'status', 'cancel']) {
      const ctx = createKasContext(workflowControls());

      expect(await executeCommand(`/workflow ${subcommand}`, ctx)).toBe(true);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        `Usage: /workflow ${subcommand} <workflowId>`,
        'error',
        3000
      );
    }
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
      ...workflowRecipes([
        {
          name: 'release',
          source: 'bundled://release',
          builtIn: true,
        },
      ]),
      ...controls,
    });

    expect(await executeCommand('/workflows', ctx)).toBe(true);
    expect(await executeCommand('/workflow-run release', ctx)).toBe(true);
    expect(await executeCommand('/workflow-resume wf-resume', ctx)).toBe(true);
    expect(await executeCommand('/workflow-status wf-status', ctx)).toBe(true);
    expect(await executeCommand('/workflow-cancel wf-cancel', ctx)).toBe(true);

    expect(ctx._spies.setShowWorkflowHistory).toHaveBeenCalled();
    expect(ctx.kiro.createWorkflow).toHaveBeenCalled();
    expect(ctx.kiro.invokeWorkflow).toHaveBeenCalled();
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
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
