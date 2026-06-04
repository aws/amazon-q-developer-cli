import { ToolNameAlias } from '../../e2e_tests/types/agent.js';
import type { BuiltinToolId } from './tool-status.js';
import type {
  CommandMeta,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from './commands.js';
import type { KasCommand } from '../kas-commands.js';

export enum AgentEventType {
  Content = 'content',
  Thought = 'thought',
  UserMessage = 'user_message',
  ToolCall = 'tool_call',
  ToolCallUpdate = 'tool_call_update',
  ToolCallFinished = 'tool_call_finished',
  ApprovalRequest = 'approval_request',
  CommandsUpdate = 'commands_update',
  PromptsUpdate = 'prompts_update',
  SkillsUpdate = 'skills_update',
  SteeringUpdate = 'steering_update',
  ContextUsage = 'context_usage',
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
  KasCommandsDiscovered = 'kas_commands_discovered',
  EffortUpdate = 'effort_update',
  HooksUpdate = 'hooks_update',
  GoalStatus = 'goal_status',
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

// Built-in tool name sets for matching
export const WRITE_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.FsWrite,
  ToolNameAlias.Write,
  'str_replace',
  'fs_append',
  'delete_file',
  'Write File',
  'Replace in File',
  'Append to File',
  'Delete File',
]);
export const READ_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.FsRead,
  ToolNameAlias.Read,
  'read_file',
  'read_files',
  'list_directory',
  'Read File',
  'Read Files',
  'List Directory',
]);
export const SHELL_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.ExecuteBash,
  ToolNameAlias.ExecuteCmd,
  ToolNameAlias.Shell,
  'control_bash_process',
  'control_pwsh_process',
  'run_command',
  'Run Command',
  'Control Process',
]);
export const WEB_SEARCH_TOOL_NAMES: Set<string> = new Set([
  'web_search',
  'Searching the web',
]);
export const WEB_FETCH_TOOL_NAMES: Set<string> = new Set([
  'web_fetch',
  'Fetching web content',
  'Fetch URL',
]);
export const GREP_TOOL_NAMES: Set<string> = new Set([
  'grep',
  'grep_search',
  'Grep Search',
]);
export const GLOB_TOOL_NAMES: Set<string> = new Set([
  'glob',
  'file_search',
  'File Search',
]);
// TODO: Remove LS_TOOL_NAMES and IMAGE_READ_TOOL_NAMES once enough time has passed that users
// are unlikely to load saved conversations containing old ls/imageRead tool calls.
// These tools are now part of the unified fs_read tool (mode: "Directory" / "Image").
export const LS_TOOL_NAMES: Set<string> = new Set([ToolNameAlias.Ls]);
export const CODE_TOOL_NAMES: Set<string> = new Set([
  'code',
  'Code Intelligence',
]);
export const IMAGE_READ_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.ImageRead,
  'imageRead',
]);
export const SESSION_TOOL_NAMES: Set<string> = new Set([
  'session_management',
  'subagent',
  'agent_crew',
  'invoke_sub_agent',
  'subagent_response',
  'Invoke Agent',
  'Subagent Response',
]);
export const INTROSPECT_TOOL_NAMES: Set<string> = new Set([
  'introspect',
  'Introspect',
]);
export const TASK_TOOL_NAMES: Set<string> = new Set([
  'task',
  'todo_list',
  'todo',
  'Task List',
]);

/** Map a wire tool name to its BuiltinToolId, or undefined for MCP/unknown tools. */
export function resolveToolId(name: string): BuiltinToolId | undefined {
  if (WRITE_TOOL_NAMES.has(name)) return 'write';
  if (READ_TOOL_NAMES.has(name)) return 'read';
  if (SHELL_TOOL_NAMES.has(name)) return 'shell';
  if (WEB_SEARCH_TOOL_NAMES.has(name)) return 'web_search';
  if (WEB_FETCH_TOOL_NAMES.has(name)) return 'web_fetch';
  if (GREP_TOOL_NAMES.has(name)) return 'grep';
  if (GLOB_TOOL_NAMES.has(name)) return 'glob';
  // TODO: Remove ls/image_read resolution once legacy tool names are cleaned up.
  if (LS_TOOL_NAMES.has(name)) return 'ls';
  if (CODE_TOOL_NAMES.has(name)) return 'code';
  if (IMAGE_READ_TOOL_NAMES.has(name)) return 'image_read';
  if (TASK_TOOL_NAMES.has(name)) return 'task';
  return undefined;
}

