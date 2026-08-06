import type * as acp from '@agentclientprotocol/sdk';
import {
  AgentEventType,
  type AgentStreamEvent,
  type KiroMeta,
  type ToolCallOrigin,
} from '../types/agent-events';
import type { SessionLifecycleEvent } from '../types/multi-session.js';

export interface KasSubagentRoutingState {
  /** Maps every routed child tool call to its owning subtask. */
  toolCallToSubtask: Map<string, string>;
  /** Correlates only independent-subagent lifecycle wrappers to their subtask. */
  subagentLifecycleToolCallToSubtask: Map<string, string>;
  /** Subtasks rendered as explicit independent KAS subagent sessions. */
  independentSubagentSubtasks: Set<string>;
  /** Tool cards already mirrored into the main transcript. */
  standaloneMainForwardedToolCalls: Set<string>;
  /** Child tools first announced by an unclassified `tool_call_chunk`. */
  chunkDiscoveredToolCalls: Map<string, string>;
  /** Hidden subtasks proven to need their tool cards in the main transcript. */
  standaloneSubtasks: Set<string>;
  /** Initial tool metadata retained for later metadata-poor permission events. */
  kasToolCallSnapshots: Map<
    string,
    {
      title?: string;
      name?: string;
      origin?: ToolCallOrigin;
      originalTitle?: string;
      kind?: string;
      rawInput?: Record<string, unknown>;
    }
  >;
  /** Subtasks with a visible crew stage, including derived wrapper ids. */
  pipelineStageSubtasks: Set<string>;
}

export interface KasPipelineRoutingResult {
  subagents: Array<{
    sessionId: string;
    sessionName: string;
    agentName: string;
    status: { type: string };
    group: string;
    role: string;
    dependsOn: string[];
  }>;
  pendingStages: Array<{
    sessionId?: string;
    name: string;
    role: string;
    agentName: string;
    group: string;
    dependsOn: string[];
  }>;
}

/**
 * Output sinks for KAS subagent routing. KAS streams every subagent's
 * activity inline on the main session's wire connection; the routing logic
 * classifies each event and uses exactly one of these sinks to re-broadcast
 * it on the client channel where an equivalent V2 event would have arrived,
 * so downstream stores and views stay engine-agnostic.
 */
export interface KasSubagentRoutingEmitter {
  /**
   * Main-transcript stream. Events emitted here render in the primary chat
   * log, attributed to the main turn (used for subtask tool calls that
   * should stay visible inline rather than behind a subagent session).
   */
  emitMain: (event: AgentStreamEvent) => void;
  /**
   * Per-subagent stream, tagged with the owning subagent's session id.
   * Delivers a stream event scoped to a single subagent session, for
   * per-session buffering and rendering.
   */
  emitMultiSession: (sessionId: string, event: AgentStreamEvent) => void;
  /**
   * Subagent lifecycle (created/terminated). Drives incremental roster
   * mutation: created adds a session row, terminated flips its status.
   */
  emitSession: (event: SessionLifecycleEvent) => void;
  /**
   * Crew roster snapshot: the complete set of live pipeline subagents plus
   * not-yet-spawned stages. Replace-on-arrival, not a delta.
   */
  emitSubagentList: (
    subagents: KasPipelineRoutingResult['subagents'],
    pendingStages: KasPipelineRoutingResult['pendingStages']
  ) => void;
}

export type KasPermissionRequest = Omit<
  acp.RequestPermissionRequest,
  'toolCall'
> & {
  toolCallId?: string;
  toolCall?: Partial<acp.RequestPermissionRequest['toolCall']> & {
    name?: string;
    kind?: acp.ToolKind | null;
    origin?: ToolCallOrigin;
  };
};

export interface KasPermissionRoutingInput {
  request: KasPermissionRequest;
  toolCallId: string;
  metadataSubtaskId?: string;
  isSubagentSpawn: boolean;
  fallbackTitle?: string;
  fallbackRawInput?: Record<string, unknown>;
  originSessionId?: string;
}

