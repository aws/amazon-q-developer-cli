import type {
  AgentNotificationMethod,
  WorkflowStepsQueuedResolution,
} from '@kiro/acp-type-covenant';
import type {
  WorkflowEvent,
  WorkflowJoinPolicy,
  WorkflowLoadResponse,
  WorkflowMaxIterationPolicy,
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeType,
  WorkflowStepSessionRef,
  WorkflowStateSnapshot,
  WorkflowWatchOutcome,
} from '../../../types/workflow.js';
import {
  isRunCompleteWorkflowStatus,
  isWorkflowNodeStatus,
  isWorkflowStatus,
} from '../../../types/workflow-status.js';
import { logger } from '../../../utils/logger.js';
import type {
  WorkflowActionAttribution,
  WorkflowCancelResponse,
  WorkflowInspectResponse,
  WorkflowListResponse,
  WorkflowPauseResponse,
  WorkflowRetryResponse,
  WorkflowResumeResponse,
  WorkflowRunSummary,
} from '../../../types/workflow-history.js';
import type {
  WorkflowCreateRequest,
  WorkflowCreateResponse,
  WorkflowInvokeResponse,
  WorkflowRecipeDescriptor,
  WorkflowRecipeListResponse,
} from '../../../types/workflow-launch.js';
import type { NotificationContract, RpcContract } from '../runtime.js';

const WORKFLOW_METHOD_PREFIX = '_kiro/workflow/';
const MAX_TREE_DEPTH = 100;

type CovenantWorkflowNotificationMethod = Extract<
  AgentNotificationMethod,
  `_kiro/workflow/${string}`
>;

const WORKFLOW_NOTIFICATION_METHOD_SET = {
  '_kiro/workflow/run_start': true,
  '_kiro/workflow/node_start': true,
  '_kiro/workflow/node_complete': true,
  '_kiro/workflow/node_paused': true,
  '_kiro/workflow/loop_iteration': true,
  '_kiro/workflow/watch_poll': true,
  '_kiro/workflow/paused': true,
  '_kiro/workflow/run_complete': true,
  '_kiro/workflow/steps_queued': true,
} as const satisfies Record<CovenantWorkflowNotificationMethod, true>;

export type WorkflowNotificationMethod =
  keyof typeof WORKFLOW_NOTIFICATION_METHOD_SET;

export const WORKFLOW_NOTIFICATION_METHODS: readonly WorkflowNotificationMethod[] =
  Object.keys(WORKFLOW_NOTIFICATION_METHOD_SET) as WorkflowNotificationMethod[];

const EVENT_TYPES: ReadonlySet<WorkflowEvent['type']> = new Set([
  'run_start',
  'node_start',
  'node_complete',
  'node_paused',
  'loop_iteration',
  'watch_poll',
  'paused',
  'run_complete',
  'steps_queued',
]);

const NODE_TYPES: ReadonlySet<WorkflowNodeType> = new Set([
  'step',
  'sequence',
  'repeat',
  'parallel',
  'watch',
]);

const COMPLETION_SIGNALS = new Set(['success', 'need_input', 'error']);
const COMPLETION_SIGNAL_SOURCES = new Set(['send_message', 'status_update']);

/**
 * Whether the connected agent serves the workflow control plane. Silence counts
 * as capable, since only newer KAS builds publish the list and refusing would
 * break every agent predating it. A non-empty list that lacks the methods is a
 * real answer, and we honor it.
 */
export function agentServesWorkflowControl(
  extensionMethods: readonly string[] | undefined
): boolean {
  if (extensionMethods === undefined || extensionMethods.length === 0) {
    return true;
  }
  return WORKFLOW_CONTROL_METHODS.every((method) =>
    extensionMethods.includes(method)
  );
}

/** `'user'` is the only initiator KAS attributes today. */
const INITIATORS = new Set(['user']);

/**
 * Drop an optional enum-valued field whose value this build doesn't recognise.
 *
 * The rule for this boundary: an unknown enum value degrades the *field*, never
 * the enclosing event. KAS can legitimately add a value ahead of the pinned
 * covenant, and one unrecognised node would otherwise invalidate the whole
 * snapshot — losing a hint field costs a label, losing the event costs the run's
 * lifecycle. Mutates in place; every caller owns a freshly parsed payload.
 *
 * Logged at debug: one unknown value in a plan reaches here once per field per
 * node, so warning turns a single upgrade into a wall of duplicate output.
 */
