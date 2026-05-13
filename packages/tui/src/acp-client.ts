import * as acp from '@agentclientprotocol/sdk';
import { KiroClient } from '@kiro/client';
import type { Stream } from '@kiro/client';
import { logger } from './utils/logger';
import {
  getTelemetryIdentity,
  isTelemetryEnabled,
} from './utils/telemetry-identity';
import { buildKasSettings } from './utils/kas-settings';
import { maybeWrapStreamWithRecorder } from './acp-recorder';
import { spawn, type ChildProcess } from 'node:child_process';
import type { SessionClient } from './types/session-client';
import {
  AgentEventType,
  ContentType,
  ApprovalOptionId,
  ToolCallStatus,
  type AgentStreamEvent,
  type MeteringUsage,
} from './types/agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from './types/commands';
import type { ListSessionsResponse } from './types/session-client';

import packageJson from '../package.json';
import { SLASH_COMMANDS } from './slash-commands';
import { readClipboardImage } from './utils/clipboard-image';

const TUI_VERSION: string = packageJson.version;

export type AcpSessionUpdate = acp.SessionNotification['update'];

const EXT_METHODS = {
  COMMANDS_AVAILABLE: 'kiro.dev/commands/available',
  COMMANDS_EXECUTE: 'kiro.dev/commands/execute',
  COMMANDS_OPTIONS: 'kiro.dev/commands/options',
  METADATA: 'kiro.dev/metadata',
  COMPACTION_STATUS: 'kiro.dev/compaction/status',
  CLEAR_STATUS: 'kiro.dev/clear/status',
  MCP_SERVER_INIT_FAILURE: 'kiro.dev/mcp/server_init_failure',
  MCP_OAUTH_REQUEST: 'kiro.dev/mcp/oauth_request',
  MCP_SERVER_INITIALIZED: 'kiro.dev/mcp/server_initialized',
  MCP_GOVERNANCE_DISABLED: 'kiro.dev/mcp/governance_disabled',
  AGENT_NOT_FOUND: 'kiro.dev/agent/not_found',
  AGENT_CONFIG_ERROR: 'kiro.dev/agent/config_error',
  RATE_LIMIT_ERROR: 'kiro.dev/error/rate_limit',
  SUBAGENT_LIST_UPDATE: 'kiro.dev/subagent/list_update',
  SESSION_ACTIVITY: 'kiro.dev/session/activity',
  SESSION_LIST_UPDATE: 'kiro.dev/session/list_update',
  INBOX_NOTIFICATION: 'kiro.dev/session/inbox_notification',
  SESSION_LIST: 'session/list',
  SESSION_SPAWN: 'session/spawn',
  SESSION_TERMINATE: 'session/terminate',
  SESSION_ATTACH: 'session/attach',
  MESSAGE_SEND: 'message/send',
  AGENT_SWITCHED: 'kiro.dev/agent/switched',
  SESSION_UPDATE: 'kiro.dev/session/update',
} as const;

/** Subset of ACP's SessionModeState that we cache client-side.  Used for the
 *  /agent command, which is composed from the modes advertised on
 *  session/new and session/load responses (and kept in sync via
 *  current_mode_update notifications) rather than a custom extension
 *  method. */
type CachedModesState = {
  availableModes: Array<{
    id: string;
    name: string;
    description?: string | null;
    _meta?: Record<string, unknown> | null;
  }>;
  currentModeId?: string;
};

function extractCurrentAgent(
  modes?: {
    currentModeId?: string;
    availableModes?: Array<{
      id: string;
      _meta?: Record<string, unknown> | null;
    }>;
  } | null
): { name: string; welcomeMessage?: string } | undefined {
  if (!modes?.currentModeId) return undefined;
  const currentMode = modes.availableModes?.find(
    (m) => m.id === modes.currentModeId
  );
  return {
    name: modes.currentModeId,
    welcomeMessage: currentMode?._meta?.welcomeMessage as string | undefined,
  };
}

type SessionResult = {
  sessionId: string;
  currentModel?: { id: string; name: string };
  currentAgent?: { name: string; welcomeMessage?: string };
};

// ─── Shared stdio plumbing ───────────────────────────────────────────

/** Build the parsed-message ReadableStream and ndJson WritableStream from a child process. */
function buildStdioStreams(agentProcess: ChildProcess) {
  const stdin = agentProcess.stdin!;
  const stdout = agentProcess.stdout!;

  stdin.on('error', (err) => logger.error('Agent stdin error:', err));

  const writable = new WritableStream<Uint8Array>({
    async write(chunk) {
      if (stdin.destroyed || stdin.writableEnded) return;
      return new Promise<void>((resolve, reject) => {
        stdin.write(chunk, (err) => (err ? reject(err) : resolve()));
      });
    },
    close() {
      stdin.end();
    },
    abort(reason) {
      stdin.destroy(
        reason instanceof Error ? reason : new Error(String(reason))
      );
    },
  });

  let buffer = '';
  const decoder = new TextDecoder();
  let messageController: ReadableStreamDefaultController<any>;
  const parsedMessages = new ReadableStream<any>({
    start(controller) {
      messageController = controller;
    },
    cancel() {
      stdout.destroy();
    },
  });

  stdout.on('data', (chunk: Buffer) => {
    buffer += decoder.decode(chunk, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed) {
        try {
          messageController.enqueue(JSON.parse(trimmed));
        } catch (err) {
          logger.error('[pipe] Failed to parse JSON:', trimmed, err);
        }
      }
    }
  });
  stdout.on('end', () => {
    if (buffer.trim()) {
      try {
        messageController.enqueue(JSON.parse(buffer.trim()));
      } catch {
        /* ignore */
      }
    }
    messageController.close();
  });
  stdout.on('error', (err) => {
    logger.error('[pipe] stdout error:', err);
    messageController.error(err);
  });

  const dummyReadable = new ReadableStream<Uint8Array>({ start() {} });
  const ndJson = acp.ndJsonStream(writable, dummyReadable);
  return { readable: parsedMessages, writable: ndJson.writable };
}

/**
 * Narrowed view of `ChildProcess` used by `BaseAcpClient` and its
 * subclasses. Declared explicitly so the null shim used in test mode can
 * satisfy it without pretending to implement the entirety of
 * `ChildProcess`. Adding a new method here forces the mock to implement
 * it rather than silently misbehaving at runtime.
 */
export interface AgentProcess {
  readonly stdin: NodeJS.WritableStream | null;
  readonly stdout: NodeJS.ReadableStream | null;
  readonly stderr: NodeJS.ReadableStream | null;
  kill(signal?: NodeJS.Signals | number): boolean;
  /**
   * Registers an exit listener. Returns an unsubscribe function so callers
   * that may register many times (e.g. once per prompt) can detach on
   * completion and never leak listeners in mock mode where exit never
   * fires.
   */
  onExit(listener: (code: number | null) => void): () => void;
}

/** Wrap a real `ChildProcess` so it satisfies `AgentProcess`. */
function toAgentProcess(proc: ChildProcess): AgentProcess {
  // stdin/stdout/stderr are set synchronously on spawn and never change,
  // so plain property reads are sufficient (no getter indirection).
  return {
    stdin: proc.stdin,
    stdout: proc.stdout,
    stderr: proc.stderr,
    kill: (signal) => proc.kill(signal),
    onExit(listener) {
      proc.once('exit', listener);
      return () => {
        proc.off('exit', listener);
      };
    },
  };
}

/**
 * `AgentProcess` implementation for test mode where no real subprocess
 * exists. stdin/stdout/stderr are real in-memory PassThrough streams so
 * `BaseAcpClient`'s stdio existence check passes and `pipeStderr` has a
 * stream to consume (it reads nothing, since nothing writes). `onExit`
 * never fires and registers no underlying listener, so repeated
 * subscriptions cannot accumulate.
 */
export function createNullAgentProcess(): AgentProcess {
  const { PassThrough } = require('node:stream');
  return {
    stdin: new PassThrough(),
    stdout: new PassThrough(),
    stderr: new PassThrough(),
    kill: () => true,
    onExit: () => () => {},
  };
}

function pipeStderr(agentProcess: AgentProcess) {
  if (!agentProcess.stderr) return;
  let buf = '';
  agentProcess.stderr.on('data', (chunk: Buffer) => {
    buf += chunk.toString();
    const lines = buf.split('\n');
    buf = lines.pop() || '';
    for (const line of lines) {
      if (line.trim()) logger.warn('[agent-stderr]', line);
    }
  });
  agentProcess.stderr.on('end', () => {
    if (buf.trim()) logger.warn('[agent-stderr]', buf);
  });
}