export interface KasSubagentRoutingStore {
  resetKasSubagentRouting: () => void;
  rememberKasToolCall: (event: AgentStreamEvent) => void;
  forgetFinishedKasToolCallSnapshot: (event: AgentStreamEvent) => void;
  recordKasChunkToolCall: (toolCallId: string, subtaskId: string) => void;
  routeKasSubtaskEvent: (
    event: AgentStreamEvent,
    meta: KiroMeta | undefined,
    emitter: KasSubagentRoutingEmitter,
    parentSessionId?: string
  ) => boolean;
  handlePipelineStateUpdate: (
    pipeline: NonNullable<KiroMeta['pipeline']>,
    parentToolCallId: string | undefined,
    emitter: KasSubagentRoutingEmitter
  ) => void;
  prepareKasPermissionRequest: (
    input: KasPermissionRoutingInput
  ) => acp.RequestPermissionRequest;
}

export function createInitialKasSubagentRoutingState(): KasSubagentRoutingState {
  return {
    toolCallToSubtask: new Map(),
    subagentLifecycleToolCallToSubtask: new Map(),
    independentSubagentSubtasks: new Set(),
    standaloneMainForwardedToolCalls: new Set(),
    chunkDiscoveredToolCalls: new Map(),
    standaloneSubtasks: new Set(),
    kasToolCallSnapshots: new Map(),
    pipelineStageSubtasks: new Set(),
  };
}

/**
 * Subagent event types that should also render in the main transcript when the
 * subtask is standalone and has no crew panel.
 */
const STANDALONE_MAIN_FORWARD_TYPES: ReadonlySet<AgentEventType> = new Set([
  AgentEventType.ToolCall,
  AgentEventType.ToolCallUpdate,
  AgentEventType.ToolCallFinished,
  AgentEventType.Content,
  AgentEventType.Thought,
]);

function isDerivedPipelineStageSubtaskId(subtaskId: string): boolean {
  return /^invoke_sub_?agent_.+_stage_.+$/.test(subtaskId);
}

function extractKasSubagentName(title: string | undefined): string | undefined {
  const name = title?.match(/^Sub-agent:\s*(.+)$/)?.[1]?.trim();
  return name || undefined;
}

function kasStringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function kasSubagentNameFromArgs(
  args: Record<string, unknown> | undefined
): string | undefined {
  if (!args) return undefined;
  return (
    kasStringValue(args.name) ??
    kasStringValue(args.agentName) ??
    kasStringValue(args.subAgentName) ??
    kasStringValue(args.agent)
  );
}

function isMappedStandaloneSubtask(
  state: KasSubagentRoutingState,
  subtaskId: string
): boolean {
  return (
    state.standaloneSubtasks.has(subtaskId) &&
    !state.pipelineStageSubtasks.has(subtaskId) &&
    !state.independentSubagentSubtasks.has(subtaskId)
  );
}

function isUnclassifiedChunkToolCall(
  state: KasSubagentRoutingState,
  toolCallId: string,
  subtaskId: string
): boolean {
  return (
    state.chunkDiscoveredToolCalls.get(toolCallId) === subtaskId &&
    !state.standaloneSubtasks.has(subtaskId) &&
    !state.pipelineStageSubtasks.has(subtaskId) &&
    !state.independentSubagentSubtasks.has(subtaskId)
  );
}

function promoteMappedStandaloneToolCallToMain(
  state: KasSubagentRoutingState,
  emitter: KasSubagentRoutingEmitter,
  toolCallId: string,
  subtaskId: string,
  event?: AgentStreamEvent
): boolean {
  if (!isMappedStandaloneSubtask(state, subtaskId)) return false;
  if (state.standaloneMainForwardedToolCalls.has(toolCallId)) return false;
  if (event?.type === AgentEventType.ToolCall) {
    state.standaloneMainForwardedToolCalls.add(toolCallId);
    emitter.emitMain({ ...event, sessionId: undefined });
    return true;
  }
  const snapshot = state.kasToolCallSnapshots.get(toolCallId);
  if (!snapshot) return false;
  state.standaloneMainForwardedToolCalls.add(toolCallId);
  emitter.emitMain({
    type: AgentEventType.ToolCall,
    id: toolCallId,
    name: snapshot.title ?? toolCallId,
    kind: snapshot.kind,
    args: snapshot.rawInput ?? {},
  });
  return true;
}