function degradeUnknownEnum(
  value: Record<string, unknown>,
  key: string,
  allowed: ReadonlySet<string>
): boolean {
  const candidate = value[key];
  if (candidate === undefined) return false;
  if (typeof candidate === 'string' && allowed.has(candidate)) return false;
  logger.debug('[acp-client] Ignored unknown KAS workflow enum value', {
    field: key,
    value: String(candidate),
  });
  delete value[key];
  return true;
}

/**
 * An attribution is the initiator *and* its reason, so degrade them together.
 * They resolve through independent `??` chains downstream, so dropping only the
 * unknown initiator lets the reason survive and attach to whatever initiator was
 * carried forward — a scheduler's "nightly budget exceeded" rendered as
 * "Stopped by you: nightly budget exceeded". No attribution beats a wrong one.
 */
function degradeUnknownInitiator(
  value: Record<string, unknown>,
  initiatorKey: string,
  reasonKey: string
): void {
  if (degradeUnknownEnum(value, initiatorKey, INITIATORS)) {
    delete value[reasonKey];
  }
}
const JOIN_POLICIES: ReadonlySet<WorkflowJoinPolicy> = new Set([
  'all',
  'allSettled',
  'any',
]);
const ON_MAX_ITERATIONS: ReadonlySet<WorkflowMaxIterationPolicy> = new Set([
  'abort',
  'continue',
  'pause',
]);
const WATCH_OUTCOMES: ReadonlySet<WorkflowWatchOutcome> = new Set([
  'idle',
  'new-activity',
  'terminal-state',
]);

interface WorkflowNotificationMetadata {
  kind?: unknown;
  workflowId?: unknown;
  eventType?: unknown;
  notifyId?: unknown;
}

interface WorkflowKiroMetadata {
  kind?: unknown;
  messageId?: unknown;
  notification?: WorkflowNotificationMetadata;
}

export interface ParsedWorkflowProgress {
  event: WorkflowEvent;
  messageId?: string;
}

export type PersistedWorkflowProgressParseResult =
  | { kind: 'not-workflow' }
  | { kind: 'invalid-workflow'; messageId?: string }
  | { kind: 'workflow-progress'; progress: ParsedWorkflowProgress };

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isOptionalString(value: unknown): boolean {
  return value === undefined || typeof value === 'string';
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

function isOptionalNonEmptyString(value: unknown): boolean {
  return value === undefined || isNonEmptyString(value);
}

function isOptionalNumber(value: unknown): boolean {
  return (
    value === undefined || (typeof value === 'number' && Number.isFinite(value))
  );
}

function isOptionalBoolean(value: unknown): boolean {
  return value === undefined || typeof value === 'boolean';
}

function isOptionalStringArray(value: unknown): boolean {
  return (
    value === undefined ||
    (Array.isArray(value) &&
      value.every((segment) => typeof segment === 'string'))
  );
}

function isNonEmptyStringArray(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.length > 0 &&
    value.every((segment) => typeof segment === 'string')
  );
}

function isOptionalStringMap(value: unknown): boolean {
  return (
    value === undefined ||
    (isRecord(value) &&
      Object.values(value).every((entry) => typeof entry === 'string'))
  );
}

function isOptionalNonNegativeInteger(value: unknown): boolean {
  return (
    value === undefined ||
    (typeof value === 'number' && Number.isInteger(value) && value >= 0)
  );
}

function isWorkflowStepsQueuedResolution(
  value: unknown
): value is WorkflowStepsQueuedResolution {
  return (
    value === undefined ||
    (isRecord(value) &&
      (value.outcome === 'applied' ||
        value.outcome === 'rejected' ||
        value.outcome === 'dropped') &&
      isOptionalString(value.reason))
  );
}