export type ToolKind = 'edit' | 'read' | 'shell' | 'grep' | 'glob' | string;

/**
 * Map an ACP `ToolKind` to a `BuiltinToolId` for display-label purposes.
 *
 * Used as a fallback when a tool's wire name isn't a known builtin but its
 * `kind` is — mirroring the kind-based routing in ToolUseMessage so a failed
 * read/edit renders a friendly label ("Read"/"Write") instead of the raw wire
 * name (e.g. KAS sends "read_files"/"Replace in File"). Only the kinds that
 * routing keys on are mapped; everything else falls back to the raw name.
 */
export function kindToToolId(
  kind: ToolKind | undefined
): BuiltinToolId | undefined {
  switch (kind) {
    case 'read':
      return 'read';
    case 'edit':
      return 'write';
    case 'execute':
      return 'shell';
    case 'search':
      return 'grep';
    default:
      return undefined;
  }
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
  | { status: 'error'; error: string }
  | { status: 'cancelled' };

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
  _meta?: { trustOption?: TrustOption };
}

export type PermissionResponse =
  | PermissionResponseCancelled
  | PermissionResponseSelected;

export interface ApprovalRequestInfo {
  sessionId?: string;
  toolCall: { toolCallId: string };
  permissionOptions: PermissionOption[];
  trustOptions?: TrustOption[];
  resolve: (response: PermissionResponse) => void;
}

export interface AgentContentEvent {
  type: AgentEventType.Content;
  id: string;
  content: ContentChunk;
}

export interface AgentThoughtEvent {
  type: AgentEventType.Thought;
  id: string;
  content: ContentChunk;
}

export interface UserMessageEvent {
  type: AgentEventType.UserMessage;
  id: string;
  content: ContentChunk;
}

export interface ToolCallEvent {
  type: AgentEventType.ToolCall;
  id: string;
  name: string;
  kind?: ToolKind;
  args: Record<string, unknown>;
  toolContent?: Array<ToolCallDiffContent>;
  locations?: ToolCallLocation[];
  /** Session ID of the subagent that made this tool call (if from a subagent) */
  sessionId?: string;
}

export interface ToolCallUpdateEvent {
  type: AgentEventType.ToolCallUpdate;
  id: string;
  content: ContentChunk;
}

export interface ToolCallFinishedEvent {
  type: AgentEventType.ToolCallFinished;
  id: string;
  result: ToolCallResult;
  toolContent?: Array<ToolCallDiffContent>;
}

export interface ApprovalRequestEvent {
  type: AgentEventType.ApprovalRequest;
  value: ApprovalRequestInfo;
}

export interface CommandsUpdateEvent {
  type: AgentEventType.CommandsUpdate;
  commands: Array<{
    name: string;
    description: string;
    meta?: CommandMeta;
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
  percent: number;
}

export interface EffortUpdateEvent {
  type: AgentEventType.EffortUpdate;
  effort: string | null;
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
  error?: string;
  summary?: string;
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

export interface AgentNotFoundEvent {
  type: AgentEventType.AgentNotFound;
  requestedAgent: string;
  fallbackAgent: string;
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

export interface McpOauthRequestEvent {
  type: AgentEventType.McpOauthRequest;
  serverName: string;
  oauthUrl: string;
}

export interface McpServerInitializedEvent {
  type: AgentEventType.McpServerInitialized;
  serverName: string;
}

export interface McpGovernanceDisabledEvent {
  type: AgentEventType.McpGovernanceDisabled;
  apiFailure: boolean;
}

export type AuthErrorType = string;
export type SessionErrorType = string;

export interface KasCommandsDiscoveredEvent {
  type: AgentEventType.KasCommandsDiscovered;
  commands: KasCommand[];
}

export type AgentStreamEvent =
  | AgentContentEvent
  | AgentThoughtEvent
  | UserMessageEvent
  | ToolCallEvent
  | ToolCallUpdateEvent
  | ToolCallFinishedEvent
  | ApprovalRequestEvent
  | CommandsUpdateEvent
  | PromptsUpdateEvent
  | SkillsUpdateEvent
  | SteeringUpdateEvent
  | ContextUsageEvent
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
  | McpOauthRequestEvent
  | McpServerInitializedEvent
  | McpGovernanceDisabledEvent
  | KasCommandsDiscoveredEvent
  | EffortUpdateEvent
  | HooksUpdateEvent
  | GoalStatusEvent;
