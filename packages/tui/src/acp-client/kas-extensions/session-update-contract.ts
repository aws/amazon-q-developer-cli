import type { KiroMeta, KiroPipelineStage } from '../../types/agent-events.js';

const KIRO_META_KINDS = [
  'agent-subtask',
  'system-notification',
  'workflow-progress',
] as const;
const PIPELINE_STATUSES = [
  'pending',
  'running',
  'completed',
  'failed',
] as const;
const NOTIFICATION_KINDS = [
  'system-notification',
  'workflow-progress',
] as const;

interface ToolCallChunkUpdate {
  sessionUpdate: 'tool_call_chunk';
  toolCallId: string;
  title: string;
  kind: string;
  kiroMeta?: KiroMeta;
}

interface RetryWarningUpdate {
  sessionUpdate: 'retry_warning';
  attempt: number;
  maxAttempts: number;
  delaySecs: number;
  message: string;
}

interface SteeringContentUpdate {
  sessionUpdate:
    | 'AgentExecutionUserMessageQueued'
    | 'AgentExecutionSteeringInjected';
  content: string;
}

interface SteeringClearedUpdate {
  sessionUpdate: 'AgentExecutionUserMessageCleared';
}

export type ExtSessionUpdate =
  | ToolCallChunkUpdate
  | RetryWarningUpdate
  | SteeringContentUpdate
  | SteeringClearedUpdate;

export interface ExtSessionUpdateEnvelope {
  sessionId?: string;
  update: ExtSessionUpdate;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === 'string';
}

function isStringArray(value: unknown): value is string[] {
  return (
    Array.isArray(value) && value.every((entry) => typeof entry === 'string')
  );
}

function isOneOf<const Value extends string>(
  value: unknown,
  values: readonly Value[]
): value is Value {
  return typeof value === 'string' && values.includes(value as Value);
}

function parsePipelineStage(value: unknown): KiroPipelineStage | null {
  if (
    !isRecord(value) ||
    typeof value.name !== 'string' ||
    typeof value.role !== 'string' ||
    !isOneOf(value.status, PIPELINE_STATUSES) ||
    !isStringArray(value.dependsOn) ||
    (value.agentSubtaskId !== null && typeof value.agentSubtaskId !== 'string')
  ) {
    return null;
  }
  return {
    name: value.name,
    role: value.role,
    status: value.status,
    dependsOn: value.dependsOn,
    agentSubtaskId: value.agentSubtaskId,
  };
}