function isWorkflowStopCondition(value: unknown): boolean {
  if (!isRecord(value)) return false;
  const fileCheck = value.fileCheck;
  const validFileCheck =
    fileCheck === undefined ||
    (isRecord(fileCheck) &&
      typeof fileCheck.path === 'string' &&
      typeof fileCheck.jsonPath === 'string' &&
      Object.hasOwn(fileCheck, 'value'));
  const hasCondition =
    typeof value.containsText === 'string' ||
    fileCheck !== undefined ||
    COMPLETION_SIGNALS.has(value.completionSignal as string);
  return (
    isOptionalString(value.containsText) &&
    validFileCheck &&
    (value.completionSignal === undefined ||
      COMPLETION_SIGNALS.has(value.completionSignal as string)) &&
    hasCondition
  );
}

function isWorkflowNodeDescriptor(
  value: unknown,
  depth = 0
): value is WorkflowNodeDescriptor {
  if (!isRecord(value) || depth > MAX_TREE_DEPTH) return false;
  if (
    typeof value.nodeId !== 'string' ||
    !NODE_TYPES.has(value.type as WorkflowNodeType) ||
    !isOptionalString(value.agentName) ||
    !isOptionalString(value.modelId) ||
    !isOptionalString(value.effortLevel) ||
    (value.joinPolicy !== undefined &&
      !JOIN_POLICIES.has(value.joinPolicy as WorkflowJoinPolicy)) ||
    (value.maxIterations !== undefined &&
      (typeof value.maxIterations !== 'number' ||
        !Number.isInteger(value.maxIterations) ||
        value.maxIterations <= 0)) ||
    (value.stopCondition !== undefined &&
      !isWorkflowStopCondition(value.stopCondition)) ||
    !isOptionalString(value.stopWhen) ||
    (value.onMaxIterations !== undefined &&
      !ON_MAX_ITERATIONS.has(
        value.onMaxIterations as WorkflowMaxIterationPolicy
      ))
  ) {
    return false;
  }
  for (const key of ['steps', 'branches'] as const) {
    const children = value[key];
    if (
      children !== undefined &&
      (!Array.isArray(children) ||
        !children.every((child) => isWorkflowNodeDescriptor(child, depth + 1)))
    ) {
      return false;
    }
  }
  return true;
}

function isWorkflowNodeState(
  value: unknown,
  depth = 0
): value is WorkflowNodeState {
  if (!isRecord(value) || depth > MAX_TREE_DEPTH) return false;
  if (
    typeof value.nodeId !== 'string' ||
    !NODE_TYPES.has(value.type as WorkflowNodeType) ||
    !isWorkflowNodeStatus(value.status) ||
    !isOptionalString(value.agentName) ||
    !isOptionalString(value.modelId) ||
    !isOptionalString(value.effortLevel) ||
    !isOptionalString(value.sessionId) ||
    !isOptionalString(value.startedAt) ||
    !isOptionalString(value.endedAt) ||
    !isOptionalNonNegativeInteger(value.iteration) ||
    !isOptionalString(value.branchId) ||
    !isOptionalStringMap(value.artifacts) ||
    !isOptionalString(value.capturedOutput) ||
    !isOptionalBoolean(value.watchTerminal) ||
    !isOptionalString(value.failureReason) ||
    !isOptionalNonNegativeInteger(value.continuationAttempts)
  ) {
    return false;
  }
  degradeUnknownEnum(value, 'completionSignal', COMPLETION_SIGNALS);
  degradeUnknownEnum(
    value,
    'completionSignalSource',
    COMPLETION_SIGNAL_SOURCES
  );
  return (
    value.children === undefined ||
    (Array.isArray(value.children) &&
      value.children.every((child) => isWorkflowNodeState(child, depth + 1)))
  );
}

