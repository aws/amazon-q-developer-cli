import type {
  WorkflowEvent,
  WorkflowNodeDescriptor,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowNodeType,
  WorkflowStateSnapshot,
  WorkflowStatus,
} from '../types/workflow.js';
import { logger } from './logger';

const WORKFLOW_METHOD_PREFIX = '_kiro/workflow/';
const MAX_TREE_DEPTH = 100;

const EVENT_TYPES: ReadonlySet<WorkflowEvent['type']> = new Set([
  'run_start',
  'node_start',
  'node_complete',
  'node_paused',
  'need_input',
  'loop_iteration',
  'watch_poll',
  'paused',
  'run_complete',
  'steps_queued',
]);

const TERMINAL_EVENT_ALIASES = {
  run_failed: 'failed',
  run_aborted: 'aborted',
} as const;

const NODE_TYPES: ReadonlySet<WorkflowNodeType> = new Set([
  'step',
  'sequence',
  'repeat',
  'parallel',
  'watch',
]);

const NODE_STATUSES: ReadonlySet<WorkflowNodeStatus> = new Set([
  'pending',
  'running',
  'paused',
  'completed',
  'failed',
  'aborted',
  'skipped',
]);

const WORKFLOW_STATUSES: ReadonlySet<WorkflowStatus> = new Set([
  'running',
  'paused',
  'completed',
  'failed',
  'aborted',
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

function isOptionalNumber(value: unknown): boolean {
  return value === undefined || typeof value === 'number';
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

function isOptionalStringMap(value: unknown): boolean {
  return (
    value === undefined ||
    (isRecord(value) &&
      Object.values(value).every((entry) => typeof entry === 'string'))
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
    !isOptionalNumber(value.maxIterations) ||
    !isOptionalString(value.stopWhen) ||
    !isOptionalString(value.onMaxIterations)
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
    !NODE_STATUSES.has(value.status as WorkflowNodeStatus) ||
    !isOptionalString(value.agentName) ||
    !isOptionalString(value.modelId) ||
    !isOptionalString(value.effortLevel) ||
    !isOptionalString(value.sessionId) ||
    !isOptionalString(value.startedAt) ||
    !isOptionalString(value.endedAt) ||
    !isOptionalNumber(value.iteration) ||
    !isOptionalString(value.branchId) ||
    !isOptionalStringMap(value.artifacts) ||
    !isOptionalString(value.capturedOutput) ||
    !isOptionalBoolean(value.watchTerminal) ||
    !isOptionalString(value.failureReason) ||
    !isOptionalNumber(value.continuationAttempts)
  ) {
    return false;
  }
  if (
    value.completionSignal !== undefined &&
    !['success', 'need_input', 'error'].includes(
      value.completionSignal as string
    )
  ) {
    return false;
  }
  return (
    value.children === undefined ||
    (Array.isArray(value.children) &&
      value.children.every((child) => isWorkflowNodeState(child, depth + 1)))
  );
}

function isWorkflowStateSnapshot(
  value: unknown
): value is WorkflowStateSnapshot {
  if (!isRecord(value)) return false;
  return (
    typeof value.workflowId === 'string' &&
    typeof value.workflowName === 'string' &&
    WORKFLOW_STATUSES.has(value.status as WorkflowStatus) &&
    (value.root === undefined || isWorkflowNodeState(value.root)) &&
    isOptionalStringMap(value.inputs) &&
    isOptionalStringMap(value.artifacts) &&
    isOptionalStringMap(value.capturedOutputs) &&
    isOptionalString(value.pauseReason) &&
    isOptionalString(value.parentSessionId) &&
    isOptionalString(value.workspacePath) &&
    isOptionalStringArray(value.additionalDirectories) &&
    isOptionalString(value.createdAt)
  );
}

function hasWorkflowId(payload: Record<string, unknown>): boolean {
  return (
    typeof payload.workflowId === 'string' && payload.workflowId.length > 0
  );
}

function hasOptionalParent(payload: Record<string, unknown>): boolean {
  return isOptionalString(payload.parentSessionId);
}

function isValidEvent(
  type: WorkflowEvent['type'],
  payload: Record<string, unknown>
): boolean {
  if (!hasWorkflowId(payload) || !hasOptionalParent(payload)) return false;

  switch (type) {
    case 'run_start':
      return (
        isOptionalString(payload.workflowName) &&
        isOptionalStringMap(payload.inputs) &&
        (payload.nodeTree === undefined ||
          (Array.isArray(payload.nodeTree) &&
            payload.nodeTree.every((node) => isWorkflowNodeDescriptor(node))))
      );
    case 'node_start':
      return (
        typeof payload.nodeId === 'string' &&
        (payload.nodeType === undefined ||
          NODE_TYPES.has(payload.nodeType as WorkflowNodeType)) &&
        isOptionalStringArray(payload.nodePath) &&
        isOptionalString(payload.agentName) &&
        isOptionalString(payload.prompt) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNumber(payload.iteration) &&
        isOptionalString(payload.branchId)
      );
    case 'node_complete':
      return (
        typeof payload.nodeId === 'string' &&
        NODE_STATUSES.has(payload.status as WorkflowNodeStatus) &&
        isOptionalStringArray(payload.nodePath) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNumber(payload.iteration) &&
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
        isOptionalStringArray(payload.nodePath) &&
        isOptionalString(payload.sessionId) &&
        isOptionalNumber(payload.iteration) &&
        isOptionalString(payload.branchId)
      );
    case 'need_input':
      return (
        typeof payload.nodeId === 'string' &&
        typeof payload.reason === 'string' &&
        isOptionalStringArray(payload.nodePath)
      );
    case 'loop_iteration':
      return (
        typeof payload.loopId === 'string' &&
        typeof payload.iteration === 'number' &&
        typeof payload.stopConditionMet === 'boolean'
      );
    case 'watch_poll':
      return (
        typeof payload.nodeId === 'string' &&
        typeof payload.outcome === 'string' &&
        isOptionalStringArray(payload.nodePath) &&
        isOptionalString(payload.at)
      );
    case 'paused':
      return typeof payload.pauseReason === 'string';
    case 'run_complete':
      return (
        WORKFLOW_STATUSES.has(payload.status as WorkflowStatus) &&
        (payload.finalState === undefined ||
          isWorkflowStateSnapshot(payload.finalState))
      );
    case 'steps_queued':
      return (
        payload.pendingSteps === undefined ||
        (Array.isArray(payload.pendingSteps) &&
          payload.pendingSteps.every((node) => isWorkflowNodeDescriptor(node)))
      );
  }
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
  const terminalStatus =
    TERMINAL_EVENT_ALIASES[wireType as keyof typeof TERMINAL_EVENT_ALIASES];
  const type = terminalStatus ? 'run_complete' : wireType;
  if (!EVENT_TYPES.has(type as WorkflowEvent['type'])) return null;

  const normalized: Record<string, unknown> = {
    ...payload,
    ...(type === 'node_start' && payload.type !== undefined
      ? { nodeType: payload.type }
      : {}),
    type,
    ...(terminalStatus ? { status: terminalStatus } : {}),
  };

  if (!isValidEvent(type as WorkflowEvent['type'], normalized)) return null;
  return normalized as unknown as WorkflowEvent;
}

/**
 * Rehydrate a lifecycle record replayed as an ACP user-message update.
 *
 * Current records identify themselves through
 * `_meta.kiro.notification.kind`. The top-level kind and `wf-progress-*`
 * message-id paths keep existing workflow histories readable.
 */
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

  const method =
    typeof payload.method === 'string'
      ? payload.method
      : typeof notification?.eventType === 'string'
        ? `${WORKFLOW_METHOD_PREFIX}${notification.eventType}`
        : undefined;
  if (!method) return invalid();

  const workflowId =
    typeof payload.workflowId === 'string'
      ? payload.workflowId
      : typeof notification?.workflowId === 'string'
        ? notification.workflowId
        : undefined;
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