function extractModel(
  models?: {
    currentModelId?: string;
    availableModels?: Array<{ modelId: string; name: string }>;
  } | null
): { id: string; name: string } | undefined {
  if (!models?.currentModelId || !models.availableModels) return undefined;
  const m = models.availableModels.find(
    (x) => x.modelId === models.currentModelId
  );
  return m ? { id: m.modelId, name: m.name } : undefined;
}

// ─── KAS model config extraction (ACP Session Config Options) ────────
//
// KAS exposes model selection through ACP's standard Session Config
// Options API, not through the (Rust-backend-specific) `models` field.
// The model option appears as:
//   { type: 'select', id: 'model', category: 'model',
//     currentValue: <id>, options: [{value, name, description?}, ...] }
//
// The shape of a flat SessionConfigSelectOption — matches ACP SDK 0.19.2.
interface ModelOption {
  value: string;
  name: string;
  description?: string;
}

/** Find the `category: 'model'` entry in a KAS configOptions array. */
function findModelConfigOption(
  configOptions: unknown
): { currentValue?: string; options: ModelOption[] } | undefined {
  if (!Array.isArray(configOptions)) return undefined;
  for (const opt of configOptions as Array<Record<string, unknown>>) {
    if (opt.category !== 'model' || opt.type !== 'select') continue;
    // `options` may be flat (SessionConfigSelectOption[]) or grouped
    // (SessionConfigSelectGroup[]). KAS currently emits flat; we only
    // support flat here. Grouped options simply yield an empty list,
    // which surfaces as "No options available" in the TUI.
    const raw = Array.isArray(opt.options) ? opt.options : [];
    const options = raw
      .filter((o: any): o is Record<string, unknown> => {
        return (
          typeof o === 'object' &&
          o !== null &&
          typeof (o as any).value === 'string' &&
          typeof (o as any).name === 'string'
        );
      })
      .map((o: any) => ({
        value: o.value as string,
        name: o.name as string,
        description:
          typeof o.description === 'string' ? o.description : undefined,
      }));
    return {
      currentValue:
        typeof opt.currentValue === 'string' ? opt.currentValue : undefined,
      options,
    };
  }
  return undefined;
}

/** Extract the currently selected model as `{id, name}` from configOptions. */
function extractModelFromConfigOptions(
  configOptions: unknown
): { id: string; name: string } | undefined {
  const modelOpt = findModelConfigOption(configOptions);
  if (!modelOpt?.currentValue) return undefined;
  const match = modelOpt.options.find((o) => o.value === modelOpt.currentValue);
  return match ? { id: match.value, name: match.name } : undefined;
}

// ─── Prompt types ────────────────────────────────────────────────────

type PromptCacheEntry = {
  name: string;
  description?: string;
  arguments: Array<{ name: string; description?: string; required?: boolean }>;
  serverName: string;
};

/** KAS command types that represent prompts (skills, steering docs, etc.) */
const PROMPT_COMMAND_TYPES = new Set(['prompt', 'skill', 'steering']);

/**
 * Map KAS _meta.kiro.type to a group name matching Rust's convention.
 * Rust uses "workspace" for file prompts and "skill" for skills.
 * KAS only provides the type, not a source field.
 */
function kasTypeToGroupName(type: string | undefined): string {
  switch (type) {
    case 'steering':
      return 'workspace';
    case 'skill':
      return 'skill';
    case 'prompt':
      return 'workspace';
    default:
      return type || '';
  }
}

// ─── Base class ──────────────────────────────────────────────────────

abstract class BaseAcpClient implements SessionClient {
  public sessionId?: string;
  protected agentProcess: AgentProcess;
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private multiSessionHandlers: Set<
    (sessionId: string, event: AgentStreamEvent) => void
  > = new Set();
  private inboxHandlers: Set<(notification: any) => void> = new Set();
  private sessionEventHandlers: Set<(event: any) => void> = new Set();
  private subagentListHandlers: Set<
    (subagents: any[], pendingStages?: any[]) => void
  > = new Set();
  protected promptsCache: PromptCacheEntry[] = [];

  constructor(agentProcess: AgentProcess) {
    this.agentProcess = agentProcess;
    if (!agentProcess.stdout || !agentProcess.stdin) {
      throw new Error('Failed to create agent process stdio streams');
    }
    pipeStderr(agentProcess);
  }

  // ── Abstract methods (differ per engine) ──

