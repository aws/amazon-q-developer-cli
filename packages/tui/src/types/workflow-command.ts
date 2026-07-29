import type { WorkflowRecipeDescriptor } from './workflow-launch.js';

const RECIPE_ACTION_PREFIX = '__kiro_workflow_recipe__:';

export interface WorkflowRecipeInputPanelModel {
  type: 'workflow-recipe-inputs';
  recipe: WorkflowRecipeDescriptor;
  initialValues: Record<string, string>;
}

export type WorkflowRecipeCommandAction =
  | {
      type: 'select';
      recipe: WorkflowRecipeDescriptor;
    }
  | {
      type: 'run';
      recipe: WorkflowRecipeDescriptor;
      values: Record<string, string>;
    };

export function encodeWorkflowRecipeAction(
  action: WorkflowRecipeCommandAction
): string {
  return `${RECIPE_ACTION_PREFIX}${encodeURIComponent(JSON.stringify(action))}`;
}

export function decodeWorkflowRecipeAction(
  value: string
): WorkflowRecipeCommandAction | null {
  if (!value.startsWith(RECIPE_ACTION_PREFIX)) return null;

  try {
    const decoded: unknown = JSON.parse(
      decodeURIComponent(value.slice(RECIPE_ACTION_PREFIX.length))
    );
    if (!isRecord(decoded) || !isRecipe(decoded.recipe)) return null;

    if (decoded.type === 'select') {
      return { type: 'select', recipe: decoded.recipe };
    }
    if (decoded.type === 'run' && isStringMap(decoded.values)) {
      return {
        type: 'run',
        recipe: decoded.recipe,
        values: decoded.values,
      };
    }
    return null;
  } catch {
    return null;
  }
}

function isRecipe(value: unknown): value is WorkflowRecipeDescriptor {
  return (
    isRecord(value) &&
    isNonEmptyString(value.name) &&
    isOptionalString(value.description) &&
    isOptionalString(value.source) &&
    isOptionalBoolean(value.builtIn) &&
    isOptionalString(value.validationError) &&
    (value.inputs === undefined || isStringMap(value.inputs))
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === 'string';
}

function isOptionalBoolean(value: unknown): value is boolean | undefined {
  return value === undefined || typeof value === 'boolean';
}

function isStringMap(value: unknown): value is Record<string, string> {
  return (
    isRecord(value) &&
    Object.values(value).every((entry) => typeof entry === 'string')
  );
}
