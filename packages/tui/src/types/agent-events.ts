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
export const WRITE_TOOL_NAMES: Set<string> = new Set([ToolNameAlias.FsWrite, ToolNameAlias.Write]);
export const READ_TOOL_NAMES: Set<string> = new Set([ToolNameAlias.FsRead, ToolNameAlias.Read]);
export const SHELL_TOOL_NAMES: Set<string> = new Set([ToolNameAlias.ExecuteBash, ToolNameAlias.ExecuteCmd, ToolNameAlias.Shell]);

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
  args: Record<string, unknown>;
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

export type AgentStreamEvent =
  | AgentContentEvent
  | ToolCallEvent
  | ToolCallUpdateEvent
  | ToolCallFinishedEvent
  | ApprovalRequestEvent
  | CommandsUpdateEvent
  | ContextUsageEvent
  | MetadataEvent;