  abstract initialize(): Promise<void>;
  abstract newSession(): Promise<SessionResult>;
  abstract loadSession(sessionId: string): Promise<SessionResult>;
  abstract prompt(messages: acp.ContentBlock[]): Promise<void>;
  abstract cancel(): Promise<void>;
  abstract executeCommand(command: TuiCommand): Promise<CommandResult>;
  abstract getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse>;
  abstract setMode(modeId: string): Promise<void>;
  abstract listSessions(cwd: string): Promise<ListSessionsResponse>;
  abstract listSettings(): Promise<Record<string, unknown>>;
  abstract setSetting(key: string, value: unknown): Promise<void>;
  abstract terminateSession(sessionId: string): Promise<void>;
  abstract spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }>;
  abstract sendMessage(sessionId: string, content: string): Promise<void>;

  // ── Shared methods ──

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  onMultiSessionUpdate(
    handler: (sessionId: string, event: AgentStreamEvent) => void
  ): () => void {
    this.multiSessionHandlers.add(handler);
    return () => this.multiSessionHandlers.delete(handler);
  }

  onSubagentListUpdate(
    handler: (subagents: any[], pendingStages?: any[]) => void
  ): () => void {
    this.subagentListHandlers.add(handler);
    return () => this.subagentListHandlers.delete(handler);
  }

  onSessionEvent(handler: (event: any) => void): () => void {
    this.sessionEventHandlers.add(handler);
    return () => this.sessionEventHandlers.delete(handler);
  }

  onInboxNotification(handler: (notification: any) => void): () => void {
    this.inboxHandlers.add(handler);
    return () => this.inboxHandlers.delete(handler);
  }

  close(): void {
    this.agentProcess.kill('SIGTERM');
  }

  protected broadcastStreamEvent(event: AgentStreamEvent): void {
    this.updateHandlers.forEach((handler) => handler(event));
  }

  protected broadcastMultiSession(
    sessionId: string,
    event: AgentStreamEvent
  ): void {
    this.multiSessionHandlers.forEach((h) => h(sessionId, event));
  }

  protected broadcastSubagentList(
    subagents: any[],
    pendingStages?: any[]
  ): void {
    this.subagentListHandlers.forEach((h) => h(subagents, pendingStages));
  }

  protected broadcastInbox(notification: any): void {
    this.inboxHandlers.forEach((h) => h(notification));
  }

  // ── Shared ext notification handlers ──

  protected extNotificationHandlers: Record<
    string,
    (params: Record<string, unknown>) => void
  > = {
    [EXT_METHODS.COMMANDS_AVAILABLE]: (p) => this.handleCommandsAdvertising(p),
    [EXT_METHODS.METADATA]: (p) => this.handleMetadataUpdate(p),
    [EXT_METHODS.COMPACTION_STATUS]: (p) => this.handleCompactionStatus(p),
    [EXT_METHODS.CLEAR_STATUS]: () => this.handleClearStatus(),
    [EXT_METHODS.MCP_SERVER_INIT_FAILURE]: (p) =>
      this.handleMcpServerInitFailure(p),
    [EXT_METHODS.MCP_OAUTH_REQUEST]: (p) => this.handleMcpOauthRequest(p),
    [EXT_METHODS.MCP_SERVER_INITIALIZED]: (p) =>
      this.handleMcpServerInitialized(p),
    [EXT_METHODS.MCP_GOVERNANCE_DISABLED]: (p) =>
      this.handleMcpGovernanceDisabled(p),
    [EXT_METHODS.AGENT_NOT_FOUND]: (p) => this.handleAgentNotFound(p),
    [EXT_METHODS.AGENT_CONFIG_ERROR]: (p) => this.handleAgentConfigError(p),
    [EXT_METHODS.RATE_LIMIT_ERROR]: (p) => this.handleRateLimitError(p),
    [EXT_METHODS.SUBAGENT_LIST_UPDATE]: (p) => this.handleSubagentListUpdate(p),
    [EXT_METHODS.SESSION_ACTIVITY]: (p) => this.handleSessionActivity(p),
    [EXT_METHODS.SESSION_LIST_UPDATE]: (p) => this.handleSessionListUpdate(p),
    [EXT_METHODS.INBOX_NOTIFICATION]: (p) => this.handleInboxNotification(p),
    [EXT_METHODS.AGENT_SWITCHED]: (p) => this.handleAgentSwitched(p),
    [EXT_METHODS.SESSION_UPDATE]: (p) => this.handleExtSessionUpdate(p),
  };

  private handleCommandsAdvertising(params: Record<string, unknown>) {
    const commands =
      (params.commands as Array<{
        name: string;
        description: string;
        meta?: Record<string, unknown>;
      }>) || [];
    const prompts = (params.prompts as PromptCacheEntry[]) || [];
    const tools =
      (params.tools as Array<{
        name: string;
        description: string;
        source: string;
      }>) || [];
    const mcpServers =
      (params.mcpServers as Array<{
        name: string;
        status: string;
        toolCount: number;
      }>) || [];

    this.broadcastStreamEvent({
      type: AgentEventType.CommandsUpdate,
      commands: commands.map((cmd) => {
        let description = cmd.description;
        if (cmd.name === 'tools' && tools.length > 0) {
          description = `${cmd.description} (${tools.length} available)`;
        } else if (cmd.name === 'mcp' && mcpServers.length > 0) {
          const running = mcpServers.filter(
            (s) => s.status === 'running'
          ).length;
          description = `${cmd.description} (${running}/${mcpServers.length} running)`;
        }
        return { name: cmd.name, description, meta: cmd.meta };
      }),
    });
    this.broadcastStreamEvent({ type: AgentEventType.PromptsUpdate, prompts });
    this.promptsCache = prompts;
  }

  private handleMetadataUpdate(params: Record<string, unknown>) {
    const sessionId = params.sessionId as string | undefined;
    if (sessionId && sessionId !== this.sessionId) return;
    const percent =
      (params.contextUsagePercentage as number | undefined) ?? null;
    if (percent !== null) {
      this.broadcastStreamEvent({ type: AgentEventType.ContextUsage, percent });
    }
    const metering = params.meteringUsage as MeteringUsage[] | undefined;
    const durationMs = params.turnDurationMs as number | undefined;
    if (metering && metering.length > 0) {
      this.broadcastStreamEvent({
        type: AgentEventType.TurnSummary,
        meteringUsage: metering,
        turnDurationMs: durationMs,
      });
    }
    const effort = (params.effort as string | undefined) ?? null;
    this.broadcastStreamEvent({ type: AgentEventType.EffortUpdate, effort });
  }

  private handleClearStatus() {
    logger.debug('Clear status received');
  }

  private handleCompactionStatus(params: Record<string, unknown>) {
    const status = params.status as { type: string; error?: string };
    const summary = params.summary as string | undefined;
    if (status) {
      this.broadcastStreamEvent({
        type: AgentEventType.CompactionStatus,
        status: status.type as 'started' | 'completed' | 'failed',
        error: status.error,
        summary,
      });
    }
  }

  private handleMcpServerInitFailure(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerInitFailure,
      serverName: (params.serverName as string) ?? '',
      error: (params.error as string) ?? '',
    });
  }

  private handleMcpOauthRequest(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpOauthRequest,
      serverName: (params.serverName as string) ?? '',
      oauthUrl: (params.oauthUrl as string) ?? '',
    });
  }

  private handleMcpServerInitialized(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerInitialized,
      serverName: (params.serverName as string) ?? '',
    });
  }

  private handleMcpGovernanceDisabled(params: Record<string, unknown>) {
    const apiFailure = (params.apiFailure as boolean) ?? false;
    logger.warn('MCP governance disabled:', { apiFailure });
    this.broadcastStreamEvent({
      type: AgentEventType.McpGovernanceDisabled,
      apiFailure,
    });
  }

  private handleRateLimitError(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.RateLimitError,
      message: (params.message as string) ?? '',
    });
  }

  private handleAgentNotFound(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.AgentNotFound,
      requestedAgent: (params.requestedAgent as string) ?? '',
      fallbackAgent: (params.fallbackAgent as string) ?? '',
    });
  }

  private handleAgentConfigError(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.AgentConfigError,
      path: params.path as string | undefined,
      error: (params.error as string) ?? '',
    });
  }

  private handleSubagentListUpdate(params: Record<string, unknown>) {
    this.broadcastSubagentList(
      (params as any)?.subagents ?? [],
      (params as any)?.pendingStages ?? []
    );
  }

  private handleSessionActivity(params: Record<string, unknown>) {
    const sessionId = (params as any)?.sessionId as string;
    const event = (params as any)?.event as AgentStreamEvent;
    if (sessionId && event) this.broadcastMultiSession(sessionId, event);
  }

  private handleSessionListUpdate(params: Record<string, unknown>) {
    this.broadcastSubagentList((params as any)?.sessions ?? []);
  }

  private handleInboxNotification(params: Record<string, unknown>) {
    this.broadcastInbox(params);
  }

  private handleAgentSwitched(params: Record<string, unknown>) {
    const p = params as {
      agentName: string;
      previousAgentName?: string;
      welcomeMessage?: string;
      model?: string;
    };
    this.broadcastStreamEvent({
      type: AgentEventType.AgentSwitched,
      agentName: p.agentName,
      previousAgentName: p.previousAgentName,
      welcomeMessage: p.welcomeMessage,
      model: p.model,
    });
  }

  private handleExtSessionUpdate(params: Record<string, unknown>) {
    const update = params.update as Record<string, unknown> | undefined;
    if (!update) return;

    if (update.sessionUpdate === 'tool_call_chunk') {
      const chunk = update as {
        toolCallId: string;
        title: string;
        kind: string;
      };
      const sessionId = params.sessionId as string | undefined;
      const isSubagentEvent = sessionId && sessionId !== this.sessionId;
      const event: AgentStreamEvent = {
        type: AgentEventType.ToolCall,
        id: chunk.toolCallId,
        name: chunk.title,
        kind: chunk.kind,
        args: {},
        sessionId: isSubagentEvent ? sessionId : undefined,
      };
      if (isSubagentEvent) this.broadcastMultiSession(sessionId, event);
      this.broadcastStreamEvent(event);
    } else if (update.sessionUpdate === 'retry_warning') {
      const warning = update as {
        attempt: number;
        maxAttempts: number;
        delaySecs: number;
        message: string;
      };
      logger.warn('Retry warning received:', warning);
      this.broadcastStreamEvent({
        type: AgentEventType.RetryWarning,
        attempt: warning.attempt,
        maxAttempts: warning.maxAttempts,
        delaySecs: warning.delaySecs,
        message: warning.message,
      });
    }
  }

  // ── Shared session update → event conversion ──

  protected convertAcpUpdateToEvent(
    update: AcpSessionUpdate
  ): AgentStreamEvent | null {
    switch (update.sessionUpdate) {
      case 'user_message_chunk':
        return update.content.type === 'text'
          ? {
              type: AgentEventType.UserMessage,
              id: crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
            }
          : null;

      case 'agent_message_chunk':
        switch (update.content.type) {
          case 'text':
            return {
              type: AgentEventType.Content,
              id: crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
            };
          case 'image':
            return {
              type: AgentEventType.Content,
              id: crypto.randomUUID(),
              content: { type: ContentType.Image, image: update.content },
            };
          default:
            return null;
        }

      case 'tool_call': {
        const toolContent = ((update as any).content || [])
          .filter((c: any) => c.type === 'diff')
          .map((c: any) => ({
            type: 'diff' as const,
            path: c.path,
            newText: c.newText || '',
            oldText: c.oldText,
          }));
        const locations = ((update as any).locations || []).map((loc: any) => ({
          path: loc.path,
          line: loc.line,
        }));
        return {
          type: AgentEventType.ToolCall,
          id: update.toolCallId,
          name: update.title || 'unknown',
          kind: (update as any).kind,
          args: (update as any).rawInput || {},
          toolContent: toolContent.length > 0 ? toolContent : undefined,
          locations: locations.length > 0 ? locations : undefined,
        };
      }

      case 'tool_call_update': {
        const toolCallUpdate = update as any;
        if (toolCallUpdate.status === ToolCallStatus.Completed)
          return {
            type: AgentEventType.ToolCallFinished,
            id: toolCallUpdate.toolCallId,
            result: { status: 'success', output: toolCallUpdate.rawOutput },
          };
        if (toolCallUpdate.status === ToolCallStatus.Failed) {
          // If the backend rejected the tool before execution, no `tool_call`
          // notification was sent. Synthesize one from rawInput so the TUI
          // can render the tool name and attempted arguments.
          if (toolCallUpdate.rawInput !== undefined) {
            this.broadcastStreamEvent({
              type: AgentEventType.ToolCall,
              id: toolCallUpdate.toolCallId,
              name: toolCallUpdate.title || 'unknown',
              kind: toolCallUpdate.kind,
              args: toolCallUpdate.rawInput || {},
            });
          }
          // Prefer a descriptive error from the content block; fall back to
          // rawOutput, then a generic message.
          let errorText: string | undefined;
          const failedContent = toolCallUpdate.content;
          if (Array.isArray(failedContent)) {
            const textItem = failedContent.find(
              (item: any) =>
                item.type === 'content' && item.content?.type === 'text'
            );
            if (textItem && typeof textItem.content.text === 'string') {
              errorText = textItem.content.text;
            }
          }
          if (!errorText && typeof toolCallUpdate.rawOutput === 'string') {
            errorText = toolCallUpdate.rawOutput;
          }
          return {
            type: AgentEventType.ToolCallFinished,
            id: toolCallUpdate.toolCallId,
            result: {
              status: 'error',
              error: errorText || 'Tool execution failed',
            },
          };
        }

        // content is a Vec<ToolCallContent> — a tagged enum where the Content
        // variant wraps a ContentBlock: { type: "content", content: { type: "text", text: "..." } }
        const contentArray = toolCallUpdate.content;
        let firstText = '';
        if (Array.isArray(contentArray)) {
          const textItem = contentArray.find(
            (item: any) =>
              item.type === 'content' && item.content?.type === 'text'
          );
          if (textItem) {
            firstText = textItem.content.text ?? '';
          }
        }

        return {
          type: AgentEventType.ToolCallUpdate,
          id: toolCallUpdate.toolCallId,
          content: { type: ContentType.Text, text: firstText },
        };
      }

      case 'available_commands_update': {
        const cu = update as any;
        // Extract prompt-type commands into promptsCache for /prompts selection.
        // KAS sends prompts/skills/steering as commands with _meta.kiro.type.
        const allCommands = (cu.availableCommands || []) as Array<{
          name: string;
          description?: string;
          _meta?: {
            kiro?: { type?: string };
            arguments?: Array<{
              name: string;
              description?: string;
              required?: boolean;
            }>;
          };
        }>;
        const promptCommands = allCommands.filter((cmd) => {
          const type = cmd._meta?.kiro?.type;
          return type != null && PROMPT_COMMAND_TYPES.has(type);
        });
        this.promptsCache = promptCommands.map((cmd) => ({
          name: cmd.name,
          description: cmd.description,
          arguments: (cmd._meta?.arguments || []) as Array<{
            name: string;
            description?: string;
            required?: boolean;
          }>,
          serverName: kasTypeToGroupName(cmd._meta?.kiro?.type),
        }));
        return {
          type: AgentEventType.CommandsUpdate,
          commands: allCommands.map((cmd: any) => ({
            name: cmd.name,
            description: cmd.description,
            meta: cmd._meta,
          })),
        };
      }

      // KAS-specific update types — log and skip for now
      case 'session_info_update':
      case 'config_option_update':
      case 'plan':
      case 'usage_update':
      case 'agent_thought_chunk':
        logger.debug(
          'KAS session update (not yet mapped):',
          update.sessionUpdate
        );
        return null;

      case 'current_mode_update': {
        const modeId = (update as { currentModeId?: string }).currentModeId;
        if (modeId) {
          return {
            type: AgentEventType.AgentSwitched,
            agentName: modeId,
          };
        }
        return null;
      }

      default:
        logger.debug(
          'Unhandled session update type:',
          (update as any).sessionUpdate
        );
        return null;
    }
  }

  // ── Shared permission handling ──

  protected handlePermissionRequest(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    return new Promise<acp.RequestPermissionResponse>((resolve) => {
      const event: AgentStreamEvent = {
        type: AgentEventType.ApprovalRequest,
        value: {
          sessionId: (params as any).sessionId as string | undefined,
          toolCall: { toolCallId: params.toolCall?.toolCallId || '' },
          permissionOptions: (params.options || []).map((opt) => ({
            kind: opt.kind as ApprovalOptionId,
            name: opt.name,
            optionId: opt.optionId,
          })),
          trustOptions: (params._meta as any)?.trustOptions,
          resolve: (userResponse) => {
            resolve(
              userResponse.outcome === 'selected'
                ? {
                    outcome: {
                      outcome: 'selected' as const,
                      optionId: userResponse.optionId,
                      _meta: userResponse._meta,
                    } as acp.RequestPermissionResponse['outcome'],
                  }
                : { outcome: { outcome: 'cancelled' as const } }
            );
          },
        },
      };
      this.broadcastStreamEvent(event);
    });
  }

  // ── Shared session update routing ──

  protected handleSessionUpdate(params: acp.SessionNotification): void {
    const { update } = params;
    if (!update) return;
    const notifSessionId = (params as any).sessionId as string | undefined;
    const isSubagentEvent = notifSessionId && notifSessionId !== this.sessionId;
    const event = this.convertAcpUpdateToEvent(update);
    if (!event) return;

    if (isSubagentEvent) {
      this.broadcastMultiSession(notifSessionId, event);
      const isToolEvent =
        event.type === AgentEventType.ToolCall ||
        event.type === AgentEventType.ToolCallUpdate ||
        event.type === AgentEventType.ToolCallFinished;
      if (isToolEvent) this.broadcastStreamEvent(event);
    } else {
      this.broadcastStreamEvent(event);
    }
  }
}

