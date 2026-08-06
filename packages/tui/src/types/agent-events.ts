import {
  toolDiffPolicy,
  type ToolCallOrigin,
  type ToolKind,
} from './tool-capabilities.js';
export {
  CODE_TOOL_NAMES,
  GLOB_TOOL_NAMES,
  GREP_TOOL_NAMES,
  IMAGE_READ_TOOL_NAMES,
  INTROSPECT_TOOL_NAMES,
  isParentSubagentTool,
  isWorkflowLaunchTool,
  KNOWLEDGE_TOOL_NAMES,
  LS_TOOL_NAMES,
  NON_SCROLLBACK_TOOL_IDS,
  PARENT_SUBAGENT_TOOL_NAMES,
  READ_TOOL_NAMES,
  resolveScrollbackToolRenderer,
  resolveToolId,
  SESSION_TOOL_NAMES,
  SHELL_PROCESS_TOOL_NAMES,
  SHELL_TOOL_NAMES,
  TASK_TOOL_NAMES,
  TOOL_CAPABILITIES,
  WEB_FETCH_TOOL_NAMES,
  WEB_SEARCH_TOOL_NAMES,
  WORKFLOW_LAUNCH_TOOL_NAMES,
  WORKFLOW_TOOL_NAMES,
  WRITE_TOOL_NAMES,
  type ScrollbackToolRenderer,
  type ToolCallOrigin,
  type ToolKind,
} from './tool-capabilities.js';
import type {
  CommandMeta,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from './commands.js';
import type {
  AgentEntry,
  EffortEntry,
  ModelEntry,
  KasConfigOrigin,
} from '../utils/kas-config-options.js';
import type { KasCommand } from '../kas-commands.js';
import type { SessionsChangedNotification } from './session-client.js';
import type { ContextBreakdownData } from './context.js';
import type { SessionRepositoryEntry } from '../utils/session-repositories.js';
import type {
  UserInputRequest,
  UserInputResponse,
  PolicyDenialInfo,
} from '@kiro/acp-type-covenant';
import type { WorkflowProgressEvent } from './workflow.js';

export enum AgentEventType {
  Content = 'content',
  Thought = 'thought',
  UserMessage = 'user_message',
  ToolCall = 'tool_call',
  ToolCallUpdate = 'tool_call_update',
  ToolCallFinished = 'tool_call_finished',
  ApprovalRequest = 'approval_request',
  QuestionRequest = 'question_request',
  CommandsUpdate = 'commands_update',
  PromptsUpdate = 'prompts_update',
  SkillsUpdate = 'skills_update',
  SteeringUpdate = 'steering_update',
  ContextUsage = 'context_usage',
  ContextBreakdownUpdate = 'context_breakdown_update',
  Metadata = 'metadata',
  CompactionStatus = 'compaction_status',
  McpServerInitFailure = 'mcp_server_init_failure',
  RateLimitError = 'rate_limit_error',
  RetryWarning = 'retry_warning',
  AuthError = 'auth_error',
  SessionError = 'session_error',
  AgentSwitched = 'agent_switched',
  AgentNotFound = 'agent_not_found',
  AgentConfigError = 'agent_config_error',
  TurnSummary = 'turn_summary',
  McpOauthRequest = 'mcp_oauth_request',
  McpServerInitialized = 'mcp_server_initialized',
  McpGovernanceDisabled = 'mcp_governance_disabled',
  WebToolsGovernanceDisabled = 'web_tools_governance_disabled',
  KasCommandsDiscovered = 'kas_commands_discovered',
  EffortUpdate = 'effort_update',
  KasAgentsUpdate = 'agents_update',
  KasModelConfigUpdate = 'model_config_update',
  SteeringQueued = 'steering_queued',
  SteeringConsumed = 'steering_consumed',
  SteeringCleared = 'steering_cleared',
  HooksUpdate = 'hooks_update',
  ToolsUpdate = 'tools_update',
  McpServersUpdate = 'mcp_servers_update',
  McpServerSnapshot = 'mcp_server_snapshot',
  McpRegistrySnapshot = 'mcp_registry_snapshot',
  GoalStatus = 'goal_status',
  KasMessageIdAssigned = 'kas_message_id_assigned',
  ModelRefusal = 'model_refusal',
  SessionRosterDelta = 'session_roster_delta',
  SpecPhaseCheckpoint = 'spec_phase_checkpoint',
  WorkflowProgress = 'workflow_progress',
  TurnStart = 'turn_start',
  TurnEnd = 'turn_end',
  SessionRepositoriesUpdate = 'session_repositories_update',
  SystemNotice = 'system_notice',
}

export enum ContentType {
  Text = 'text',
  Image = 'image',
  ResourceLink = 'resource_link',
}

// SACP ToolCallStatus values
export enum ToolCallStatus {
  InProgress = 'in_progress',
  Completed = 'completed',
  Failed = 'failed',
}

export interface ToolCallLocation {
  path: string;
  line?: number;
}

/**
 * A file diff produced by a tool call.
 * Carried on tool messages so the Write component can render a diff
 * without parsing tool args out of a JSON-encoded `content` blob.
 */
export interface ToolDiff {
  path: string;
  newText: string;
  oldText?: string;
}

/**
 * Wire shape of a `diff` entry inside an ACP `ToolCallContent` array.
 * Matches `(Diff & { type: 'diff' })` from the ACP SDK.
 */
export type ToolCallDiffContent = ToolDiff & { type: 'diff' };

/**
 * Extract a `ToolDiff` from a `ToolCall` event, if one is implied.
 *
 * Two sources, in priority order:
 *   1. `event.toolContent[0]` — explicit diff content from the agent
 *      (e.g. KAS sends this on the initial tool_call for fs_write/str_replace).
 *   2. `event.kind === 'edit'` with `oldStr`/`newStr`/`path` in args — the V2
 *      shape, where the LLM's tool input itself describes the diff.
 *
 * Returns undefined when no diff can be derived, in which case the tool will
 * not render a diff preview during approval.
 */
export function deriveToolDiff(event: ToolCallEvent): ToolDiff | undefined {
  if (toolDiffPolicy(event.name, event.kind, event.origin) === 'none') {
    return undefined;
  }
  const wireDiff = event.toolContent?.[0];
  if (wireDiff) {
    return {
      path: wireDiff.path,
      newText: wireDiff.newText,
      oldText: wireDiff.oldText,
    };
  }
  if (event.kind === 'edit') {
    const args = event.args as Record<string, unknown>;
    const path = typeof args.path === 'string' ? args.path : undefined;
    if (!path) return undefined;
    const oldStr = typeof args.oldStr === 'string' ? args.oldStr : undefined;
    const newStr = typeof args.newStr === 'string' ? args.newStr : undefined;
    const content =
      typeof args.text === 'string'
        ? args.text
        : typeof args.content === 'string'
          ? args.content
          : undefined;
    const newText = newStr ?? content ?? '';
    return { path, newText, oldText: oldStr };
  }
  return undefined;
}

export type ContentChunk =
  | { type: ContentType.Text; text: string }
  | { type: ContentType.Image; image: any }
  | { type: ContentType.ResourceLink; link: any };

export type ToolCallResult =
  | { status: 'success'; output: any }
  | { status: 'error'; error: string; output?: any }
  | { status: 'cancelled'; output?: any };

export enum ApprovalOptionId {
  AllowOnce = 'allow_once',
  AllowAlways = 'allow_always',
  RejectOnce = 'reject_once',
  RejectAlways = 'reject_always',
}

export interface PermissionOption {
  kind: ApprovalOptionId;
  name: string;
  optionId: string;
}

export interface TrustOption {
  label: string;
  display: string;
  setting_key: string;
  patterns: string[];
}

export interface PermissionResponseCancelled {
  outcome: 'cancelled';
}

export interface PermissionResponseSelected {
  outcome: 'selected';
  optionId: string;
  _meta?: {
    trustOption?: TrustOption;
    kiro?: { consent: Record<string, unknown> };
  };
}

export type PermissionResponse =
  | PermissionResponseCancelled
  | PermissionResponseSelected;

/** KAS consent context — sent by the agent in _meta.kiro.consent */
export interface ConsentContext {
  capability?: string;
  resource?: string;
  askType?: 'explicit' | 'implicit';
  triggeringResource?: string;
  matchedRule?: string;
  scope?: string;
  source?: string;
  workspaceRoot?: string;
}

export interface ApprovalRequestInfo {
  /**
   * Session that owns the backend permission request. This is separate from
   * `sessionId`, which routes visible subagent approvals in the UI.
   */
  originSessionId?: string;
  sessionId?: string;
  toolCall: {
    toolCallId: string;
    title?: string;
    rawInput?: unknown;
    /** Canonical identity used by registry-driven approval rendering. */
    name?: string;
    kind?: ToolKind;
    origin?: ToolCallOrigin;
  };
  /**
   * Stable KAS tool identifier from `_meta.kiro.toolId`, when available.
   */
  toolId?: string;
  permissionOptions: PermissionOption[];
  trustOptions?: TrustOption[];
  consentContext?: ConsentContext;
  resolve: (response: PermissionResponse) => void;
}

export interface QuestionRequestInfo extends UserInputRequest {
  resolve: (response: UserInputResponse) => void;
}

export interface KiroPipelineStage {
  name: string;
  role: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  dependsOn: string[];
  agentSubtaskId: string | null;
}

export interface KiroMeta {
  /** Discriminator for per-stage events. The pipeline parent itself has no
   *  `kind` — it is identified by the presence of `pipeline`. */
  kind?: 'agent-subtask' | 'system-notification' | 'workflow-progress';
  /**
   * KAS marker for a model-only turn initiated by the agent rather than the
   * user. Older persisted transcripts may carry this transient marker on the
   * assistant rows even though newer producers should use `visibility`.
   */
  agentInitiated?: boolean;
  /** Render visibility supplied by KAS. `hidden` rows never enter chat. */
  visibility?: string;
  agentSubtaskId?: string;
  pipeline?: {
    groupId: string;
    stages: KiroPipelineStage[];
  };
  toolName?: string;
  toolId?: string;
  /** Content-policy refusal marker on a KAS message chunk; all fields optional. */
  refusal?: {
    category?: string;
    explanation?: string;
    recommendedModel?: string;
  };
  mcpServerName?: string;
  /** Workflow child ownership stamped on every persisted step event. */
  workflow?: {
    workflowId: string;
    workflowName?: string;
    nodeId: string;
    nodePath?: readonly string[];
    type?: 'step';
    iteration?: number;
    branchId?: string;
  };
  /** Typed identity for workflow notifications and persisted lifecycle rows. */
  notification?: {
    kind: 'system-notification' | 'workflow-progress';
    status?: string;
    workflowId?: string;
    agentName?: string;
    nodeName?: string;
    notifyId?: string;
    eventType?: string;
  };
  messageId?: string;
  timestamp?: string;
  steeringClearedIds?: readonly string[];
  /**
   * Permission-policy denial surfaced on a rejected tool call's
   * `_meta.kiro.policyDenial` (KAS trust v2). Mirrors the IDE's
   * `PolicyDenialDetails` card: which rule blocked the call and where it is
   * defined. The canonical shape is defined once in `@kiro/acp-type-covenant`
   * (capabilities/trust/types.ts) and imported here for unmarshalling rather
   * than redefined.
   */
  policyDenial?: PolicyDenialInfo;
  /**
   * Infrastructure-safety block surfaced on a gated tool call's
   * `_meta.kiro.safetyOverride`. Mirrors the IDE's `SafetyDenialDetails` card:
   * the violated rule(s) and the tool.
   *
   * This shape has no covenant type to import — the agent defines it inline in
   * `acp-safety-override-permission.ts` (it is not promoted to
   * `@kiro/acp-type-covenant`), so the source of truth for these fields is that
   * file's `safetyOverrideBase` + `SafetyOverrideDecision`. `decision` is stamped
   * ONLY on the terminal `tool_call_update`; the pending card carries the base
   * fields without it. `deriveToolDenial` gates the "Blocked" card on `decision`
   * so it neither shows while pending nor persists after an `allowed` override.
   */
  safetyOverride?: {
    kind?: 'infra-safety';
    toolName: string;
    reason: string;
    blockedProperties?: readonly string[];
    decision?: 'allowed' | 'denied' | 'cancelled' | 'error';
  };
}

export interface AgentContentEvent {
  type: AgentEventType.Content;
  id: string;
  content: ContentChunk;
  /** True when this content is thinking/reasoning, not the final response */
  _thinking?: boolean;
  meta?: { kiro?: KiroMeta };
}

export interface AgentThoughtEvent {
  type: AgentEventType.Thought;
  id: string;
  content: ContentChunk;
  /** KAS subagent discriminator. Carried so a pipeline stage's reasoning can be
   *  routed to its subtask session instead of bleeding into the main agent's
   *  thinking block. Mirrors AgentContentEvent. */
  meta?: { kiro?: KiroMeta };
}

export interface UserMessageEvent {
  type: AgentEventType.UserMessage;
  id: string;
  content: ContentChunk;
  meta?: { kiro?: KiroMeta };
}

export interface ToolCallEvent {
  type: AgentEventType.ToolCall;
  id: string;
  name: string;
  /** Source family retained when KAS strips an MCP title's `@server/` prefix. */
  origin?: ToolCallOrigin;
  /** Unmodified ACP title when normalization changed the displayed tool name. */
  originalTitle?: string;
  kind?: ToolKind;
  args: Record<string, unknown>;
  toolContent?: Array<ToolCallDiffContent>;
  locations?: ToolCallLocation[];
  /** Session ID of the subagent that made this tool call (if from a subagent) */
  sessionId?: string;
  /**
   * True when this event is a client-synthesized replay of a call already
   * known to the store (e.g. a rejected-before-exec tool whose only wire
   * representation was a failed update). Consumers must treat it as a
   * re-emission of the existing call, never as a new call reusing the id.
   */
  synthesized?: boolean;
  meta?: { kiro?: KiroMeta };
}

export interface ToolCallUpdateEvent {
  type: AgentEventType.ToolCallUpdate;
  id: string;
  content: ContentChunk;
  meta?: { kiro?: KiroMeta };
}

export interface ToolCallFinishedEvent {
  type: AgentEventType.ToolCallFinished;
  id: string;
  result: ToolCallResult;
  toolContent?: Array<ToolCallDiffContent>;
  meta?: { kiro?: KiroMeta };
}

export interface ApprovalRequestEvent {
  type: AgentEventType.ApprovalRequest;
  value: ApprovalRequestInfo;
}

export interface QuestionRequestEvent {
  type: AgentEventType.QuestionRequest;
  value: QuestionRequestInfo;
}

export interface CommandsUpdateEvent {
  type: AgentEventType.CommandsUpdate;
  commands: Array<{
    name: string;
    description: string;
    meta?: CommandMeta;
  }>;
  mcpServers?: Array<{
    name: string;
    status: string;
    toolCount: number;
  }>;
}

export interface PromptsUpdateEvent {
  type: AgentEventType.PromptsUpdate;
  prompts: PromptEntry[];
}

export interface SkillsUpdateEvent {
  type: AgentEventType.SkillsUpdate;
  skills: SkillEntry[];
}

export interface SteeringUpdateEvent {
  type: AgentEventType.SteeringUpdate;
  steering: SteeringEntry[];
}

export interface ContextUsageEvent {
  type: AgentEventType.ContextUsage;
  percent: number | null;
}

export interface ContextBreakdownUpdateEvent {
  type: AgentEventType.ContextBreakdownUpdate;
  breakdown: ContextBreakdownData;
}

export interface KasMessageIdAssignedEvent {
  type: AgentEventType.KasMessageIdAssigned;
  kasMessageId: string;
}

export interface EffortUpdateEvent {
  type: AgentEventType.EffortUpdate;
  effort: string | null;
}

/**
 * Available KAS agent list, parsed from the `mode` configOption. The current
 * selection is delivered separately via `AgentSwitched` (mid-session) and the
 * session result (new/load), so this event carries only the list.
 */
export interface KasAgentsUpdateEvent {
  type: AgentEventType.KasAgentsUpdate;
  agents: AgentEntry[];
}

/**
 * The models (and model-specific efforts) parsed from a single KAS
 * `configOptions` payload (session/new, session/load, set_config_option
 * response, or config_option_update notification).
 *
 * `origin` identifies the emitting call site so the store can decide whether to
 * apply a per-model effort default. Required in the case where KAS autonomously
 * changes the model mid-session.
 */
export interface KasModelConfigUpdateEvent {
  type: AgentEventType.KasModelConfigUpdate;
  models: ModelEntry[];
  currentModelId?: string;
  efforts: EffortEntry[];
  currentLevel: string | null;
  origin: KasConfigOrigin;
}

export interface HooksUpdateEvent {
  type: AgentEventType.HooksUpdate;
  hooks: Array<{
    name?: string;
    trigger: string;
    command: string;
    matcher?: string;
  }>;
}

/**
 * Emitted when KAS pushes `_kiro/tools/didChange`. Carries the full current
 * tool listing for the session (tag-based for KAS — no per-tool status).
 * Shape matches `ToolInfo` structurally; inlined to avoid a circular import
 * with the app store (mirrors `HooksUpdateEvent`).
 */
export interface ToolsUpdateEvent {
  type: AgentEventType.ToolsUpdate;
  tools: Array<{
    name: string;
    source: string;
    description: string;
    status?: 'allowed' | 'requires-approval' | 'denied';
  }>;
  /** True when the originating notification carried the active session's id
   *  (KAS >= 0.26.14 tags every push). Feeds cloud snapshot readiness. */
  sessionTagged?: boolean;
}

export interface McpServersUpdateEvent {
  type: AgentEventType.McpServersUpdate;
  servers: Array<{ name: string; status: string; toolCount: number }>;
}

export interface McpServerSnapshotEvent {
  type: AgentEventType.McpServerSnapshot;
  servers: Array<{
    name: string;
    status: 'running' | 'loading' | 'failed' | 'disabled' | 'auth-required';
    toolCount: number;
  }>;
  /** True when the originating notification carried the active session's id
   *  (KAS >= 0.26.14 tags every push). Feeds cloud snapshot readiness. */
  sessionTagged?: boolean;
}

export interface McpRegistrySnapshotEvent {
  type: AgentEventType.McpRegistrySnapshot;
  registryServers: Array<{
    name: string;
    status: 'disabled';
    toolCount: number;
    version?: string;
    description?: string;
    enabled?: boolean;
  }>;
}

export interface GoalStatusEvent {
  type: AgentEventType.GoalStatus;
  state: string;
  iteration: number;
  maxIterations: number;
  message?: string;
  elapsedSecs?: number;
}

export interface MetadataEvent {
  type: AgentEventType.Metadata;
  totalTokens?: number;
  inputTokens?: number;
  outputTokens?: number;
  cachedTokens?: number;
}

export interface CompactionStatusEvent {
  type: AgentEventType.CompactionStatus;
  status: 'started' | 'completed' | 'failed';
  attemptId?: number;
  error?: string;
  summary?: string;
}

/**
 * A raw `_kiro/sessions/changed` roster delta, forwarded unmerged. The app
 * store owns the roster state and derives the active session's cloud status
 * from it. Local sessions never produce one (the channel is cloud-only).
 */
export interface SessionRosterDeltaEvent {
  type: AgentEventType.SessionRosterDelta;
  delta: SessionsChangedNotification;
}

/**
 * A spec phase whose document a checkpoint can conclude.
 *
 * Mirrors the `_kiro/spec/phaseCheckpoint` payload the agent sends. Declared
 * here because the pinned covenant predates that notification; these must stay
 * in step with it, and should be imported from the covenant once its version
 * carries them.
 */
export const SPEC_CHECKPOINT_PHASES = [
  'requirements',
  'design',
  'tasks',
  // A bugfix spec opens with bugfix.md in place of requirements.
  'bugfix',
] as const;

export type SpecCheckpointPhase = (typeof SPEC_CHECKPOINT_PHASES)[number];

/**
 * A spec phase's document is written and the agent is about to ask whether to
 * proceed. What comes next isn't carried here — the agent names it in the
 * question it asks.
 */
export interface SpecPhaseCheckpointEvent {
  type: AgentEventType.SpecPhaseCheckpoint;
  featureName: string;
  phase: SpecCheckpointPhase;
  artifactPath: string;
}

/**
 * The session's bound-repository set, as KAS reports it over the ACP wire
 * (`_meta.kiro.repositories` on a `session_info_update`). Pushed when the
 * sandbox attaches/detaches repos mid-session, so the cloud footer tracks the
 * sandbox's actual workspace instead of only the create-time bindings.
 * Cloud-only: local sessions never produce one.
 */
export interface SessionRepositoriesUpdateEvent {
  type: AgentEventType.SessionRepositoriesUpdate;
  repositories: SessionRepositoryEntry[];
}

export interface McpServerInitFailureEvent {
  type: AgentEventType.McpServerInitFailure;
  serverName: string;
  error: string;
}

export interface RateLimitErrorEvent {
  type: AgentEventType.RateLimitError;
  message: string;
}

export interface RetryWarningEvent {
  type: AgentEventType.RetryWarning;
  attempt: number;
  maxAttempts: number;
  delaySecs: number;
  message: string;
}

export interface AuthErrorEvent {
  type: AgentEventType.AuthError;
  errorType: string;
  message: string;
}

export interface SessionErrorEvent {
  type: AgentEventType.SessionError;
  errorType: string;
  message: string;
  pid?: number;
}

export interface AgentSwitchedEvent {
  type: AgentEventType.AgentSwitched;
  agentName: string;
  previousAgentName?: string;
  welcomeMessage?: string;
  model?: string;
}

/** How a file that declared an agent failed to load. */
export type AgentConfigRejectionReason =
  | 'cli_only_agent'
  | 'invalid_config'
  | 'unreadable'
  | 'internal_error';

/** A file that claimed an agent id the backend then could not use. */
export interface RejectedAgentConfig {
  path: string;
  /** Absent when the backend sent a code this client does not know. */
  reasonCode?: AgentConfigRejectionReason;
  error: string;
}

export interface AgentNotFoundEvent {
  type: AgentEventType.AgentNotFound;
  requestedAgent: string;
  fallbackAgent: string;
  /**
   * The rejected file that claimed `requestedAgent`. Absent means no file
   * claimed it, i.e. the agent really is missing rather than defective.
   */
  skipped?: RejectedAgentConfig;
}

export interface AgentConfigErrorEvent {
  type: AgentEventType.AgentConfigError;
  path?: string;
  error: string;
}

export interface MeteringUsage {
  value: number;
  unit: string;
  unitPlural: string;
}

export interface TurnSummaryEvent {
  type: AgentEventType.TurnSummary;
  meteringUsage: MeteringUsage[];
  turnDurationMs?: number;
}

export interface TurnStartEvent {
  type: AgentEventType.TurnStart;
}

export interface TurnEndEvent {
  type: AgentEventType.TurnEnd;
  stopReason?: string;
}

/**
 * The model stopped due to a content-policy refusal or a content-filtered stop
 * reason. Carries the provider-supplied explanation when available so the TUI
 * can surface a notice to the user.
 */
export interface ModelRefusalEvent {
  type: AgentEventType.ModelRefusal;
  stopReason?: string;
  category?: string;
  explanation?: string;
  recommendedModel?: string;
}

export interface McpOauthRequestEvent {
  type: AgentEventType.McpOauthRequest;
  serverName: string;
  oauthUrl: string;
}

/**
 * A client-originated notice to surface as a system line in the transcript
 * (e.g. a cloud session silently reverting a mode the client just set).
 * `success` mirrors the `addSystemMessage` convention (false = warning tone).
 */
export interface SystemNoticeEvent {
  type: AgentEventType.SystemNotice;
  message: string;
  success: boolean;
}

export interface McpServerInitializedEvent {
  type: AgentEventType.McpServerInitialized;
  serverName: string;
}

export interface McpGovernanceDisabledEvent {
  type: AgentEventType.McpGovernanceDisabled;
  apiFailure: boolean;
}

export interface WebToolsGovernanceDisabledEvent {
  type: AgentEventType.WebToolsGovernanceDisabled;
  apiFailure: boolean;
}

export type AuthErrorType = string;
export type SessionErrorType = string;

export interface KasCommandsDiscoveredEvent {
  type: AgentEventType.KasCommandsDiscovered;
  commands: KasCommand[];
}

export interface SteeringQueuedEvent {
  type: AgentEventType.SteeringQueued;
  message: string;
}

export interface SteeringConsumedEvent {
  type: AgentEventType.SteeringConsumed;
  content: string;
}

export interface SteeringClearedEvent {
  type: AgentEventType.SteeringCleared;
}

export interface WorkflowProgressStreamEvent {
  type: AgentEventType.WorkflowProgress;
  id: string;
  event: WorkflowProgressEvent;
}

export type AgentStreamEvent =
  | AgentContentEvent
  | AgentThoughtEvent
  | UserMessageEvent
  | ToolCallEvent
  | ToolCallUpdateEvent
  | ToolCallFinishedEvent
  | ApprovalRequestEvent
  | QuestionRequestEvent
  | CommandsUpdateEvent
  | PromptsUpdateEvent
  | SkillsUpdateEvent
  | SteeringUpdateEvent
  | ContextUsageEvent
  | ContextBreakdownUpdateEvent
  | KasMessageIdAssignedEvent
  | MetadataEvent
  | CompactionStatusEvent
  | McpServerInitFailureEvent
  | RateLimitErrorEvent
  | RetryWarningEvent
  | AuthErrorEvent
  | SessionErrorEvent
  | AgentSwitchedEvent
  | AgentNotFoundEvent
  | AgentConfigErrorEvent
  | TurnSummaryEvent
  | TurnStartEvent
  | TurnEndEvent
  | McpOauthRequestEvent
  | McpServerInitializedEvent
  | McpGovernanceDisabledEvent
  | WebToolsGovernanceDisabledEvent
  | SteeringQueuedEvent
  | SteeringConsumedEvent
  | SteeringClearedEvent
  | KasCommandsDiscoveredEvent
  | EffortUpdateEvent
  | KasAgentsUpdateEvent
  | KasModelConfigUpdateEvent
  | HooksUpdateEvent
  | ToolsUpdateEvent
  | McpServersUpdateEvent
  | McpServerSnapshotEvent
  | McpRegistrySnapshotEvent
  | GoalStatusEvent
  | ModelRefusalEvent
  | SessionRosterDeltaEvent
  | SpecPhaseCheckpointEvent
  | WorkflowProgressStreamEvent
  | SessionRepositoriesUpdateEvent
  | SystemNoticeEvent;