function isWorkflowStateSnapshot(
  value: unknown,
  envelopeParentSessionId?: string
): value is WorkflowStateSnapshot {
  if (!isRecord(value)) return false;
  const root = value.root;
  if (!isWorkflowNodeState(root)) return false;
  degradeUnknownInitiator(value, 'stopInitiator', 'stopReason');
  const structurallyValid =
    isNonEmptyString(value.workflowId) &&
    typeof value.workflowName === 'string' &&
    isWorkflowStatus(value.status) &&
    isOptionalStringMap(value.inputs) &&
    value.inputs !== undefined &&
    isOptionalStringMap(value.artifacts) &&
    value.artifacts !== undefined &&
    isOptionalStringMap(value.capturedOutputs) &&
    value.capturedOutputs !== undefined &&
    isOptionalString(value.pauseReason) &&
    isOptionalString(value.stopReason) &&
    isOptionalString(value.parentSessionId) &&
    isOptionalString(value.workspacePath) &&
    isOptionalStringArray(value.additionalDirectories) &&
    isOptionalString(value.createdAt) &&
    isOptionalString(value.parentModelId) &&
    isOptionalString(value.modelId) &&
    isOptionalString(value.parentEffortLevel) &&
    isOptionalString(value.effortLevel) &&
    isOptionalNonNegativeInteger(value.planRevision);
  if (!structurallyValid) return false;

  const stateParentSessionId = value.parentSessionId;
  if (!isOptionalNonEmptyString(stateParentSessionId)) return false;
  const effectiveParentSessionId =
    stateParentSessionId ?? envelopeParentSessionId;

  const sessionIds = new Set<string>();
  const visit = (node: WorkflowNodeState): boolean => {
    if (node.sessionId !== undefined) {
      if (
        !isNonEmptyString(node.sessionId) ||
        node.sessionId === effectiveParentSessionId ||
        sessionIds.has(node.sessionId)
      ) {
        return false;
      }
      sessionIds.add(node.sessionId);
    }
    return node.children?.every(visit) ?? true;
  };
  return visit(root);
}

function isWorkflowStepSessionRef(
  value: unknown
): value is WorkflowStepSessionRef {
  return (
    isRecord(value) &&
    typeof value.nodeId === 'string' &&
    typeof value.sessionId === 'string' &&
    value.sessionId.length > 0 &&
    isNonEmptyStringArray(value.nodePath) &&
    isOptionalNonNegativeInteger(value.iteration) &&
    isOptionalString(value.branchId)
  );
}

function hasWorkflowId(payload: Record<string, unknown>): boolean {
  return (
    typeof payload.workflowId === 'string' && payload.workflowId.length > 0
  );
}

function hasOptionalParent(payload: Record<string, unknown>): boolean {
  return isOptionalNonEmptyString(payload.parentSessionId);
}

function isCanonicalRunComplete(payload: Record<string, unknown>): boolean {
  const envelopeParentSessionId =
    typeof payload.parentSessionId === 'string'
      ? payload.parentSessionId
      : undefined;
  degradeUnknownInitiator(payload, 'initiator', 'initiatorReason');
  if (
    !isRunCompleteWorkflowStatus(payload.status) ||
    !isOptionalString(payload.initiatorReason) ||
    !isWorkflowStateSnapshot(payload.finalState, envelopeParentSessionId)
  ) {
    return false;
  }
  const finalState = payload.finalState;
  return (
    finalState.workflowId === payload.workflowId &&
    finalState.status === payload.status &&
    (payload.parentSessionId === undefined ||
      finalState.parentSessionId === undefined ||
      payload.parentSessionId === finalState.parentSessionId)
  );
}

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'isWorkflowEvent' has a complexity of 50. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
function isWorkflowEvent(value: unknown): value is WorkflowEvent {
  if (!isRecord(value)) return false;
  const payload = value;
  const type = payload.type;
  if (!EVENT_TYPES.has(type as WorkflowEvent['type'])) return false;
  if (!hasWorkflowId(payload) || !hasOptionalParent(payload)) return false;

  switch (type) {
    case 'run_start':
      return (
        typeof payload.workflowName === 'string' &&
        isOptionalStringMap(payload.inputs) &&
        payload.inputs !== undefined &&
        Array.isArray(payload.nodeTree) &&
        payload.nodeTree.every((node) => isWorkflowNodeDescriptor(node))
      );
    case 'node_start':
      return (
        typeof payload.nodeId === 'string' &&
        NODE_TYPES.has(payload.nodeType as WorkflowNodeType) &&
        isNonEmptyStringArray(payload.nodePath) &&
        isOptionalString(payload.agentName) &&
        isOptionalString(payload.prompt) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNonNegativeInteger(payload.iteration) &&
        isOptionalString(payload.branchId)
      );
    case 'node_complete':
      return (
        typeof payload.nodeId === 'string' &&
        isWorkflowNodeStatus(payload.status) &&
        isNonEmptyStringArray(payload.nodePath) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNonNegativeInteger(payload.iteration) &&
        isOptionalString(payload.branchId) &&
        isOptionalStringMap(payload.artifacts) &&
        isOptionalString(payload.capturedOutput) &&
        isOptionalNumber(payload.durationSecs) &&
        isOptionalString(payload.failureReason)
      );
    case 'node_paused':
      return (
        typeof payload.nodeId === 'string' &&
        typeof payload.reason === 'string' &&
        isNonEmptyStringArray(payload.nodePath) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNonNegativeInteger(payload.iteration) &&
        isOptionalString(payload.branchId)
      );
    case 'loop_iteration':
      return (
        typeof payload.loopId === 'string' &&
        isOptionalNonNegativeInteger(payload.iteration) &&
        payload.iteration !== undefined &&
        typeof payload.stopConditionMet === 'boolean'
      );
    case 'watch_poll':
      return (
        typeof payload.nodeId === 'string' &&
        WATCH_OUTCOMES.has(payload.outcome as WorkflowWatchOutcome) &&
        isNonEmptyStringArray(payload.nodePath) &&
        typeof payload.at === 'string'
      );
    case 'paused':
      degradeUnknownInitiator(payload, 'initiator', 'initiatorReason');
      return (
        typeof payload.pauseReason === 'string' &&
        isOptionalString(payload.initiatorReason)
      );
    case 'run_complete':
      return isCanonicalRunComplete(payload);
    case 'steps_queued':
      return (
        Array.isArray(payload.pendingSteps) &&
        payload.pendingSteps.every((node) => isWorkflowNodeDescriptor(node)) &&
        isWorkflowStepsQueuedResolution(payload.resolution) &&
        (payload.pendingSteps.length === 0
          ? payload.resolution !== undefined
          : payload.resolution === undefined)
      );
  }
  return false;
}