// ─── Rust ACP client ─────────────────────────────────────────────────

export class RustAcpClient extends BaseAcpClient implements acp.Client {
  private connection: acp.ClientSideConnection;

  constructor(agentPath: string, extraAcpArgs: string[] = []) {
    const proc = spawn(agentPath, ['acp', ...extraAcpArgs], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
    });
    super(toAgentProcess(proc));
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    this.connection = new acp.ClientSideConnection(() => this, finalStream);
  }

  /** SDK >=0.16 no longer prepends '_' to ext methods; the Rust sacp backend expects it. */
  private ext(method: string): string {
    return method.startsWith('_') ? method : `_${method}`;
  }

  async initialize(): Promise<void> {
    const initResult = await this.connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
      clientInfo: { name: 'kiro-tui', version: TUI_VERSION },
    });
    logger.debug(
      '[acp-client] ACP handshake done, protocolVersion:',
      initResult.protocolVersion
    );
  }

  async newSession(): Promise<SessionResult> {
    const r = await this.connection.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });
    this.sessionId = r.sessionId;
    logger.debug('ACP session created', { sessionId: this.sessionId });
    return {
      sessionId: r.sessionId,
      currentModel: extractModel(r.models),
      currentAgent: extractCurrentAgent(r.modes),
    };
  }

  async loadSession(sessionId: string): Promise<SessionResult> {
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;
    const r = await this.connection
      .loadSession({ sessionId, cwd: process.cwd(), mcpServers: [] })
      .catch((err) => {
        this.sessionId = previousSessionId;
        throw err;
      });
    logger.debug('[acp-client] loadSession completed for session:', sessionId);
    return {
      sessionId,
      currentModel: extractModel(r.models),
      currentAgent: extractCurrentAgent(r.modes),
    };
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId)
      throw new Error('cannot send prompt without an active session');
    if (this.connection.signal.aborted)
      throw new Error('Agent connection closed unexpectedly');

    const connectionClosed = new Promise<never>((_resolve, reject) => {
      if (this.connection.signal.aborted) {
        reject(new Error('Agent connection closed unexpectedly'));
        return;
      }
      this.connection.signal.addEventListener(
        'abort',
        () => reject(new Error('Agent connection closed unexpectedly')),
        { once: true }
      );
    });
    connectionClosed.catch(() => {});

    await Promise.race([
      this.connection.prompt({ prompt: messages, sessionId: this.sessionId }),
      connectionClosed,
    ]);
  }

  async cancel(): Promise<void> {
    if (!this.sessionId) return;
    try {
      await this.connection.cancel({ sessionId: this.sessionId });
    } catch (e) {
      logger.error('Failed to send cancel notification:', e);
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      return (await this.connection.extMethod(
        this.ext(EXT_METHODS.COMMANDS_EXECUTE),
        {
          sessionId: this.sessionId,
          command,
        }
      )) as unknown as CommandResult;
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Command failed',
      };
    }
  }

  async getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionId) return { options: [] };
    try {
      return (await this.connection.extMethod(
        this.ext(EXT_METHODS.COMMANDS_OPTIONS),
        {
          sessionId: this.sessionId,
          command: commandName.replace(/^\//, ''),
          partial,
        }
      )) as unknown as CommandOptionsResponse;
    } catch {
      return { options: [] };
    }
  }

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionId) return;
    await this.connection.setSessionMode({ sessionId: this.sessionId, modeId });
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    return (await this.connection.extMethod(this.ext('kiro.dev/session/list'), {
      cwd,
    })) as unknown as ListSessionsResponse;
  }

  async listSettings(): Promise<Record<string, unknown>> {
    return (await this.connection.extMethod(
      this.ext('kiro.dev/settings/list'),
      {}
    )) as unknown as Record<string, unknown>;
  }

  async setSetting(key: string, value: unknown): Promise<void> {
    await this.connection.extMethod(this.ext('kiro.dev/settings/set'), {
      key,
      value,
    });
  }

  async terminateSession(sessionId: string): Promise<void> {
    try {
      await this.connection.extMethod(this.ext('kiro.dev/session/terminate'), {
        sessionId,
      });
    } catch (err) {
      logger.warn('terminateSession failed (best-effort)', { sessionId, err });
    }
  }

  async spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    const result = await this.connection.extMethod(
      this.ext(EXT_METHODS.SESSION_SPAWN),
      {
        sessionId: this.sessionId,
        task,
        name,
      }
    );
    return {
      sessionId: (result as any).sessionId,
      name: (result as any).name ?? name ?? '',
    };
  }

  async sendMessage(sessionId: string, content: string): Promise<void> {
    await this.connection.extMethod(this.ext(EXT_METHODS.MESSAGE_SEND), {
      sessionId,
      content,
    });
  }

  // ── acp.Client interface ──

  async requestPermission(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    return this.handlePermissionRequest(params);
  }

  async sessionUpdate(params: acp.SessionNotification): Promise<void> {
    this.handleSessionUpdate(params);
  }

  async writeTextFile?(
    _p: acp.WriteTextFileRequest
  ): Promise<acp.WriteTextFileResponse> {
    throw new Error('not implemented');
  }
  async readTextFile?(
    _p: acp.ReadTextFileRequest
  ): Promise<acp.ReadTextFileResponse> {
    throw new Error('not implemented');
  }
  async createTerminal?(
    _p: acp.CreateTerminalRequest
  ): Promise<acp.CreateTerminalResponse> {
    throw new Error('not implemented');
  }
  async terminalOutput?(
    _p: acp.TerminalOutputRequest
  ): Promise<acp.TerminalOutputResponse> {
    throw new Error('not implemented');
  }
  async releaseTerminal?(
    _p: acp.ReleaseTerminalRequest
  ): Promise<acp.ReleaseTerminalResponse | void> {
    throw new Error('not implemented');
  }
  async waitForTerminalExit?(
    _p: acp.WaitForTerminalExitRequest
  ): Promise<acp.WaitForTerminalExitResponse> {
    throw new Error('not implemented');
  }
  async killTerminal?(
    _p: acp.KillTerminalRequest
  ): Promise<acp.KillTerminalResponse | void> {
    throw new Error('not implemented');
  }
  async extMethod?(
    _method: string,
    _params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    throw new Error('not implemented');
  }

  async extNotification?(
    method: string,
    params: Record<string, unknown>
  ): Promise<void> {
    // ACP SDK >=0.16 passes raw method names; older versions strip the leading '_'.
    const key = method.startsWith('_') ? method.substring(1) : method;
    const handler = this.extNotificationHandlers[key];
    if (handler) handler(params);
  }
}