function finishSubagentLifecycleToolCall(
  state: KasSubagentRoutingState,
  emitter: KasSubagentRoutingEmitter,
  toolCallId: string,
  subtaskId: string,
  isExplicitLifecycleSignal = false
): void {
  const lifecycleSubtaskId =
    state.subagentLifecycleToolCallToSubtask.get(toolCallId);
  if (isExplicitLifecycleSignal || lifecycleSubtaskId === subtaskId) {
    emitter.emitSession({
      type: 'session_terminated',
      sessionId: subtaskId,
    });
    state.independentSubagentSubtasks.delete(subtaskId);
  }
  state.subagentLifecycleToolCallToSubtask.delete(toolCallId);
}

function routeKasSubtaskEvent(
  state: KasSubagentRoutingState,
  event: AgentStreamEvent,
  meta: KiroMeta | undefined,
  emitter: KasSubagentRoutingEmitter,
  parentSessionId?: string
): boolean {
  if (meta?.agentSubtaskId) {
    const subtaskId = meta.agentSubtaskId;
    const isKnownIndependentSubagent =
      state.independentSubagentSubtasks.has(subtaskId);
    const isDerivedPipelineStageSubtask =
      isDerivedPipelineStageSubtaskId(subtaskId);
    const isIndependentLifecycleSignal =
      meta.kind === 'agent-subtask' &&
      !state.pipelineStageSubtasks.has(subtaskId) &&
      !isDerivedPipelineStageSubtask;
    const isCrewActivity =
      !isKnownIndependentSubagent &&
      !isIndependentLifecycleSignal &&
      (state.pipelineStageSubtasks.has(subtaskId) ||
        isDerivedPipelineStageSubtask);
    const shouldManageSubagentLifecycle =
      isIndependentLifecycleSignal && !isCrewActivity;
    const isIndependentLifecycleToolCall =
      shouldManageSubagentLifecycle ||
      ('id' in event &&
        state.subagentLifecycleToolCallToSubtask.get(event.id) === subtaskId);
    const isUnclassifiedChunkCall =
      'id' in event && isUnclassifiedChunkToolCall(state, event.id, subtaskId);
    if (event.type === AgentEventType.ToolCall) {
      event.sessionId = subtaskId;
      state.toolCallToSubtask.set(event.id, subtaskId);
      if (shouldManageSubagentLifecycle) {
        state.independentSubagentSubtasks.add(subtaskId);
        state.subagentLifecycleToolCallToSubtask.set(event.id, subtaskId);
        const sessionName =
          kasSubagentNameFromArgs(event.args) ??
          extractKasSubagentName(event.name) ??
          subtaskId;
        emitter.emitSession({
          type: 'session_created',
          session: {
            id: subtaskId,
            name: sessionName,
            agentName: sessionName,
            status: 'busy',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
            ...(parentSessionId ? { parentSession: parentSessionId } : {}),
          },
        });
      }
    }
    emitter.emitMultiSession(subtaskId, event);
    if (
      !isCrewActivity &&
      !isUnclassifiedChunkCall &&
      (!isKnownIndependentSubagent || isIndependentLifecycleToolCall) &&
      STANDALONE_MAIN_FORWARD_TYPES.has(event.type)
    ) {
      const mainEvent =
        event.type === AgentEventType.ToolCall
          ? { ...event, sessionId: undefined }
          : event;
      if (event.type === AgentEventType.ToolCall) {
        state.standaloneMainForwardedToolCalls.add(event.id);
      }
      emitter.emitMain(mainEvent);
    }
    if (event.type === AgentEventType.ToolCallFinished) {
      state.toolCallToSubtask.delete(event.id);
      state.standaloneMainForwardedToolCalls.delete(event.id);
      state.chunkDiscoveredToolCalls.delete(event.id);
      finishSubagentLifecycleToolCall(
        state,
        emitter,
        event.id,
        subtaskId,
        shouldManageSubagentLifecycle
      );
    }
    return true;
  }

  const mappedSubtaskId =
    'id' in event ? state.toolCallToSubtask.get(event.id) : undefined;
  if (!mappedSubtaskId) return false;

  const mappedEvent =
    event.type === AgentEventType.ToolCall
      ? { ...event, sessionId: mappedSubtaskId }
      : event;
  emitter.emitMultiSession(mappedSubtaskId, mappedEvent);
  if (
    event.type === AgentEventType.ToolCall &&
    state.chunkDiscoveredToolCalls.get(event.id) === mappedSubtaskId
  ) {
    state.standaloneSubtasks.add(mappedSubtaskId);
  }
  const promotedMappedStandaloneToolCall =
    event.type === AgentEventType.ToolCall &&
    promoteMappedStandaloneToolCallToMain(
      state,
      emitter,
      event.id,
      mappedSubtaskId,
      event
    );
  if (
    'id' in event &&
    !promotedMappedStandaloneToolCall &&
    STANDALONE_MAIN_FORWARD_TYPES.has(event.type)
  ) {
    promoteMappedStandaloneToolCallToMain(
      state,
      emitter,
      event.id,
      mappedSubtaskId
    );
  }
  if (
    'id' in event &&
    state.standaloneMainForwardedToolCalls.has(event.id) &&
    !promotedMappedStandaloneToolCall
  ) {
    const mainEvent =
      event.type === AgentEventType.ToolCall
        ? { ...event, sessionId: undefined }
        : event;
    emitter.emitMain(mainEvent);
  }
  if (event.type === AgentEventType.ToolCallFinished) {
    state.toolCallToSubtask.delete(event.id);
    state.standaloneMainForwardedToolCalls.delete(event.id);
    state.chunkDiscoveredToolCalls.delete(event.id);
    state.standaloneSubtasks.delete(mappedSubtaskId);
    finishSubagentLifecycleToolCall(state, emitter, event.id, mappedSubtaskId);
  }
  return true;
}