/**
 * Validate and normalize one direct `_kiro/workflow/*` notification.
 *
 * `node_start` uses `type` for the node type on the wire. The returned event
 * reserves `type` for its discriminant and preserves the wire value as
 * `nodeType`.
 */
export function parseWorkflowNotification(
  method: string,
  payload: unknown
): WorkflowEvent | null {
  if (!method.startsWith(WORKFLOW_METHOD_PREFIX) || !isRecord(payload)) {
    return null;
  }

  const wireType = method.slice(WORKFLOW_METHOD_PREFIX.length);
  if (!EVENT_TYPES.has(wireType as WorkflowEvent['type'])) return null;

  const normalized: Record<string, unknown> = {
    ...payload,
    ...(wireType === 'node_start' && payload.type !== undefined
      ? { nodeType: payload.type }
      : {}),
    type: wireType,
  };

  return isWorkflowEvent(normalized) ? normalized : null;
}

/**
 * Rehydrate a lifecycle record replayed as an ACP user-message update.
 *
 * Current records identify themselves through
 * `_meta.kiro.notification.kind`. The top-level kind and `wf-progress-*`
 * message-id paths keep existing workflow histories readable.
 */
// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'parsePersistedWorkflowProgress' has a complexity of 51. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
export function parsePersistedWorkflowProgress(
  update: unknown
): PersistedWorkflowProgressParseResult {
  if (!isRecord(update) || update.sessionUpdate !== 'user_message_chunk') {
    return { kind: 'not-workflow' };
  }

  const outerMeta = isRecord(update._meta) ? update._meta : undefined;
  const meta =
    outerMeta && isRecord(outerMeta.kiro)
      ? (outerMeta.kiro as WorkflowKiroMetadata)
      : undefined;
  const notification = isRecord(meta?.notification)
    ? (meta.notification as WorkflowNotificationMetadata)
    : undefined;
  const messageId =
    typeof meta?.messageId === 'string'
      ? meta.messageId
      : typeof notification?.notifyId === 'string'
        ? notification.notifyId
        : undefined;
  const isWorkflowProgress =
    notification?.kind === 'workflow-progress' ||
    meta?.kind === 'workflow-progress' ||
    messageId?.startsWith('wf-progress-') === true;
  if (!isWorkflowProgress) return { kind: 'not-workflow' };

  const invalid = (): PersistedWorkflowProgressParseResult => ({
    kind: 'invalid-workflow',
    ...(messageId !== undefined ? { messageId } : {}),
  });
  const content = update.content;
  if (
    !isRecord(content) ||
    content.type !== 'text' ||
    typeof content.text !== 'string'
  ) {
    return invalid();
  }

  let payload: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(content.text);
    if (!isRecord(parsed)) return invalid();
    payload = parsed;
  } catch (error) {
    logger.debug('workflow-protocol: failed to parse persisted progress', {
      error,
      messageId,
    });
    return invalid();
  }

  const payloadMethod = isNonEmptyString(payload.method)
    ? payload.method
    : undefined;
  const metadataMethod = isNonEmptyString(notification?.eventType)
    ? `${WORKFLOW_METHOD_PREFIX}${notification.eventType}`
    : undefined;
  if (
    (payload.method !== undefined && payloadMethod === undefined) ||
    (notification?.eventType !== undefined && metadataMethod === undefined) ||
    (payloadMethod !== undefined &&
      metadataMethod !== undefined &&
      payloadMethod !== metadataMethod)
  ) {
    return invalid();
  }
  const method = payloadMethod ?? metadataMethod;
  if (!method) return invalid();

  const payloadWorkflowId = isNonEmptyString(payload.workflowId)
    ? payload.workflowId
    : undefined;
  const metadataWorkflowId = isNonEmptyString(notification?.workflowId)
    ? notification.workflowId
    : undefined;
  if (
    (payload.workflowId !== undefined && payloadWorkflowId === undefined) ||
    (notification?.workflowId !== undefined &&
      metadataWorkflowId === undefined) ||
    (payloadWorkflowId !== undefined &&
      metadataWorkflowId !== undefined &&
      payloadWorkflowId !== metadataWorkflowId)
  ) {
    return invalid();
  }
  const workflowId = payloadWorkflowId ?? metadataWorkflowId;
  if (!workflowId) return invalid();

  const { method: _method, ...eventPayload } = payload;
  void _method;
  const event = parseWorkflowNotification(method, {
    ...eventPayload,
    workflowId,
  });
  return event
    ? {
        kind: 'workflow-progress',
        progress: {
          event,
          ...(messageId !== undefined ? { messageId } : {}),
        },
      }
    : invalid();
}

