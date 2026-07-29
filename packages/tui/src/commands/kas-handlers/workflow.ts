import { extractRpcErrorMessage } from '../../utils/error-handling';
import type { WorkflowRunSummary } from '../../types/workflow-history';
import { shellSplit } from '../../utils/shell-split';
import type {
  WorkflowRecipeDescriptor,
  WorkflowRunSource,
} from '../../types/workflow-launch';
import {
  decodeWorkflowRecipeAction,
  encodeWorkflowRecipeAction,
} from '../../types/workflow-command.js';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import type { CommandContext } from '../types';

type WorkflowControlSubcommand = 'pause' | 'resume' | 'status' | 'cancel';

export async function handleWorkflow(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();
  if (options?.argIsSynthetic && trimmed) {
    const action = decodeWorkflowRecipeAction(trimmed);
    if (!action) {
      ctx.showAlert('Invalid workflow recipe selection.', 'error', 3000);
      return;
    }
    await prepareRecipeLaunch(
      cmd,
      ctx,
      action.recipe,
      action.type === 'run' ? action.values : {}
    );
    return;
  }

  if (!trimmed) {
    await openWorkflowHistory(ctx);
    return;
  }

  const separator = trimmed.search(/\s/);
  const subcommand = separator === -1 ? trimmed : trimmed.slice(0, separator);
  const rest = separator === -1 ? '' : trimmed.slice(separator).trim();

  switch (subcommand) {
    case 'list':
      await openWorkflowHistory(ctx);
      return;
    case 'run':
      await runWorkflowCommand(cmd, rest, ctx);
      return;
    case 'pause':
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

async function runWorkflowCommand(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  if (!args) {
    await openRecipePicker(cmd, ctx);
    return;
  }

  const separator = args.search(/\s/);
  const selector = separator === -1 ? args : args.slice(0, separator);
  const inputText = separator === -1 ? '' : args.slice(separator).trim();
  const inputs = parseWorkflowInputs(inputText);

  try {
    ctx.setLoadingMessage(`Resolving workflow ${selector}...`);
    const recipes = await ctx.kiro.listWorkflowRecipes();
    const recipe = findRecipe(recipes, selector);
    if (!recipe) {
      throw new Error(`No workflow recipe named "${selector}" was found.`);
    }
    await prepareRecipeLaunch(cmd, ctx, recipe, inputs);
  } catch (error) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(error, `Failed to run workflow ${selector}`),
      'error',
      5000
    );
  }
}

async function openRecipePicker(
  cmd: KasCommand,
  ctx: CommandContext
): Promise<void> {
  try {
    ctx.setLoadingMessage('Loading workflow recipes...');
    const recipes = await ctx.kiro.listWorkflowRecipes();
    ctx.setLoadingMessage(null);
    if (recipes.length === 0) {
      ctx.showAlert(
        'No workflow recipes available in this workspace.',
        'warning',
        3000
      );
      return;
    }

    ctx.setActiveCommand({
      command: cmd,
      options: recipes.map((recipe) => ({
        value: encodeWorkflowRecipeAction({
          type: 'select',
          recipe: serializableRecipe(recipe),
        }),
        label: recipe.validationError ? `Invalid: ${recipe.name}` : recipe.name,
        description:
          recipe.validationError ?? recipe.description ?? recipe.source ?? '',
      })),
    });
  } catch (error) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(error, 'Failed to list workflow recipes'),
      'error',
      5000
    );
  }
}

async function prepareRecipeLaunch(
  cmd: KasCommand,
  ctx: CommandContext,
  recipe: WorkflowRecipeDescriptor,
  inputs: Record<string, string>
): Promise<void> {
  if (recipe.validationError) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      `Workflow recipe "${recipe.name}" is invalid: ${recipe.validationError}`,
      'error',
      5000
    );
    return;
  }

  const declaredInputs = Object.keys(recipe.inputs ?? {});
  const missingInput = declaredInputs.find((name) => !inputs[name]?.trim());
  if (missingInput) {
    ctx.setLoadingMessage(null);
    ctx.setActiveCommand({
      command: cmd,
      options: [],
      panel: {
        type: 'workflow-recipe-inputs',
        recipe: serializableRecipe(recipe),
        initialValues: inputs,
      },
    });
    return;
  }

  await runRecipe(ctx, recipe, inputs);
}

