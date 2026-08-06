import type {
  WorkflowNodeDescriptor,
  WorkflowStateSnapshot,
  WorkflowStatus,
} from './workflow.js';

export type WorkflowJsonPrimitive = string | number | boolean | null;
export type WorkflowJsonValue =
  | WorkflowJsonPrimitive
  | WorkflowJsonValue[]
  | { [key: string]: WorkflowJsonValue };
export type WorkflowDefinition = { [key: string]: WorkflowJsonValue };

/** Recipe descriptor returned by `_kiro/workflow/listRecipes`. */
export interface WorkflowRecipeDescriptor {
  name: string;
  description?: string;
  source?: string;
  builtIn?: boolean;
  validationError?: string;
  inputs?: Record<string, string>;
  plan?: WorkflowNodeDescriptor[];
}

export interface WorkflowRecipeListResponse {
  recipes: WorkflowRecipeDescriptor[];
}

export type WorkflowRunSource =
  | { type: 'path'; workflowPath: string }
  | { type: 'inline'; workflow: WorkflowDefinition };

export interface WorkflowCreateRequest {
  source: WorkflowRunSource;
  inputs: Record<string, string>;
  parentSessionId?: string;
  /**
   * Concrete model id to seed the run with. Client stopgap for #9: when the
   * session has a real model selected we forward it so KAS's `parentModelId`
   * cascade resolves to that id instead of falling through to the literal
   * `'auto'` (which the backend rejects with INVALID_MODEL_ID). Omitted when
   * the session model is itself `'auto'`/unset — nothing concrete to pass.
   */
  modelId?: string;
}

export interface WorkflowCreateResponse {
  workflowId: string;
  initialState: WorkflowStateSnapshot;
}

export interface WorkflowInvokeResponse {
  workflowId: string;
  status: WorkflowStatus;
}

/** Typed launch plane for KAS workflow recipes and runs. */
export interface WorkflowLaunchApi {
  listRecipes(
    workspacePaths: readonly string[]
  ): Promise<WorkflowRecipeDescriptor[]>;
  createRun(request: WorkflowCreateRequest): Promise<WorkflowCreateResponse>;
  invokeRun(workflowId: string): Promise<WorkflowInvokeResponse>;
}