/** Validate the ownership data used before loading or messaging a child. */
function isWorkflowLoadResponse(value: unknown): value is WorkflowLoadResponse {
  return (
    isRecord(value) &&
    typeof value.workflowId === 'string' &&
    isWorkflowStateSnapshot(value.state) &&
    value.state.workflowId === value.workflowId &&
    Array.isArray(value.stepSessions) &&
    value.stepSessions.every(isWorkflowStepSessionRef) &&
    (value.nodePlan === undefined ||
      (Array.isArray(value.nodePlan) &&
        value.nodePlan.every((node) => isWorkflowNodeDescriptor(node))))
  );
}

export function parseWorkflowLoadResponse(
  value: unknown
): WorkflowLoadResponse | null {
  return isWorkflowLoadResponse(value) ? value : null;
}

export const WORKFLOW_LOAD_CONTRACT: RpcContract<
  { workflowId: string },
  WorkflowLoadResponse
> = {
  method: '_kiro/workflow/load',
  encode: ({ workflowId }) => ({ workflowId }),
  decode: parseWorkflowLoadResponse,
};

function isWorkflowRecipeDescriptor(
  value: unknown
): value is WorkflowRecipeDescriptor {
  return (
    isRecord(value) &&
    isNonEmptyString(value.name) &&
    isOptionalString(value.description) &&
    isOptionalString(value.source) &&
    isOptionalBoolean(value.builtIn) &&
    isOptionalString(value.validationError) &&
    isOptionalStringMap(value.inputs) &&
    (value.plan === undefined ||
      (Array.isArray(value.plan) &&
        value.plan.every((node) => isWorkflowNodeDescriptor(node))))
  );
}

export function parseWorkflowRecipeListResponse(
  value: unknown
): WorkflowRecipeListResponse | null {
  return isRecord(value) &&
    Array.isArray(value.recipes) &&
    value.recipes.every(isWorkflowRecipeDescriptor)
    ? { recipes: value.recipes }
    : null;
}

export function parseWorkflowCreateResponse(
  value: unknown
): WorkflowCreateResponse | null {
  return isRecord(value) &&
    isNonEmptyString(value.workflowId) &&
    isWorkflowStateSnapshot(value.initialState) &&
    value.initialState.workflowId === value.workflowId
    ? {
        workflowId: value.workflowId,
        initialState: value.initialState,
      }
    : null;
}