async function runRecipe(
  ctx: CommandContext,
  recipe: WorkflowRecipeDescriptor,
  inputs: Record<string, string>
): Promise<void> {
  if (recipe.validationError) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      `Workflow recipe "${recipe.name}" is invalid: ${recipe.validationError}`,
      'error',
      5000
    );
    return;
  }

  try {
    ctx.setLoadingMessage(`Starting workflow ${recipe.name}...`);
    const source = await resolveRunSource(recipe);
    const created = await ctx.kiro.createWorkflow({
      source,
      inputs,
      ...(ctx.kiro.sessionId ? { parentSessionId: ctx.kiro.sessionId } : {}),
    });
    await ctx.kiro.invokeWorkflow(created.workflowId);
    ctx.setLoadingMessage(null);
    ctx.announceWorkflowLifecycle({
      workflowId: created.workflowId,
      workflowName: recipe.name,
      status: 'started',
    });
  } catch (error) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(error, `Failed to run workflow ${recipe.name}`),
      'error',
      5000
    );
  }
}

async function resolveRunSource(
  recipe: WorkflowRecipeDescriptor
): Promise<WorkflowRunSource> {
  const source = recipe.source?.trim();
  if (recipe.builtIn === true || !source || source.startsWith('bundled://')) {
    return {
      type: 'path',
      workflowPath:
        source?.startsWith('bundled://') === true
          ? source
          : `bundled://${recipe.name}`,
    };
  }

  return {
    type: 'path',
    workflowPath: source,
  };
}

function findRecipe(
  recipes: readonly WorkflowRecipeDescriptor[],
  selector: string
): WorkflowRecipeDescriptor | undefined {
  return recipes.find(
    (recipe) =>
      recipe.name === selector ||
      recipe.source === selector ||
      recipe.source === `bundled://${selector}`
  );
}

function serializableRecipe(
  recipe: WorkflowRecipeDescriptor
): WorkflowRecipeDescriptor {
  return {
    name: recipe.name,
    ...(recipe.description === undefined
      ? {}
      : { description: recipe.description }),
    ...(recipe.source === undefined ? {} : { source: recipe.source }),
    ...(recipe.builtIn === undefined ? {} : { builtIn: recipe.builtIn }),
    ...(recipe.validationError === undefined
      ? {}
      : { validationError: recipe.validationError }),
    ...(recipe.inputs === undefined ? {} : { inputs: recipe.inputs }),
  };
}

/** Parse `--key=value`, `--key value`, and a free-form prompt tail. */
function parseWorkflowInputs(value: string): Record<string, string> {
  if (!value) return {};
  const tokens = shellSplit(value);
  const inputs: Record<string, string> = {};
  const prompt: string[] = [];
  const flag = /^--([a-zA-Z][a-zA-Z0-9_-]*)(?:=(.*))?$/;

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index]!;
    const match = token.match(flag);
    if (!match) {
      prompt.push(token);
      continue;
    }

    const key = match[1]!;
    let inputValue = match[2];
    const next = tokens[index + 1];
    if (inputValue === undefined && next && !next.startsWith('--')) {
      inputValue = next;
      index += 1;
    }
    inputs[key] = inputValue ?? '';
  }

  if (prompt.length > 0 && inputs.prompt === undefined) {
    inputs.prompt = prompt.join(' ');
  }
  return inputs;
}

async function openWorkflowHistory(ctx: CommandContext): Promise<void> {
  const sessionId = ctx.kiro.sessionId;
  const localRuns = ctx.getLocalWorkflowRuns();
  let remoteRuns: WorkflowRunSummary[] = [];
  try {
    remoteRuns = await ctx.kiro.listWorkflows();
  } catch (error) {
    if (localRuns.length === 0) {
      ctx.showAlert(
        extractRpcErrorMessage(error, 'Failed to list workflow history'),
        'error',
        5000
      );
      return;
    }
  }

  const runsById = new Map(
    localRuns.map((run) => [run.workflowId, run] as const)
  );
  for (const run of remoteRuns) runsById.set(run.workflowId, run);
  const scopedRuns = sessionId
    ? [...runsById.values()]
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

    if (subcommand === 'pause') {
      await ctx.kiro.pauseWorkflow(id);
      ctx.showAlert('Workflow paused', 'success', 3000);
    } else if (subcommand === 'resume') {
      await ctx.kiro.resumeWorkflow(id);
      ctx.showAlert('Workflow resumed', 'success', 3000);
    } else {
      await ctx.kiro.cancelWorkflow(id);
      ctx.showAlert('Workflow cancelled', 'success', 3000);
    }
  } catch (error) {
    const action = subcommand === 'status' ? 'inspect' : subcommand;
    ctx.showAlert(
      extractRpcErrorMessage(error, `Failed to ${action} workflow ${id}`),
      'error',
      5000
    );
  }
}