// ─── KAS ACP client ──────────────────────────────────────────────────

export class KasAcpClient extends BaseAcpClient {
  private kiroClient: KiroClient;

  /**
   * Construct a KAS ACP client.
   *
   * Default (no options): spawn the KAS subprocess and wire its stdio as
   * the ACP `Stream`. This is the production path.
   *
   * With `options.stream`: skip the subprocess spawn and use the provided
   * stream (for `acp_integ_tests/`'s mock transport). The client reports
   * `kill()`/`onExit()` through a no-op null agent process internally so
   * `BaseAcpClient` has a uniform interface to work against.
   *
   * `agentProcess` is intentionally not a public option - mock callers
   * never need to inject a different one, and accepting it without a
   * stream would silently ignore it.
   */
  constructor(options?: { stream: Stream }) {
    if (options) {
      super(createNullAgentProcess());
      const finalStream = maybeWrapStreamWithRecorder(options.stream);
      this.kiroClient = new KiroClient({
        stream: finalStream,
        clientInfo: { name: 'kiro-cli', version: TUI_VERSION },
      });
      return;
    }

    // Resolve KAS server: env var override > installed npm package
    let kasServerPath = process.env.KIRO_KAS_SERVER_PATH;
    const kasTokenPath = process.env.KIRO_KAS_TOKEN_PATH;
    if (!kasServerPath) {
      // Walk up from this file to find node_modules/@kiro/agent
      const { existsSync } = require('node:fs');
      const { join, dirname } = require('node:path');
      const serverFile = 'node_modules/@kiro/agent/dist/server/acp-server.js';
      let dir = __dirname;
      for (let i = 0; i < 10; i++) {
        const candidate = join(dir, serverFile);
        if (existsSync(candidate)) {
          kasServerPath = candidate;
          break;
        }
        const parent = dirname(dir);
        if (parent === dir) break;
        dir = parent;
      }
      if (!kasServerPath) {
        throw new Error(
          'KAS agent not found. Install @kiro/agent or set KIRO_KAS_SERVER_PATH.'
        );
      }
    }
    const nodeBin = process.env.KIRO_AGENT_PATH || 'node';
    logger.info(`[acp-client] Spawning KAS agent: ${nodeBin} ${kasServerPath}`);

    const proc = spawn(
      nodeBin,
      [
        '--experimental-wasm-modules',
        kasServerPath,
        '--transport=stdio',
        ...(kasTokenPath ? [`--token-path=${kasTokenPath}`] : []),
      ],
      {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: {
          ...process.env,
          NODE_CHANNEL_FD: undefined,
          NODE_CHANNEL_SERIALIZATION_MODE: undefined,
        },
      }
    );
    super(toAgentProcess(proc));
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    const kasSettings = buildKasSettings();
    this.kiroClient = new KiroClient({
      stream: finalStream,
      clientInfo: { name: 'kiro-cli', version: TUI_VERSION },
      clientMeta: {
        telemetryEnabled: isTelemetryEnabled(),
        telemetry: getTelemetryIdentity(),
        knowledge: true,
        ...(kasSettings && { settings: kasSettings }),
      },
    });
  }

  // NOTE: When subagent support is added for KAS, this method will need to
  // be reworked. Currently it assumes only one session's listeners exist at a
  // time (disposing all previous listeners on each call). For subagents we'd
  // need to key disposables by session ID and route events through
  // handleSessionUpdate() for proper main-vs-subagent discrimination.
  private sessionDisposables: Array<{ dispose: () => void }> = [];

  /** Cache of session modes received on session/new and session/load.  This
   *  is the source of truth for the /agent selection menu and stays in sync
   *  via current_mode_update session notifications. */
  private modesState: CachedModesState = {
    availableModes: [],
    currentModeId: undefined,
  };

  /** Capture availableModes / currentModeId from a session/new or
   *  session/load response.  Responses lacking a `modes` field leave the
   *  cache untouched so we don't accidentally blank out a known-good
   *  snapshot. */
  private captureModes(
    response: { modes?: CachedModesState | null | undefined } | undefined
  ): void {
    const modes = response?.modes;
    if (!modes) return;
    this.modesState = {
      availableModes: modes.availableModes ?? [],
      currentModeId: modes.currentModeId,
    };
  }

  /**
   * Cached model options from the most recent session/new, session/load,
   * or session/set_config_option response. Populated from the `model`
   * category entry in the ACP Session Config Options list.
   *
   * Used by:
   *   - getCommandOptions('/model') — powers the selection menu
   *   - executeCommand('model') — validates the switch + resolves display name
   *
   * Empty when the KAS agent has no ModelConfigProvider registered.
   */
  private modelOptions: ModelOption[] = [];
  /** ID of the currently selected model, or undefined if no model config. */
  private currentModelId?: string;

