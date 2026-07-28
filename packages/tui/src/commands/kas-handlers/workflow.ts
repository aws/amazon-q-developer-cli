import { extractRpcErrorMessage } from '../../utils/error-handling';
import type { WorkflowRunSummary } from '../../types/workflow-history';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import type { CommandContext } from '../types';

type WorkflowControlSubcommand = 'resume' | 'status' | 'cancel';

export async function handleWorkflow(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();
  if (!trimmed) {
    await openWorkflowHistory(ctx);
    return;
  }

  const separator = trimmed.search(/\s/);
  const subcommand = separator === -1 ? trimmed : trimmed.slice(0, separator);
  const rest = separator === -1 ? '' : trimmed.slice(separator).trim();

  switch (subcommand) {
    case 'list':
    case 'run':
      await ctx.sendMessage(`/workflow ${trimmed}`);
      return;
    case 'resume':
    case 'status':
    case 'cancel':
      await controlWorkflow(ctx, subcommand, rest);
      return;
    default:
      ctx.showAlert(
        `Unknown /workflow subcommand: ${subcommand}`,
        'error',
        3000
      );
  }
}

async function openWorkflowHistory(ctx: CommandContext): Promise<void> {
  const sessionId = ctx.kiro.sessionId;
  let runs: WorkflowRunSummary[];
  try {
    runs = await ctx.kiro.listWorkflows();
  } catch (error) {
    ctx.showAlert(
      extractRpcErrorMessage(error, 'Failed to list workflow history'),
      'error',
      5000
    );
    return;
  }

  const scopedRuns = sessionId
    ? runs
        .filter((run) => run.parentSessionId === sessionId)
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    : [];

  if (scopedRuns.length === 0) {
    ctx.showAlert(
      'No workflows in this session yet. Start one with /workflow run.',
      'warning',
      3000
    );
    return;
  }

  if (!ctx.setShowWorkflowHistory) {
    ctx.showAlert('Workflow history view is unavailable.', 'error', 3000);
    return;
  }

  ctx.setShowWorkflowHistory(true, scopedRuns);
}

async function controlWorkflow(
  ctx: CommandContext,
  subcommand: WorkflowControlSubcommand,
  workflowId: string
): Promise<void> {
  const id = workflowId.trim();
  if (!id) {
    ctx.showAlert(`Usage: /workflow ${subcommand} <workflowId>`, 'error', 3000);
    return;
  }

  try {
    if (subcommand === 'status') {
      const run = await ctx.kiro.inspectWorkflow(id);
      ctx.showAlert(
        `Workflow "${run.state.workflowName}" is ${run.state.status}.`,
        'success',
        3000
      );
      return;
    }

    if (subcommand === 'resume') {
      await ctx.kiro.resumeWorkflow(id);
      ctx.showAlert('Workflow resumed', 'success', 3000);
    } else {
      await ctx.kiro.cancelWorkflow(id);
      ctx.showAlert('Workflow cancelled', 'success', 3000);
    }
  } catch (error) {
    const action =
      subcommand === 'status'
        ? 'inspect'
        : subcommand === 'resume'
          ? 'resume'
          : 'cancel';
    ctx.showAlert(
      extractRpcErrorMessage(error, `Failed to ${action} workflow ${id}`),
      'error',
      5000
    );
  }
}
