import { ToolNameAlias } from '../../e2e_tests/types/agent.js';

export enum AgentEventType {
  Content = 'content',
  ToolCall = 'tool_call',
  ToolCallUpdate = 'tool_call_update',
  ToolCallFinished = 'tool_call_finished',
  ApprovalRequest = 'approval_request',
  CommandsUpdate = 'commands_update',
  ContextUsage = 'context_usage',
  Metadata = 'metadata',
  CompactionStatus = 'compaction_status',
  McpServerInitFailure = 'mcp_server_init_failure',
  RateLimitError = 'rate_limit_error',
  AuthError = 'auth_error',
  SessionError = 'session_error',
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
]);
export const READ_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.FsRead,
  ToolNameAlias.Read,
]);
export const SHELL_TOOL_NAMES: Set<string> = new Set([
  ToolNameAlias.ExecuteBash,
  ToolNameAlias.ExecuteCmd,
  ToolNameAlias.Shell,
]);
export const WEB_SEARCH_TOOL_NAMES: Set<string> = new Set([
  'web_search',
  'Searching the web',
]);
export const WEB_FETCH_TOOL_NAMES: Set<string> = new Set([
  'web_fetch',
  'Fetching web content',
]);
export const GREP_TOOL_NAMES: Set<string> = new Set(['grep', 'grep_search']);
export const GLOB_TOOL_NAMES: Set<string> = new Set(['glob', 'file_search']);
export const LS_TOOL_NAMES: Set<string> = new Set([ToolNameAlias.Ls]);
export const CODE_TOOL_NAMES: Set<string> = new Set(['code']);

export type ToolKind = 'edit' | 'read' | 'shell' | 'grep' | 'glob' | string;

export interface ToolCallLocation {
  path: string;
  line?: number;
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

export interface PermissionResponseCancelled {
  outcome: 'cancelled';
}

export interface PermissionResponseSelected {
  outcome: 'selected';
  optionId: string;
}

export type PermissionResponse =
  | PermissionResponseCancelled
  | PermissionResponseSelected;

export interface ApprovalRequestInfo {
  toolCall: { toolCallId: string };
  permissionOptions: PermissionOption[];
  resolve: (response: PermissionResponse) => void;
}

export interface AgentContentEvent {
  type: AgentEventType.Content;
  id: string;
  content: ContentChunk;
}

export interface ToolCallEvent {
  type: AgentEventType.ToolCall;
  id: string;
  name: string;
  kind?: ToolKind;
  args: Record<string, unknown>;
  toolContent?: Array<{
    type: 'diff';
    path: string;
    newText: string;
    oldText?: string;
  }>;
  locations?: ToolCallLocation[];
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
    meta?: {
      inputType?: 'text' | 'selection' | 'multiselect' | 'panel';
      optionsMethod?: string;
      hint?: string;
    };
  }>;
}

export interface ContextUsageEvent {
  type: AgentEventType.ContextUsage;
  percent: number;
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

export type AuthErrorType = string;
export type SessionErrorType = string;

export type AgentStreamEvent =
  | AgentContentEvent
  | ToolCallEvent
  | ToolCallUpdateEvent
  | ToolCallFinishedEvent
  | ApprovalRequestEvent
  | CommandsUpdateEvent
  | ContextUsageEvent
  | MetadataEvent
  | CompactionStatusEvent
  | McpServerInitFailureEvent
  | RateLimitErrorEvent
  | AuthErrorEvent
  | SessionErrorEvent;