  private wireSessionListeners(sessionId: string): void {
    this.sessionDisposables.forEach((d) => d.dispose());
    this.sessionDisposables = [
      this.kiroClient.onSessionUpdate(sessionId, async (notification) => {
        const update = notification.update;
        // Keep the cached current mode in sync for /agent and agent-display
        // purposes.  ACP doesn't (yet) ship an available_modes_update, so
        // availableModes is refreshed only on session/new and session/load.
        if (update.sessionUpdate === 'current_mode_update') {
          this.modesState = {
            ...this.modesState,
            currentModeId: (update as { currentModeId: string }).currentModeId,
          };
        }
        // Intercept config_option_update (KAS-specific) to keep the
        // local model cache fresh without a round-trip. The agent may
        // push these notifications when it autonomously changes a
        // config option (e.g. fallback to a different model after
        // rate limits) or mirrors a client-initiated change.
        //
        // We only refresh the cache here — propagation of the new
        // current model to the app store happens synchronously
        // through the executeCommand result flow for user-initiated
        // switches (see effect handler `updateModel`). Autonomous
        // agent-side changes will be reflected in the /model menu
        // the next time the user opens it; we intentionally skip
        // UI propagation from this path to avoid re-using the
        // AgentSwitched event channel, which would clobber
        // currentAgent on the store.
        if (
          (update as { sessionUpdate?: string }).sessionUpdate ===
          'config_option_update'
        ) {
          this.refreshModelCache(
            (update as { configOptions?: unknown }).configOptions
          );
        }
        const event = this.convertAcpUpdateToEvent(update);
        if (event) this.broadcastStreamEvent(event);
      }),
      this.kiroClient.onPermissionRequest(sessionId, async (request) => {
        return this.handlePermissionRequest(request);
      }),
    ];
  }

  /**
   * Returns a rejection-promise that fires if the KAS agent process exits,
   * together with an `unsubscribe` to detach the listener. Callers MUST
   * invoke `unsubscribe()` in a `finally` so listeners never accumulate
   * across repeated prompts (and so mock-mode prompts are a strict no-op
   * on the `onExit` path, since the null agent process never emits).
   */
  private processExitPromise(): {
    promise: Promise<never>;
    unsubscribe: () => void;
  } {
    let unsubscribe = () => {};
    const promise = new Promise<never>((_resolve, reject) => {
      unsubscribe = this.agentProcess.onExit((code) => {
        reject(
          new Error(`KAS agent process exited unexpectedly (code ${code})`)
        );
      });
    });
    return { promise, unsubscribe };
  }

  async initialize(): Promise<void> {
    await this.kiroClient.initialize();

    const commands = SLASH_COMMANDS.map((cmd) => ({
      name: cmd.name,
      description: cmd.description,
      meta: (cmd.meta ?? {}) as Record<string, unknown>,
    }));

    this.broadcastStreamEvent({
      type: AgentEventType.ExtensionMethodsDiscovered,
      commands,
    });

    logger.debug('[acp-client] KAS ACP handshake done');
  }