function parseKiroMeta(value: unknown): KiroMeta | null {
  if (!isRecord(value)) return null;
  const result: KiroMeta = {};
  let invalidPipeline = false;

  if (value.kind !== undefined) {
    if (!isOneOf(value.kind, KIRO_META_KINDS)) return null;
    result.kind = value.kind;
  }
  if (value.agentSubtaskId !== undefined) {
    if (typeof value.agentSubtaskId !== 'string') return null;
    result.agentSubtaskId = value.agentSubtaskId;
  }
  if (value.pipeline !== undefined) {
    const pipeline = value.pipeline;
    if (
      isRecord(pipeline) &&
      typeof pipeline.groupId === 'string' &&
      Array.isArray(pipeline.stages)
    ) {
      const stages = pipeline.stages.map(parsePipelineStage);
      if (!stages.some((stage) => stage === null)) {
        result.pipeline = {
          groupId: pipeline.groupId,
          stages: stages.filter(
            (stage): stage is KiroPipelineStage => stage !== null
          ),
        };
      } else {
        invalidPipeline = true;
      }
    } else {
      invalidPipeline = true;
    }
  }
  for (const key of [
    'toolName',
    'toolId',
    'mcpServerName',
    'messageId',
    'timestamp',
  ] as const) {
    const entry = value[key];
    if (entry === undefined) continue;
    if (typeof entry !== 'string') return null;
    result[key] = entry;
  }
  if (value.refusal !== undefined) {
    if (
      !isRecord(value.refusal) ||
      !isOptionalString(value.refusal.category) ||
      !isOptionalString(value.refusal.explanation) ||
      !isOptionalString(value.refusal.recommendedModel)
    ) {
      return null;
    }
    result.refusal = {
      ...(value.refusal.category === undefined
        ? {}
        : { category: value.refusal.category }),
      ...(value.refusal.explanation === undefined
        ? {}
        : { explanation: value.refusal.explanation }),
      ...(value.refusal.recommendedModel === undefined
        ? {}
        : { recommendedModel: value.refusal.recommendedModel }),
    };
  }
  if (value.workflow !== undefined) {
    if (
      !isRecord(value.workflow) ||
      typeof value.workflow.workflowId !== 'string' ||
      typeof value.workflow.nodeId !== 'string' ||
      !isOptionalString(value.workflow.workflowName) ||
      (value.workflow.nodePath !== undefined &&
        !isStringArray(value.workflow.nodePath)) ||
      (value.workflow.type !== undefined && value.workflow.type !== 'step') ||
      (value.workflow.iteration !== undefined &&
        !isFiniteNumber(value.workflow.iteration)) ||
      !isOptionalString(value.workflow.branchId)
    ) {
      return null;
    }
    result.workflow = {
      workflowId: value.workflow.workflowId,
      nodeId: value.workflow.nodeId,
      ...(value.workflow.workflowName === undefined
        ? {}
        : { workflowName: value.workflow.workflowName }),
      ...(value.workflow.nodePath === undefined
        ? {}
        : { nodePath: value.workflow.nodePath }),
      ...(value.workflow.type === undefined
        ? {}
        : { type: value.workflow.type }),
      ...(value.workflow.iteration === undefined
        ? {}
        : { iteration: value.workflow.iteration }),
      ...(value.workflow.branchId === undefined
        ? {}
        : { branchId: value.workflow.branchId }),
    };
  }
  if (value.notification !== undefined) {
    if (
      !isRecord(value.notification) ||
      !isOneOf(value.notification.kind, NOTIFICATION_KINDS)
    ) {
      return null;
    }
    const notification: NonNullable<KiroMeta['notification']> = {
      kind: value.notification.kind,
    };
    for (const key of [
      'status',
      'workflowId',
      'agentName',
      'nodeName',
      'notifyId',
      'eventType',
    ] as const) {
      const entry = value.notification[key];
      if (entry === undefined) continue;
      if (typeof entry !== 'string') return null;
      notification[key] = entry;
    }
    result.notification = notification;
  }
  if (value.steeringClearedIds !== undefined) {
    if (!isStringArray(value.steeringClearedIds)) return null;
    result.steeringClearedIds = value.steeringClearedIds;
  }
  if (invalidPipeline && Object.keys(result).length === 0) return null;
  return result;
}

export function extractKiroMeta(value: unknown): KiroMeta | undefined {
  if (!isRecord(value) || !isRecord(value._meta)) return undefined;
  const parsed = parseKiroMeta(value._meta.kiro);
  return parsed ?? undefined;
}

export function decodeExtSessionUpdate(
  value: unknown
): ExtSessionUpdateEnvelope | null {
  if (
    !isRecord(value) ||
    !isOptionalString(value.sessionId) ||
    !isRecord(value.update)
  ) {
    return null;
  }
  const update = value.update;

  switch (update.sessionUpdate) {
    case 'tool_call_chunk': {
      if (
        typeof update.toolCallId !== 'string' ||
        typeof update.title !== 'string' ||
        typeof update.kind !== 'string'
      ) {
        return null;
      }
      const kiroMeta = extractKiroMeta(update);
      return {
        ...(value.sessionId === undefined
          ? {}
          : { sessionId: value.sessionId }),
        update: {
          sessionUpdate: update.sessionUpdate,
          toolCallId: update.toolCallId,
          title: update.title,
          kind: update.kind,
          ...(kiroMeta === undefined ? {} : { kiroMeta }),
        },
      };
    }
    case 'retry_warning':
      if (
        !isFiniteNumber(update.attempt) ||
        !isFiniteNumber(update.maxAttempts) ||
        !isFiniteNumber(update.delaySecs) ||
        typeof update.message !== 'string'
      ) {
        return null;
      }
      return {
        ...(value.sessionId === undefined
          ? {}
          : { sessionId: value.sessionId }),
        update: {
          sessionUpdate: update.sessionUpdate,
          attempt: update.attempt,
          maxAttempts: update.maxAttempts,
          delaySecs: update.delaySecs,
          message: update.message,
        },
      };
    case 'AgentExecutionUserMessageQueued':
    case 'AgentExecutionSteeringInjected':
      if (!isOptionalString(update.content)) return null;
      return {
        ...(value.sessionId === undefined
          ? {}
          : { sessionId: value.sessionId }),
        update: {
          sessionUpdate: update.sessionUpdate,
          content: update.content ?? '',
        },
      };
    case 'AgentExecutionUserMessageCleared':
      return {
        ...(value.sessionId === undefined
          ? {}
          : { sessionId: value.sessionId }),
        update: { sessionUpdate: update.sessionUpdate },
      };
    default:
      return null;
  }
}
