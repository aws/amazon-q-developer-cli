import { extractRpcErrorMessage } from '../../utils/error-handling';
import type { WorkflowRunSummary } from '../../types/workflow-history';
import { isRetryableWorkflowStatus } from '../../types/workflow-status.js';
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
    case 'new':
      await createWorkflowFromDescription(ctx, rest);
      return;
    case 'list':
      await openWorkflowHistory(ctx);
      return;
    case 'run':
      await runWorkflowCommand(cmd, rest, ctx);
      return;
    case 'retry':
      await retryWorkflowCommand(ctx, rest);
      return;
    case 'pause':
    case 'resume':
    case 'status':
    case 'cancel':
      await controlWorkflow(ctx, subcommand, rest);
      return;
    default:
      ctx.showAlert(
        `Unknown /workflow subcommand: ${subcommand}. Try run, list, or new.`,
        'error',
        3000
      );
  }
}

/**
 * Bundled KAS subagent that authors workflow definitions. Registered in the
 * agent's `CustomAgentRegistry`, so the top-level chat model can reach it
 * through its delegation tool — we don't (and can't) switch the session's
 * agent to it, we just name it so the model delegates instead of improvising.
 * If it isn't registered the delegation simply won't resolve and the model
 * falls back to authoring the file itself, which the prompt also covers.
 */
const WORKFLOW_CREATOR_AGENT = 'wf-workflow-creator';

/**
 * `/workflow new <description>` — mirror of `/spec new`: hand the description
 * to the agent and ask it to author a reusable recipe.
 *
 * Three things this prompt has to get right, all learned the slow way:
 *
 * 1. Delegate to `wf-workflow-creator`, and delegate *immediately*. It owns
 *    the workflow schema and validates before returning, so it produces
 *    launchable recipes where the chat model guessing at the schema produces
 *    invalid ones. Without the "first action" directive the chat model burns
 *    a long thinking pass planning the workflow itself before handing it off.
 * 2. Ask the creator to return the *JSON*, not its native output. The
 *    creator's only tool is `save_workflow_definition`, which stores the
 *    definition server-side and returns a single-use `generated://<id>` ref
 *    the orchestrator cannot read — a ref-only reply forces the chat model
 *    to reconstruct the whole definition from the creator's summary. The
 *    parent still emits the JSON once as the write's arguments; copying
 *    verbatim removes the reconstruction reasoning, not the output tokens.
 * 3. Ask for a *file*. `/workflow run` and the recipe picker read
 *    `<name>.workflow.json` from `.kiro/workflows/`, so the returned JSON
 *    must be written there verbatim — already validated, so rewriting or
 *    re-validating it only adds tokens and thinking time.
 */
async function createWorkflowFromDescription(
  ctx: CommandContext,
  description: string
): Promise<void> {
  const goal = description.trim();
  if (!goal) {
    ctx.showAlert('Usage: /workflow new <description>', 'error', 4000);
    return;
  }

  const prompt = [
    `Create a reusable workflow recipe for this goal: ${goal}`,
    '',
    `Your FIRST action must be delegating to the \`${WORKFLOW_CREATOR_AGENT}\``,
    'agent — do not investigate the codebase, plan the workflow yourself, or',
    'write anything before delegating. It owns the workflow schema. Pass it',
    'the goal verbatim (plus any context I gave above) and these',
    'instructions: author the workflow, validate it with',
    'save_workflow_definition, then return the complete validated workflow',
    'JSON as the final response — the JSON itself, not just a `generated://`',
    'reference, because the caller cannot read server-side references.',
    'Declare any inputs the recipe needs in its `inputs` block, and pass',
    'step data through `{{...}}` references in each step `prompt` — there',
    'is no step-level `input` field.',
    '',
    'When the delegation returns, write the returned JSON verbatim to',
    '`.kiro/workflows/<name>.workflow.json` in the workspace (kebab-case',
    "`<name>` from the workflow's `name` field). It is already validated —",
    'do not edit, reformat, or re-validate it.',
    '',
    `Only if \`${WORKFLOW_CREATOR_AGENT}\` is unavailable: author the`,
    'definition yourself under the same constraints, validate it with the',
    'validate_workflow tool, and write it to the same path.',
    '',
    'End with one short sentence naming the recipe and that I can launch it',
    'with `/workflow run <name>`.',
  ].join('\n');

  await ctx.sendMessage(prompt, undefined, `/workflow new ${goal}`);
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
      // U02: surface the naming convention so users can discover how to add
      // recipes when none are found.
      ctx.showAlert(
        'No workflow recipes available in this workspace. Recipes must be named <name>.workflow.json in .kiro/workflows/',
        'warning',
        5000
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
    // #9 client stopgap: forward a concrete session model so KAS's parentModelId
    // cascade resolves to it. Skip the literal 'auto' (and unset) — there is
    // nothing concrete to pass, and forwarding 'auto' is what the backend rejects.
    const modelId = ctx.getCurrentModel?.()?.id;
    const concreteModelId = modelId && modelId !== 'auto' ? modelId : undefined;
    const created = await ctx.kiro.createWorkflow({
      source,
      inputs,
      ...(ctx.kiro.sessionId ? { parentSessionId: ctx.kiro.sessionId } : {}),
      ...(concreteModelId ? { modelId: concreteModelId } : {}),
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

async function retryWorkflowCommand(
  ctx: CommandContext,
  args: string
): Promise<void> {
  if (!args) {
    await openWorkflowHistory(ctx, true);
    return;
  }

  const [workflowId, nodeId, ...extra] = shellSplit(args);
  if (!workflowId || extra.length > 0) {
    ctx.showAlert(
      'Usage: /workflow retry <workflowId> [nodeId]',
      'error',
      3000
    );
    return;
  }

  try {
    const response = await ctx.kiro.retryWorkflow(workflowId, nodeId);
    const count = response.retriedNodeIds.length;
    ctx.showAlert(
      `Retrying ${count} workflow ${count === 1 ? 'step' : 'steps'}; status is ${response.status}.`,
      'success',
      3000
    );
  } catch (error) {
    ctx.showAlert(
      extractRpcErrorMessage(error, `Failed to retry workflow ${workflowId}`),
      'error',
      5000
    );
  }
}

async function openWorkflowHistory(
  ctx: CommandContext,
  retryableOnly = false
): Promise<void> {
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
        .filter(
          (run) =>
            run.parentSessionId === sessionId &&
            (!retryableOnly || isRetryableWorkflowStatus(run.status))
        )
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    : [];

  if (scopedRuns.length === 0) {
    ctx.showAlert(
      retryableOnly
        ? 'No failed or aborted workflows to retry.'
        : 'No workflows in this session yet. Start one with /workflow run.',
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
      const response = await ctx.kiro.resumeWorkflow(id);
      ctx.showAlert(
        `Workflow is ${response.status}`,
        response.status === 'running' ? 'success' : 'warning',
        3000
      );
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