export function parseWorkflowInvokeResponse(
  value: unknown
): WorkflowInvokeResponse | null {
  return isRecord(value) &&
    isNonEmptyString(value.workflowId) &&
    isWorkflowStatus(value.status)
    ? {
        workflowId: value.workflowId,
        status: value.status,
      }
    : null;
}

export const WORKFLOW_LIST_RECIPES_CONTRACT: RpcContract<
  { workspacePaths: readonly string[] },
  WorkflowRecipeListResponse
> = {
  method: '_kiro/workflow/listRecipes',
  encode: ({ workspacePaths }) => ({ workspacePaths: [...workspacePaths] }),
  decode: parseWorkflowRecipeListResponse,
};

export const WORKFLOW_CREATE_CONTRACT: RpcContract<
  WorkflowCreateRequest,
  WorkflowCreateResponse
> = {
  method: '_kiro/workflow/new',
  encode: ({ source, inputs, parentSessionId, modelId }) => ({
    ...(source.type === 'path'
      ? { workflowPath: source.workflowPath }
      : { workflow: source.workflow }),
    inputs,
    ...(parentSessionId === undefined ? {} : { parentSessionId }),
    ...(modelId === undefined ? {} : { modelId }),
  }),
  decode: parseWorkflowCreateResponse,
};

export const WORKFLOW_INVOKE_CONTRACT: RpcContract<
  { workflowId: string },
  WorkflowInvokeResponse
> = {
  method: '_kiro/workflow/invoke',
  encode: ({ workflowId }) => ({ workflowId }),
  decode: parseWorkflowInvokeResponse,
};

function isWorkflowRunSummary(value: unknown): value is WorkflowRunSummary {
  return (
    isRecord(value) &&
    isNonEmptyString(value.workflowId) &&
    typeof value.name === 'string' &&
    isWorkflowStatus(value.status) &&
    typeof value.createdAt === 'string' &&
    typeof value.updatedAt === 'string' &&
    isOptionalString(value.startedAt) &&
    isOptionalString(value.endedAt) &&
    isOptionalNonEmptyString(value.parentSessionId)
  );
}

export function parseWorkflowListResponse(
  value: unknown
): WorkflowListResponse | null {
  if (
    !isRecord(value) ||
    !Array.isArray(value.runs) ||
    !value.runs.every(isWorkflowRunSummary)
  ) {
    return null;
  }
  return { runs: value.runs };
}

export function parseWorkflowInspectResponse(
  value: unknown
): WorkflowInspectResponse | null {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.workflowId) ||
    !isWorkflowStateSnapshot(value.state) ||
    value.state.workflowId !== value.workflowId ||
    (value.pendingSteps !== undefined &&
      (!Array.isArray(value.pendingSteps) ||
        !value.pendingSteps.every((node) => isWorkflowNodeDescriptor(node)))) ||
    (value.nodePlan !== undefined &&
      (!Array.isArray(value.nodePlan) ||
        !value.nodePlan.every((node) => isWorkflowNodeDescriptor(node))))
  ) {
    return null;
  }
  return {
    workflowId: value.workflowId,
    state: value.state,
    ...(value.pendingSteps === undefined
      ? {}
      : { pendingSteps: value.pendingSteps }),
    ...(value.nodePlan === undefined ? {} : { nodePlan: value.nodePlan }),
  };
}

export function parseWorkflowPauseResponse(
  value: unknown
): WorkflowPauseResponse | null {
  return isRecord(value) && typeof value.paused === 'boolean'
    ? { paused: value.paused }
    : null;
}

export function parseWorkflowResumeResponse(
  value: unknown
): WorkflowResumeResponse | null {
  return isRecord(value) &&
    isNonEmptyString(value.workflowId) &&
    isWorkflowStatus(value.status)
    ? {
        workflowId: value.workflowId,
        status: value.status,
      }
    : null;
}