  async newSession(): Promise<SessionResult> {
    const r = await this.kiroClient.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });
    const sid = r.sessionId;
    this.sessionId = sid;
    logger.debug('KAS session created', { sessionId: sid });

    // Snapshot modes before any further async work so /agent has data even
    // if setSessionConfigOption below fails.
    this.captureModes(r as { modes?: CachedModesState | null | undefined });

    // Register BEFORE any async work to avoid race condition
    this.wireSessionListeners(sid);

    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: sid,
        configId: 'autopilot',
        value: 'on',
      });
    } catch (e) {
      logger.debug('Failed to set autopilot config:', e);
    }

    const mode = process.env.KIRO_MODE;
    if (mode) {
      try {
        await this.kiroClient.setSessionConfigOption({
          sessionId: sid,
          configId: 'mode',
          value: mode,
        });
        this.modesState = { ...this.modesState, currentModeId: mode };
      } catch (e) {
        logger.debug('Failed to set mode:', e);
      }
    }

    this.refreshModelCache((r as { configOptions?: unknown }).configOptions);

    return {
      sessionId: sid,
      currentModel:
        extractModelFromConfigOptions(
          (r as { configOptions?: unknown }).configOptions
        ) ?? extractModel(r.models),
      // TODO: Remove cast once @kiro/client adds `modes` to NewSessionResponse
      currentAgent: extractCurrentAgent(this.modesState),
    };
  }

  async loadSession(sessionId: string): Promise<SessionResult> {
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;

    // Register BEFORE loadSession to capture history replay events
    this.wireSessionListeners(sessionId);

    const r = await this.kiroClient
      .loadSession({ sessionId, cwd: process.cwd(), mcpServers: [] })
      .catch((err) => {
        this.sessionId = previousSessionId;
        throw err;
      });
    logger.debug(
      '[acp-client] KAS loadSession completed for session:',
      sessionId
    );

    this.captureModes(r as { modes?: CachedModesState | null | undefined });
    this.refreshModelCache((r as { configOptions?: unknown }).configOptions);

    return {
      sessionId,
      currentModel:
        extractModelFromConfigOptions(
          (r as { configOptions?: unknown }).configOptions
        ) ?? extractModel(r.models),
      // TODO: Remove cast once @kiro/client adds `modes` to LoadSessionResponse
      currentAgent: extractCurrentAgent(
        (r as { modes?: Parameters<typeof extractCurrentAgent>[0] }).modes
      ),
    };
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId)
      throw new Error('cannot send prompt without an active session');

    // Race prompt against process exit to detect KAS crashes
    const { promise: crashed, unsubscribe } = this.processExitPromise();
    // Defensive: the exit listener is detached in finally, but there is a
    // narrow window between Promise.race settling and the finally running
    // where a late rejection could surface as an unhandled rejection. The
    // unsubscribe() call in finally removes the listener; this catch
    // covers the race between scheduling finally and the listener firing.
    crashed.catch(() => {});
    try {
      await Promise.race([
        this.kiroClient.prompt({ prompt: messages, sessionId: this.sessionId }),
        crashed,
      ]);
    } finally {
      unsubscribe();
    }
  }

  async cancel(): Promise<void> {
    if (!this.sessionId) return;
    try {
      this.kiroClient.cancel(this.sessionId);
    } catch (e) {
      logger.error('Failed to send cancel notification:', e);
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    const name = command.command;
    switch (name) {
      case 'quit':
        return { success: true, message: 'Quitting' };
      case 'feedback':
        return kasFeedback(
          (command as Record<string, unknown>).args as
            | Record<string, string>
            | undefined
        );
      case 'help':
        return this.executeHelp();
      case 'clear':
        return this.executeClear();
      case 'plan':
        return this.executePlan();
      case 'paste':
        return executePaste();
      case 'agent': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const parsed = parseAgentSubcommand(args);
        switch (parsed.kind) {
          case 'list':
            return this.executeAgentList();
          case 'swap':
            if (!parsed.name) {
              return { success: false, message: 'Usage: /agent swap <name>' };
            }
            return this.executeAgentSwap(parsed.name);
          case 'create':
            // TODO: route through a KAS extension method so config writes
            // stay the source of truth on the agent side.  Until then,
            // surface a clear not-yet-implemented error.
            return {
              success: false,
              message: '/agent create is not yet implemented in KAS mode',
            };
          case 'edit':
            // TODO: route through a KAS extension method (see create).
            return {
              success: false,
              message: '/agent edit is not yet implemented in KAS mode',
            };
          default: {
            // Exhaustiveness check — also satisfies eslint no-fallthrough.
            const _exhaustive: never = parsed;
            throw new Error(
              `Unhandled /agent subcommand: ${JSON.stringify(_exhaustive)}`
            );
          }
        }
      }
      case 'chat': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const value = args?.value ?? '';
        if (/^delete\b/.test(value)) {
          const sessionId = value.slice(7).trim();
          if (!sessionId)
            return {
              success: false,
              message: 'Usage: /chat delete <sessionId>',
            };
          return this.callExtMethod('_kiro/session/delete', { sessionId });
        }
        return {
          success: false,
          message: `/chat ${value || 'save/load'} is not yet supported in KAS mode`,
        };
      }
      case 'model': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const modelId = args?.value ?? '';
        if (!modelId) {
          // Bare `/model` invocation. The dispatcher normally fetches
          // options via getCommandOptions for selection-type commands,
          // so this branch fires only when the user typed `/model`
          // directly as a command with no arg (unlikely) — surface a
          // helpful hint rather than a generic failure.
          return {
            success: false,
            message:
              this.modelOptions.length === 0
                ? 'No models available'
                : 'Usage: /model <model-id>',
          };
        }
        return this.executeModelSwap(modelId);
      }
      case 'reply':
        return { success: true, message: '' };
      case 'usage': {
        const result = await this.callExtMethod('_kiro/usage/get');
        if (!result.success) return result;
        const response = result.data as
          | { success: boolean; message: string; data?: unknown }
          | undefined;
        return {
          success: response?.success ?? true,
          message: response?.message ?? '',
          data: response?.data,
        };
      }
      case 'prompts': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const promptName = args?.value ?? '';
        if (!promptName) {
          return {
            success: true,
            message: 'Use the selection menu to pick a prompt.',
          };
        }
        return {
          success: true,
          message: '',
          data: { executePrompt: `/${promptName}` },
        };
      }
      case 'knowledge': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const value = args?.value ?? 'show';
        return this.executeKnowledge(value);
      }
      default:
        return {
          success: false,
          message: `/${name} is not yet supported in KAS mode`,
        };
    }
  }

  /** /help — effect expects data.commands with { name, description, usage } */
  private async executeHelp(): Promise<CommandResult> {
    const result = await this.callExtMethod('_kiro/help');
    if (!result.success) return result;
    const data = result.data as {
      commands?: Array<{ name: string; description: string }>;
    };
    return {
      success: true,
      message: 'Available commands',
      data: {
        commands: (data?.commands ?? []).map((c) => ({
          name: c.name,
          description: c.description,
          usage: c.name,
        })),
      },
    };
  }

  /** /agent (no args) — fallback reached only when getCommandOptions returns
   *  no options.  We derive from cached ACP modes (see getCommandOptions for
   *  the primary code path). */
  private async executeAgentList(): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    const modes = this.modesState.availableModes;
    const current = this.modesState.currentModeId ?? '';
    const agents = modes.map((m) => ({
      name: m.id,
      description: m.description ?? '',
    }));
    return {
      success: true,
      message: `${agents.length} agents available`,
      data: { agents, current },
    };
  }

  /** /agent swap — uses session/setMode (standard ACP) since KAS doesn't have _kiro/agent/swap */
  private async executeAgentSwap(agentName: string): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'mode',
        value: agentName,
      });
      return {
        success: true,
        message: `Switched to ${agentName}`,
        data: { agent: { name: agentName } },
      };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Failed to switch agent',
      };
    }
  }

  /**
   * /model switch — uses the ACP-standard `session/set_config_option`
   * with `configId: 'model'`. KAS returns the full configOptions state
   * in the response, which we use to refresh the local model cache and
   * resolve the new model's display name.
   *
   * Session state (history, tools, MCP servers, agent profile) is
   * preserved across model switches — KAS simply updates the
   * `modelId` on its in-memory session and picks it up on the next
   * prompt turn.
   */
  private async executeModelSwap(modelId: string): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      const response = await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'model',
        value: modelId,
      });
      const configOptions = (response as { configOptions?: unknown })
        .configOptions;
      this.refreshModelCache(configOptions);
      const model = extractModelFromConfigOptions(configOptions);
      // Validate the switch landed on the requested id. If KAS rejected
      // the value but still returned a configOptions state, surface a
      // clear error rather than silently reporting success.
      if (!model || model.id !== modelId) {
        return {
          success: false,
          message: `Model '${modelId}' not available`,
        };
      }
      return {
        success: true,
        message: `Switched to ${model.name}`,
        data: { model: { id: model.id, name: model.name } },
      };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Failed to switch model',
      };
    }
  }

  private async executeKnowledge(value: string): Promise<CommandResult> {
    const parts = value.trim().split(/\s+/);
    const subcommand = parts[0] || 'show';

    const params: Record<string, unknown> = { subcommand };

    if (subcommand === 'add' && parts.length >= 3) {
      params.name = parts[1];
      params.path = parts.slice(2).join(' ');
    } else if (subcommand === 'remove' && parts.length >= 2) {
      params.target = parts.slice(1).join(' ');
    } else if (subcommand === 'update' && parts.length >= 2) {
      params.path = parts.slice(1).join(' ');
    } else if (subcommand === 'cancel' && parts.length >= 2) {
      params.operationId = parts[1];
    }

    const result = await this.callExtMethod('_kiro/knowledge', params);
    if (!result.success) return result;

    const data = result.data as { entries?: unknown[]; message?: string } | undefined;
    return {
      success: true,
      message: data?.message ?? '',
      data: { entries: data?.entries ?? [] },
    };
  }

  /**
   * Refresh the cached model options + current model id from a KAS
   * configOptions array (returned by session/new, session/load, and
   * session/set_config_option).
   *
   * If the array contains no `category: 'model'` entry (e.g. KAS has
   * no ModelConfigProvider registered), the cache is cleared so that
   * `/model` surfaces "No options available" rather than stale data.
   */
  private refreshModelCache(configOptions: unknown): void {
    const modelOpt = findModelConfigOption(configOptions);
    if (!modelOpt) {
      // Visibility into the "why is /model empty?" case — logs the
      // ids/categories present so developers can see that KAS returned
      // e.g. only [mode, autopilot, contentCollection] with no model
      // entry (typical for a standalone KAS without a
      // ModelConfigProvider registered by the host IDE).
      const present = Array.isArray(configOptions)
        ? (configOptions as Array<Record<string, unknown>>).map((o) => ({
            id: o.id,
            category: o.category,
          }))
        : configOptions;
      logger.debug(
        '[kas] refreshModelCache: no `category: "model"` entry. configOptions:',
        present
      );
      this.modelOptions = [];
      this.currentModelId = undefined;
      return;
    }
    logger.debug(
      '[kas] refreshModelCache: cached',
      modelOpt.options.length,
      'models, current:',
      modelOpt.currentValue
    );
    this.modelOptions = modelOpt.options;
    this.currentModelId = modelOpt.currentValue;
  }

  /**
   * /clear — compose from ACP primitives: create a fresh session
   * (session/new) and let the TUI reset its screen.  KAS intentionally
   * does not expose a higher-level "clear conversation" extension method,
   * so the client composes this behavior itself.
   */
  private async executeClear(): Promise<CommandResult> {
    try {
      const session = await this.newSession();
      return {
        success: true,
        message: 'Conversation cleared',
        data: {
          sessionId: session.sessionId,
          currentModel: session.currentModel,
          currentAgent: session.currentAgent,
        },
      };
    } catch (e) {
      return {
        success: false,
        message:
          e instanceof Error ? e.message : 'Failed to clear conversation',
      };
    }
  }

  /** /plan — effect expects data.agent.name (uses updateAgent) */
  private async executePlan(): Promise<CommandResult> {
    const result = await this.callExtMethod('_kiro/plan');
    if (!result.success) return result;
    return {
      success: true,
      message: 'Switched to spec',
      data: { agent: { name: 'spec' } },
    };
  }

  private async callExtMethod(
    method: string,
    extra?: Record<string, unknown>
  ): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      const result = await this.kiroClient.sendExtMethod(method, {
        sessionId: this.sessionId,
        ...extra,
      });
      return { success: true, message: '', data: result };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Command failed',
      };
    }
  }

  async getCommandOptions(
    commandName: string,
    _partial: string
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionId) return { options: [] };
    const name = commandName.replace(/^\//, '');
    switch (name) {
      case 'feedback':
        return KAS_FEEDBACK_OPTIONS;
      case 'agent': {
        // Derive options from cached session modes (ACP primitive) rather
        // than a custom ext method.  Modes are grouped by `_meta.kiro.source`
        // (e.g. "bundled", "user", "workspace") so the menu reflects where
        // each agent came from.  See the review discussion at
        // https://github.com/kiro-team/kiro-agent/pull/568#discussion_r3192594213
        const { availableModes, currentModeId } = this.modesState;
        return {
          options: availableModes.map((m) => {
            const source = getModeSource(m._meta);
            const isActive = m.id === currentModeId;
            const descBase = m.description ?? '';
            return {
              value: m.id,
              label: m.name || m.id,
              description: isActive
                ? `[active]${descBase ? ` ${descBase}` : ''}`
                : descBase,
              ...(source ? { group: capitalize(source) } : {}),
            };
          }),
        };
      }
      case 'model': {
        // Served from the local cache populated by session/new,
        // session/load, and session/set_config_option responses.
        // KAS returns the full configOptions state on every
        // mutation, so the cache stays in sync without extra
        // round-trips.
        if (this.modelOptions.length === 0) return { options: [] };
        return {
          options: this.modelOptions.map((m) => {
            const isActive = m.value === this.currentModelId;
            const desc = m.description ?? '';
            return {
              value: m.value,
              label: m.name,
              description: isActive
                ? desc
                  ? `[active] ${desc}`
                  : '[active]'
                : desc,
            };
          }),
        };
      }
      case 'prompts': {
        return {
          options: this.promptsCache
            .map((p) => {
              const hint =
                p.arguments
                  .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
                  .join(' ') || undefined;
              return {
                value: p.name,
                label: `/${p.name}`,
                description: p.description ?? '',
                group: p.serverName,
                hint,
              };
            })
            .sort(
              (a, b) =>
                (a.group ?? '').localeCompare(b.group ?? '') ||
                a.label.toLowerCase().localeCompare(b.label.toLowerCase())
            ),
        };
      }
      default:
        return { options: [] };
    }
  }

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionId) return;
    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'mode',
        value: modeId,
      });
    } catch (e) {
      logger.debug('Failed to set mode:', e);
    }
  }

  async listSessions(_cwd: string): Promise<ListSessionsResponse> {
    try {
      const r = await this.kiroClient.listSessions();
      logger.debug('[kas] listSessions raw:', JSON.stringify(r));
      return {
        sessions: r.sessions.map((s: any) => ({
          sessionId: s.sessionId,
          cwd: s.cwd,
          title: s.title,
          updatedAt: s.updatedAt ?? s._meta?.createdAt,
        })),
      };
    } catch (e) {
      logger.debug('[kas] listSessions failed:', e);
      return { sessions: [] };
    }
  }

  async listSettings(): Promise<Record<string, unknown>> {
    return {};
  }

  async setSetting(_key: string, _value: unknown): Promise<void> {}

  async terminateSession(_sessionId: string): Promise<void> {}

  async spawnSession(
    _task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    logger.debug('spawnSession not yet supported in KAS mode');
    return { sessionId: '', name: name ?? '' };
  }

  async sendMessage(sessionId: string, content: string): Promise<void> {
    await this.kiroClient.prompt({
      prompt: [{ type: 'text', text: content }],
      sessionId,
    });
  }
}