function handlePipelineStateUpdate(
  state: KasSubagentRoutingState,
  pipeline: NonNullable<KiroMeta['pipeline']>,
  parentToolCallId: string | undefined,
  emitter: KasSubagentRoutingEmitter
): void {
  const statusMap: Record<string, { type: string }> = {
    running: { type: 'working' },
    completed: { type: 'terminated' },
    failed: { type: 'terminated' },
  };
  const subagents = pipeline.stages
    .filter(
      (stage) => stage.agentSubtaskId != null && stage.status !== 'pending'
    )
    .map((stage) => ({
      sessionId: stage.agentSubtaskId!,
      sessionName: stage.name,
      agentName: stage.role,
      status: statusMap[stage.status] ?? { type: 'idle' },
      group: pipeline.groupId,
      role: stage.role,
      dependsOn: stage.dependsOn,
    }));

  // Register assigned and derived wrapper ids before events or approvals can
  // arrive, including pending stages that do not yet render in the footer.
  for (const stage of pipeline.stages) {
    if (stage.agentSubtaskId) {
      state.pipelineStageSubtasks.add(stage.agentSubtaskId);
    }
    if (parentToolCallId && stage.name) {
      state.pipelineStageSubtasks.add(
        `invoke_subagent_${parentToolCallId}_stage_${stage.name}`
      );
      state.pipelineStageSubtasks.add(
        `invoke_sub_agent_${parentToolCallId}_stage_${stage.name}`
      );
    }
  }

  const pendingStages = pipeline.stages
    .filter((stage) => stage.status === 'pending')
    .map((stage) => ({
      ...(stage.agentSubtaskId ? { sessionId: stage.agentSubtaskId } : {}),
      name: stage.name,
      role: stage.role,
      agentName: stage.role,
      group: pipeline.groupId,
      dependsOn: stage.dependsOn,
    }));
  emitter.emitSubagentList(subagents, pendingStages);
}