export function parseWorkflowRetryResponse(
  value: unknown
): WorkflowRetryResponse | null {
  return isRecord(value) &&
    isNonEmptyString(value.workflowId) &&
    isWorkflowStatus(value.status) &&
    Array.isArray(value.retriedNodeIds) &&
    value.retriedNodeIds.every(isNonEmptyString)
    ? {
        workflowId: value.workflowId,
        status: value.status,
        retriedNodeIds: value.retriedNodeIds,
      }
    : null;
}

export function parseWorkflowCancelResponse(
  value: unknown
): WorkflowCancelResponse | null {
  return isRecord(value) &&
    typeof value.ok === 'boolean' &&
    isWorkflowStatus(value.previousStatus)
    ? {
        ok: value.ok,
        previousStatus: value.previousStatus,
      }
    : null;
}

export const WORKFLOW_LIST_CONTRACT: RpcContract<
  { workspacePaths: readonly string[] },
  WorkflowListResponse
> = {
  method: '_kiro/workflow/list',
  encode: ({ workspacePaths }) => ({ workspacePaths: [...workspacePaths] }),
  decode: parseWorkflowListResponse,
};

/**
 * Taken from the contracts that call them, so the gate can never name a method
 * the client doesn't send. An agent serving workflows advertises all fourteen
 * `_kiro/workflow/*` methods; these three separate "serves workflows" from
 * "doesn't" without breaking if the served set grows.
 */
const WORKFLOW_CONTROL_METHODS: readonly string[] = [
  WORKFLOW_CREATE_CONTRACT.method,
  WORKFLOW_INVOKE_CONTRACT.method,
  WORKFLOW_LIST_CONTRACT.method,
];

export const WORKFLOW_INSPECT_CONTRACT: RpcContract<
  { workflowId: string },
  WorkflowInspectResponse
> = {
  method: '_kiro/workflow/inspect',
  encode: ({ workflowId }) => ({ workflowId }),
  decode: parseWorkflowInspectResponse,
};

/**
 * Serialize control-action attribution, omitting both keys when absent so
 * unattributed calls keep their previous byte-identical request shape.
 */
function encodeAttribution({
  initiator,
  reason,
}: WorkflowActionAttribution): Record<string, string> {
  return {
    ...(initiator === undefined ? {} : { initiator }),
    // KAS rejects a `reason` that arrives without an `initiator`.
    ...(initiator !== undefined && reason !== undefined ? { reason } : {}),
  };
}

export const WORKFLOW_PAUSE_CONTRACT: RpcContract<
  { workflowId: string } & WorkflowActionAttribution,
  WorkflowPauseResponse
> = {
  method: '_kiro/workflow/pause',
  encode: ({ workflowId, ...attribution }) => ({
    workflowId,
    ...encodeAttribution(attribution),
  }),
  decode: parseWorkflowPauseResponse,
};

export const WORKFLOW_RESUME_CONTRACT: RpcContract<
  { workflowId: string } & WorkflowActionAttribution,
  WorkflowResumeResponse
> = {
  method: '_kiro/workflow/resume',
  encode: ({ workflowId, ...attribution }) => ({
    workflowId,
    ...encodeAttribution(attribution),
  }),
  decode: parseWorkflowResumeResponse,
};

export const WORKFLOW_RETRY_CONTRACT: RpcContract<
  { workflowId: string; nodeId?: string },
  WorkflowRetryResponse
> = {
  method: '_kiro/workflow/retry',
  encode: ({ workflowId, nodeId }) => ({
    workflowId,
    ...(nodeId === undefined ? {} : { nodeId }),
  }),
  decode: parseWorkflowRetryResponse,
};

export const WORKFLOW_CANCEL_CONTRACT: RpcContract<
  {
    workflowId: string;
    targetStatus?: 'aborted' | 'completed';
  } & WorkflowActionAttribution,
  WorkflowCancelResponse
> = {
  method: '_kiro/workflow/cancel',
  encode: ({ workflowId, targetStatus, ...attribution }) => ({
    workflowId,
    ...(targetStatus === undefined ? {} : { targetStatus }),
    ...encodeAttribution(attribution),
  }),
  decode: parseWorkflowCancelResponse,
};

export const WORKFLOW_LIFECYCLE_CONTRACTS: ReadonlyArray<
  NotificationContract<WorkflowEvent>
> = WORKFLOW_NOTIFICATION_METHODS.map((method) => ({
  method,
  decode: (value) => parseWorkflowNotification(method, value),
}));