/** Extract the Kiro agent source (bundled / user / workspace) from a
 *  SessionMode's `_meta` field.  Returns undefined when the agent hasn't
 *  attached any source metadata, which is fine — those modes fall into the
 *  default (ungrouped) bucket in the /agent menu. */
function getModeSource(
  meta?: Record<string, unknown> | null
): string | undefined {
  const kiroMeta = meta?.kiro as Record<string, unknown> | undefined;
  const source = kiroMeta?.source;
  return typeof source === 'string' ? source : undefined;
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0]!.toUpperCase() + s.slice(1) : s;
}

/** Tagged union describing what `/agent …` should do.  Produced by
 *  parseAgentSubcommand and consumed by KasAcpClient.executeCommand. */
export type ParsedAgentCommand =
  | { kind: 'list' }
  | { kind: 'swap'; name: string }
  | { kind: 'create'; name: string | undefined }
  | { kind: 'edit'; name: string | undefined };

/** Parse the `args` payload passed to `executeCommand({ command: 'agent' })`
 *  into a structured subcommand.
 *
 *  Grammar:
 *    ""                           → { kind: 'list' }
 *    "swap <name>"                → { kind: 'swap',   name: '<name>' }
 *    "create" | "create <name>"   → { kind: 'create', name: '<name>' | undefined }
 *    "edit"   | "edit <name>"     → { kind: 'edit',   name: '<name>' | undefined }
 *    "<name>"                     → { kind: 'swap',   name: '<name>' }  (menu shorthand)
 *
 *  The raw text comes from either `args.agentName` (direct test harness
 *  usage) or `args.value` (the dispatcher's generic selection-UI shape).
 *
 *  Exported for unit testing; not part of the public client API. */
export function parseAgentSubcommand(
  args: Record<string, string> | undefined
): ParsedAgentCommand {
  const raw = (args?.agentName ?? args?.value ?? '').trim();
  if (!raw) return { kind: 'list' };

  const spaceIdx = raw.indexOf(' ');
  const verb = (spaceIdx === -1 ? raw : raw.slice(0, spaceIdx)).toLowerCase();
  const rest = spaceIdx === -1 ? '' : raw.slice(spaceIdx + 1).trim();

  switch (verb) {
    case 'create':
      return { kind: 'create', name: rest || undefined };
    case 'edit':
      return { kind: 'edit', name: rest || undefined };
    case 'swap':
      // Bare "swap" with no name is a user error; surfaced by executeCommand.
      return { kind: 'swap', name: rest };
    default:
      // Menu shorthand: a bare value with no verb swaps to that agent.
      return { kind: 'swap', name: raw };
  }
}

/** Static feedback options for /feedback selection menu. */
const KAS_FEEDBACK_OPTIONS: CommandOptionsResponse = {
  options: [
    {
      value: 'general',
      label: 'General feedback',
      description: 'Share general thoughts or suggestions',
    },
    {
      value: 'feature',
      label: 'Feature request',
      description: 'Request a new feature or improvement',
    },
    {
      value: 'issue',
      label: 'Report an issue',
      description: 'Report a bug or problem',
    },
  ],
};

const FEEDBACK_URLS: Record<string, string> = {
  general: 'https://github.com/kirodotdev/Kiro/issues/new/choose',
  feature:
    'https://github.com/kirodotdev/Kiro/issues/new?template=feature_request.yml',
  issue: 'https://github.com/kirodotdev/Kiro/issues',
};

function kasFeedback(args?: Record<string, string>): CommandResult {
  const kind = args?.value || 'general';
  const url = FEEDBACK_URLS[kind] ?? FEEDBACK_URLS.general!;
  try {
    const { execSync } = require('child_process');
    const cmd =
      process.platform === 'darwin'
        ? 'open'
        : process.platform === 'win32'
          ? 'start'
          : 'xdg-open';
    execSync(`${cmd} '${url}'`, { stdio: 'ignore' });
    return { success: true, message: 'Opening in browser...' };
  } catch {
    return {
      success: false,
      message: `Could not open browser. Copy the URL: ${url}`,
      data: { url },
    };
  }
}

/**
 * /paste — read an image from the system clipboard and return it in the
 * shape expected by the `pasteImage` effect handler (base64 PNG + dims).
 *
 * The Rust backend handles this server-side via the `arboard` crate; in
 * KAS mode the agent is a plain TypeScript ACP server with no clipboard
 * access, so the TUI composes the command itself. The returned image is
 * forwarded as a ContentBlock on the next prompt by the effect handler.
 */
export function executePaste(): CommandResult {
  const result = readClipboardImage();
  if (!result.ok) {
    return { success: false, message: result.error.message };
  }
  return {
    success: true,
    message: 'Image pasted from clipboard',
    data: {
      data: result.image.data,
      mimeType: result.image.mimeType,
      width: result.image.width,
      height: result.image.height,
      sizeBytes: result.image.sizeBytes,
    },
  };
}

// ─── Factory ─────────────────────────────────────────────────────────

export function createAcpClient(
  agentPath: string,
  extraAcpArgs: string[] = []
): SessionClient {
  if (process.env.KIRO_AGENT_ENGINE === 'kas') {
    // Test-only: inject an in-process mock transport when the harness set
    // the socket path env var. Production boots skip this branch entirely.
    // The require() path stays in this one call site so the rest of
    // `KasAcpClient` never references test-utils.
    const mockSocketPath = process.env.KIRO_ACP_MOCK_SOCKET;
    if (mockSocketPath) {
      logger.info(
        `[acp-client] KAS mock transport (test-only), socket=${mockSocketPath}`
      );
      const {
        connectMockTransport,
      } = require('./test-utils/acp-mock/MockAcpTransport');
      const stream = connectMockTransport(mockSocketPath);
      return new KasAcpClient({ stream });
    }
    return new KasAcpClient();
  }
  return new RustAcpClient(agentPath, extraAcpArgs);
}

/** @deprecated Use createAcpClient() instead */
export const AcpClient = RustAcpClient;