export function createKasSubagentRoutingActions(
  getKas: () => KasSubagentRoutingState
): KasSubagentRoutingStore {
  // Routing correlation mutates in place; emitted events carry reactive UI updates.
  return {
    resetKasSubagentRouting: () => {
      const kas = getKas();
      kas.toolCallToSubtask.clear();
      kas.subagentLifecycleToolCallToSubtask.clear();
      kas.independentSubagentSubtasks.clear();
      kas.standaloneMainForwardedToolCalls.clear();
      kas.chunkDiscoveredToolCalls.clear();
      kas.standaloneSubtasks.clear();
      kas.kasToolCallSnapshots.clear();
      kas.pipelineStageSubtasks.clear();
    },

    rememberKasToolCall: (event) => {
      if (event.type !== AgentEventType.ToolCall) return;
      getKas().kasToolCallSnapshots.set(event.id, {
        title: event.originalTitle ?? event.name,
        name: event.name,
        origin: event.origin,
        originalTitle: event.originalTitle,
        kind: event.kind,
        rawInput: event.args,
      });
    },

    forgetFinishedKasToolCallSnapshot: (event) => {
      if (event.type !== AgentEventType.ToolCallFinished) return;
      getKas().kasToolCallSnapshots.delete(event.id);
    },

    recordKasChunkToolCall: (toolCallId, subtaskId) => {
      const kas = getKas();
      kas.toolCallToSubtask.set(toolCallId, subtaskId);
      kas.chunkDiscoveredToolCalls.set(toolCallId, subtaskId);
    },

    routeKasSubtaskEvent: (event, meta, emitter, parentSessionId) =>
      routeKasSubtaskEvent(getKas(), event, meta, emitter, parentSessionId),

    handlePipelineStateUpdate: (pipeline, parentToolCallId, emitter) =>
      handlePipelineStateUpdate(getKas(), pipeline, parentToolCallId, emitter),

    prepareKasPermissionRequest: ({
      request,
      toolCallId,
      metadataSubtaskId,
      isSubagentSpawn,
      fallbackTitle,
      fallbackRawInput,
      originSessionId,
    }) => {
      const kas = getKas();
      const subtaskId =
        kas.toolCallToSubtask.get(toolCallId) ?? metadataSubtaskId;
      const cachedToolCall = kas.kasToolCallSnapshots.get(toolCallId);
      const existingToolCall = request.toolCall ?? {};
      const hasRenderableSubagentSession =
        !!subtaskId &&
        (kas.pipelineStageSubtasks.has(subtaskId) ||
          kas.independentSubagentSubtasks.has(subtaskId));
      const enrichedToolCall = {
        ...existingToolCall,
        toolCallId,
        title:
          existingToolCall.title ??
          fallbackTitle ??
          cachedToolCall?.originalTitle ??
          cachedToolCall?.title,
        rawInput:
          existingToolCall.rawInput ??
          fallbackRawInput ??
          cachedToolCall?.rawInput,
        name: existingToolCall.name ?? cachedToolCall?.name,
        kind:
          existingToolCall.kind ??
          (cachedToolCall?.kind as acp.ToolKind | undefined),
        origin: existingToolCall.origin ?? cachedToolCall?.origin,
      };
      const enriched: acp.RequestPermissionRequest = {
        ...request,
        toolCall: enrichedToolCall,
        ...(subtaskId &&
          !isSubagentSpawn &&
          hasRenderableSubagentSession && { sessionId: subtaskId }),
        ...(originSessionId ? { originSessionId } : {}),
      };
      if (
        subtaskId &&
        !hasRenderableSubagentSession &&
        !isSubagentSpawn &&
        !kas.chunkDiscoveredToolCalls.has(toolCallId)
      ) {
        kas.standaloneSubtasks.add(subtaskId);
      }
      return enriched;
    },
  };
}
