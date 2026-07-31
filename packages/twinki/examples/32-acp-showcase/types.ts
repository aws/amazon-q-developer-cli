import type * as acp from "@agentclientprotocol/sdk";
export type ViewId = "agent" | "file";
export type ConnectionState = "connecting" | "ready" | "running" | "cancelling" | "error";
export type ToolState = "running" | "done" | "error";
export interface TextBlock {
  id: number;
  kind: "user" | "agent" | "thought" | "notice";
  text: string;
}
export interface ToolBlock {
  id: number;
  kind: "tool";
  toolId: string;
  title: string;
  state: ToolState;
  detail?: string;
}
export type TranscriptBlock = TextBlock | ToolBlock;
export interface PermissionChoice {
  id: string;
  label: string;
  kind: string;
}
export interface PermissionPrompt {
  id: string;
  toolName: string;
  detail?: string;
  choices: PermissionChoice[];
}
export type AcpEvent =
  | { type: "connection"; state: ConnectionState; agentName?: string }
  | { type: "session_update"; update: acp.SessionUpdate }
  | { type: "permission"; prompt: PermissionPrompt | null }
  | { type: "turn_done"; stopReason?: string }
  | { type: "error"; message: string };
export interface SessionState {
  connection: ConnectionState;
  agentName: string;
  blocks: TranscriptBlock[];
  permission: PermissionPrompt | null;
  contextPercent: number;
  mode: string;
  nextBlockId: number;
}
export interface FileEntry {
  path: string;
  relativePath: string;
  name: string;
  language?: string;
}
export interface OpenFile extends FileEntry {
  content: string;
  error?: string;
}
export interface ContextPoint {
  x: number;
  y: number;
  label: string;
  file?: FileEntry;
  pane?: ViewId;
}
