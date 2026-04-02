import * as acp from '@agentclientprotocol/sdk';
import { logger } from './utils/logger';
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

const TUI_VERSION: string = packageJson.version;

export type AcpSessionUpdate = acp.SessionNotification['update'];

/** Custom extension method names (without leading underscore - SDK strips it) */
const EXT_METHODS = {
  COMMANDS_AVAILABLE: 'kiro.dev/commands/available',
  COMMANDS_EXECUTE: 'kiro.dev/commands/execute',
  COMMANDS_OPTIONS: 'kiro.dev/commands/options',
  METADATA: 'kiro.dev/metadata',
  COMPACTION_STATUS: 'kiro.dev/compaction/status',
  CLEAR_STATUS: 'kiro.dev/clear/status',
  MCP_SERVER_INIT_FAILURE: 'kiro.dev/mcp/server_init_failure',
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

/**
 * ACP client implementation that converts ACP protocol to app domain types
 */
function extractCurrentAgent(
  modes?: {
    currentModeId?: string;
    availableModes?: Array<{ id: string; _meta?: Record<string, unknown> }>;
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

export class AcpClient implements acp.Client, SessionClient {
  private connection: acp.ClientSideConnection;
  public sessionId?: string;
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private multiSessionHandlers: Set<
    (sessionId: string, event: AgentStreamEvent) => void
  > = new Set();
  private inboxHandlers: Set<(notification: any) => void> = new Set();
  private sessionEventHandlers: Set<(event: any) => void> = new Set();
  private subagentListHandlers: Set<
    (subagents: any[], pendingStages?: any[]) => void
  > = new Set();
  private agentProcess: ChildProcess;

  constructor(agentPath: string, extraAcpArgs: string[] = []) {
    this.agentProcess = spawn(agentPath, ['acp', ...extraAcpArgs], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
    });

    if (!this.agentProcess.stdout || !this.agentProcess.stdin) {
      throw new Error('Failed to create agent process stdio streams');
    }

    // Route agent stderr to the TUI logger instead of inheriting it
    // to the terminal (prevents debug output like "[DEBUG] Invoked with model"
    // from bleeding into the TUI).
    if (this.agentProcess.stderr) {
      let stderrBuf = '';
      this.agentProcess.stderr.on('data', (chunk: Buffer) => {
        stderrBuf += chunk.toString();
        const lines = stderrBuf.split('\n');
        stderrBuf = lines.pop() || '';
        for (const line of lines) {
          if (line.trim()) {
            logger.debug('[agent-stderr]', line);
          }
        }
      });
      this.agentProcess.stderr.on('end', () => {
        if (stderrBuf.trim()) {
          logger.debug('[agent-stderr]', stderrBuf);
        }
      });
    }

    const stdin = this.agentProcess.stdin;
    const stdout = this.agentProcess.stdout;

    stdin.on('error', (err) => {
      logger.error('Agent stdin error:', err);
    });

    // WritableStream wrapping node stdin
    const writable = new WritableStream<Uint8Array>({
      async write(chunk) {
        if (stdin.destroyed || stdin.writableEnded) return;
        return new Promise<void>((resolve, reject) => {
          stdin.write(chunk, (err) => {
            if (err) reject(err);
            else resolve();
          });
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

    // Parse ndjson directly from 'data' events and push parsed JSON-RPC
    // messages into a ReadableStream. This bypasses ndJsonStream's
    // reader.read() on a pipe-backed stream, which intermittently stalls
    // under Bun when Ink/React is rendering concurrently.
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
            const message = JSON.parse(trimmed);
            messageController.enqueue(message);
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

    // readable: pre-parsed messages (bypasses ndJsonStream reader)
    // writable: still uses ndJsonStream serialization for outgoing messages
    const dummyReadable = new ReadableStream<Uint8Array>({ start() {} });
    const ndJson = acp.ndJsonStream(writable, dummyReadable);
    const stream = { readable: parsedMessages, writable: ndJson.writable };

    this.connection = new acp.ClientSideConnection(() => this, stream);
  }

  // ===========
  // SessionClient interface methods
  // ===========

  async initialize(): Promise<void> {
    const initResult = await this.connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
      clientInfo: {
        name: 'kiro-tui',
        version: TUI_VERSION,
      },
    });

    logger.debug(
      '[acp-client] ACP handshake done, protocolVersion:',
      initResult.protocolVersion
    );
  }

  async newSession(): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    const sessionResult = await this.connection.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });

    this.sessionId = sessionResult.sessionId;
    logger.debug('ACP session created', {
      sessionId: this.sessionId,
      models: sessionResult.models,
      modes: sessionResult.modes,
    });

    // Extract current model info from session response
    let currentModel: { id: string; name: string } | undefined;
    if (
      sessionResult.models?.currentModelId &&
      sessionResult.models?.availableModels
    ) {
      const modelInfo = sessionResult.models.availableModels.find(
        (m) => m.modelId === sessionResult.models?.currentModelId
      );
      if (modelInfo) {
        currentModel = { id: modelInfo.modelId, name: modelInfo.name };
      }
    }

    // Extract current agent info from session response
    const currentAgent = extractCurrentAgent(sessionResult.modes);

    return { sessionId: this.sessionId, currentModel, currentAgent };
  }

  async loadSession(sessionId: string): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    // Update sessionId before the RPC so that history notifications
    // arriving during loadSession are not filtered as subagent events.
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;

    const sessionResult = await this.connection
      .loadSession({
        sessionId,
        cwd: process.cwd(),
        mcpServers: [],
      })
      .catch((err) => {
        // Restore previous session ID on failure
        this.sessionId = previousSessionId;
        throw err;
      });
    logger.debug('[acp-client] loadSession completed for session:', sessionId);
    logger.debug('ACP session loaded', {
      sessionId: this.sessionId,
    });

    // Extract current model info from session response
    let currentModel: { id: string; name: string } | undefined;
    if (
      sessionResult.models?.currentModelId &&
      sessionResult.models?.availableModels
    ) {
      const modelInfo = sessionResult.models.availableModels.find(
        (m) => m.modelId === sessionResult.models?.currentModelId
      );
      if (modelInfo) {
        currentModel = { id: modelInfo.modelId, name: modelInfo.name };
      }
    }

    // Extract current agent info from session response
    const currentAgent = extractCurrentAgent(sessionResult.modes);

    return { sessionId, currentModel, currentAgent };
  }

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId) {
      throw new Error('cannot send prompt without an active session');
    }

    // Fail fast if the connection is already closed
    if (this.connection.signal.aborted) {
      logger.error('[acp] prompt called but connection already closed');
      throw new Error('Agent connection closed unexpectedly');
    }

    logger.debug('[acp] sending prompt', { sessionId: this.sessionId });
    try {
      // Race the prompt against the connection closing to avoid hanging
      // if the backend process dies while we're waiting for a response
      const connectionClosed = new Promise<never>((_resolve, reject) => {
        if (this.connection.signal.aborted) {
          reject(new Error('Agent connection closed unexpectedly'));
          return;
        }
        this.connection.signal.addEventListener(
          'abort',
          () => {
            logger.error('[acp] connection closed while prompt was pending');
            reject(new Error('Agent connection closed unexpectedly'));
          },
          { once: true }
        );
      });
      // Suppress unhandled rejection if prompt wins the race
      connectionClosed.catch(() => {});

      await Promise.race([
        this.connection.prompt({
          prompt: messages,
          sessionId: this.sessionId,
        }),
        connectionClosed,
      ]);
    } catch (e) {
      logger.error('[acp] prompt failed', e);
      throw e;
    }
  }

  async cancel(): Promise<void> {
    if (!this.sessionId) return;

    try {
      await this.connection.cancel({ sessionId: this.sessionId });
      logger.debug('Cancel notification sent');
    } catch (e) {
      logger.error('Failed to send cancel notification:', e);
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionId) {
      return { success: false, message: 'No active session' };
    }

    try {
      // extMethod already prepends '_', so don't include it
      const result = await this.connection.extMethod(
        EXT_METHODS.COMMANDS_EXECUTE,
        {
          sessionId: this.sessionId,
          command,
        }
      );
      return result as unknown as CommandResult;
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Command failed',
      };
    }
  }

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionId) return;
    await this.connection.setSessionMode({ sessionId: this.sessionId, modeId });
  }

  async getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionId) {
      return { options: [] };
    }

    try {
      const result = await this.connection.extMethod(
        EXT_METHODS.COMMANDS_OPTIONS,
        {
          sessionId: this.sessionId,
          command: commandName.replace(/^\//, ''),
          partial,
        }
      );
      return result as unknown as CommandOptionsResponse;
    } catch {
      return { options: [] };
    }
  }

  close(): void {
    this.agentProcess.kill('SIGTERM');
  }

  async terminateSession(sessionId: string): Promise<void> {
    try {
      await this.connection.extMethod('kiro.dev/session/terminate', {
        sessionId,
      });
    } catch (err) {
      logger.debug('terminateSession failed (best-effort)', { sessionId, err });
    }
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    return (await this.connection.extMethod('kiro.dev/session/list', {
      cwd,
    })) as unknown as ListSessionsResponse;
  }

  async listSettings(): Promise<Record<string, unknown>> {
    const result = await this.connection.extMethod(
      'kiro.dev/settings/list',
      {}
    );
    return result as unknown as Record<string, unknown>;
  }

  // ===========
  // acp.Client interface methods
  // ===========

  async requestPermission(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    const response = await new Promise<acp.RequestPermissionResponse>(
      (resolve) => {
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
              const acpResponse: acp.RequestPermissionResponse =
                userResponse.outcome === 'selected'
                  ? {
                      outcome: {
                        outcome: 'selected' as const,
                        optionId: userResponse.optionId,
                      },
                      _meta: userResponse._meta,
                    }
                  : { outcome: { outcome: 'cancelled' as const } };
              resolve(acpResponse);
            },
          },
        };
        this.broadcastStreamEvent(event);
      }
    );

    return response;
  }

  async sessionUpdate(params: acp.SessionNotification): Promise<void> {
    const { update } = params;
    if (!update) return;
    logger.debug('[acp] sessionUpdate received:', update.sessionUpdate);
    const notifSessionId = (params as any).sessionId as string | undefined;
    const isSubagentEvent = notifSessionId && notifSessionId !== this.sessionId;
    const event = this.convertAcpUpdateToEvent(update);
    if (!event) return;

    if (isSubagentEvent) {
      // Always forward to multi-session handlers (crew monitor)
      this.multiSessionHandlers.forEach((h) => h(notifSessionId, event));

      // Tool call events from subagents also need to reach the main chat
      // because tool_call_chunk (ext notification) creates entries in the
      // main messages array — their updates/completions must land there too.
      const isToolEvent =
        event.type === AgentEventType.ToolCall ||
        event.type === AgentEventType.ToolCallUpdate ||
        event.type === AgentEventType.ToolCallFinished;
      if (isToolEvent) {
        this.broadcastStreamEvent(event);
      }
    } else {
      this.broadcastStreamEvent(event);
    }
  }

  async writeTextFile?(
    _params: acp.WriteTextFileRequest
  ): Promise<acp.WriteTextFileResponse> {
    throw new Error('writeTextFile not implemented');
  }

  async readTextFile?(
    _params: acp.ReadTextFileRequest
  ): Promise<acp.ReadTextFileResponse> {
    throw new Error('readTextFile not implemented');
  }

  async createTerminal?(
    _params: acp.CreateTerminalRequest
  ): Promise<acp.CreateTerminalResponse> {
    throw new Error('createTerminal not implemented');
  }

  async terminalOutput?(
    _params: acp.TerminalOutputRequest
  ): Promise<acp.TerminalOutputResponse> {
    throw new Error('terminalOutput not implemented');
  }

  async releaseTerminal?(
    _params: acp.ReleaseTerminalRequest
  ): Promise<acp.ReleaseTerminalResponse | void> {
    throw new Error('releaseTerminal not implemented');
  }

  async waitForTerminalExit?(
    _params: acp.WaitForTerminalExitRequest
  ): Promise<acp.WaitForTerminalExitResponse> {
    throw new Error('waitForTerminalExit not implemented');
  }

  async killTerminal?(
    _params: acp.KillTerminalCommandRequest
  ): Promise<acp.KillTerminalResponse | void> {
    throw new Error('killTerminal not implemented');
  }

  async extMethod?(
    _method: string,

    _params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    throw new Error('extMethod not implemented');
  }

  async extNotification?(
    method: string,
    params: Record<string, unknown>
  ): Promise<void> {
    logger.debug('[acp] extNotification:', method);
    // Handle custom commands available notification (SDK strips leading _)
    const handler = this.extNotificationHandlers[method];
    if (handler) {
      handler(params);
    }
  }
  private extNotificationHandlers: Record<
    string,
    (params: Record<string, unknown>) => void
  > = {
    [EXT_METHODS.COMMANDS_AVAILABLE]: (params) =>
      this.handleCommandsAdvertising(params),
    [EXT_METHODS.METADATA]: (params) => this.handleMetadataUpdate(params),
    [EXT_METHODS.COMPACTION_STATUS]: (params) =>
      this.handleCompactionStatus(params),
    [EXT_METHODS.CLEAR_STATUS]: () => this.handleClearStatus(),
    [EXT_METHODS.MCP_SERVER_INIT_FAILURE]: (params) =>
      this.handleMcpServerInitFailure(params),
    [EXT_METHODS.RATE_LIMIT_ERROR]: (params) =>
      this.handleRateLimitError(params),
    [EXT_METHODS.SUBAGENT_LIST_UPDATE]: (params) =>
      this.handleSubagentListUpdate(params),
    [EXT_METHODS.SESSION_ACTIVITY]: (params) =>
      this.handleSessionActivity(params),
    [EXT_METHODS.SESSION_LIST_UPDATE]: (params) =>
      this.handleSessionListUpdate(params),
    [EXT_METHODS.INBOX_NOTIFICATION]: (params) =>
      this.handleInboxNotification(params),
    [EXT_METHODS.AGENT_SWITCHED]: (params) => this.handleAgentSwitched(params),
    [EXT_METHODS.SESSION_UPDATE]: (params) =>
      this.handleExtSessionUpdate(params),
  };

  private handleCommandsAdvertising(params: Record<string, unknown>) {
    const commands =
      (params.commands as Array<{
        name: string;
        description: string;
        meta?: Record<string, unknown>;
      }>) || [];
    this.broadcastStreamEvent({
      type: AgentEventType.CommandsUpdate,
      commands: commands.map((cmd) => {
        // Enrich /tools and /mcp descriptions with metadata counts
        let description = cmd.description;
        if (cmd.name === 'tools' && tools.length > 0) {
          description = `${cmd.description} (${tools.length} available)`;
        } else if (cmd.name === 'mcp' && mcpServers.length > 0) {
          const running = mcpServers.filter(
            (s) => s.status === 'running'
          ).length;
          description = `${cmd.description} (${running}/${mcpServers.length} running)`;
        }
        return {
          name: cmd.name,
          description,
          meta: cmd.meta,
        };
      }),
    });

    const prompts =
      (params.prompts as Array<{
        name: string;
        description?: string;
        arguments: Array<{
          name: string;
          description?: string;
          required?: boolean;
        }>;
        serverName: string;
      }>) || [];

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

    logger.debug(
      '[acp] commands advertising: commands=',
      commands.length,
      'prompts=',
      prompts.length,
      'tools=',
      tools.length,
      'mcpServers=',
      mcpServers.length
    );
    this.broadcastStreamEvent({
      type: AgentEventType.PromptsUpdate,
      prompts,
    });
  }

  private handleMetadataUpdate(params: Record<string, unknown>) {
    const sessionId = params.sessionId as string | undefined;
    if (sessionId && sessionId !== this.sessionId) return;
    const percent =
      (params.contextUsagePercentage as number | undefined) ?? null;
    if (percent !== null) {
      this.broadcastStreamEvent({
        type: AgentEventType.ContextUsage,
        percent,
      });
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
  }

  private handleClearStatus() {
    logger.debug('Clear status received');
    // Context usage will be updated by the next METADATA notification
  }

  private handleCompactionStatus(params: Record<string, unknown>) {
    const status = params.status as {
      type: string;
      error?: string;
    };
    const summary = params.summary as string | undefined;
    logger.debug('Compaction status received:', status);
    if (status) {
      this.broadcastStreamEvent({
        type: AgentEventType.CompactionStatus,
        status: status.type as 'started' | 'completed' | 'failed',
        error: status.error,
        summary,
      });
      // Context usage will be updated by the next METADATA notification after compaction
    }
  }

  private handleMcpServerInitFailure(params: Record<string, unknown>) {
    const serverName = params.serverName as string;
    const error = params.error as string;
    logger.debug('MCP server init failure received:', { serverName, error });
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerInitFailure,
      serverName,
      error,
    });
  }

  private handleRateLimitError(params: Record<string, unknown>) {
    const message = params.message as string;
    logger.debug('Rate limit error received:', { message });
    this.broadcastStreamEvent({
      type: AgentEventType.RateLimitError,
      message,
    });
  }

  private handleSubagentListUpdate(params: Record<string, unknown>) {
    const subagents = (params as any)?.subagents ?? [];
    const pendingStages = (params as any)?.pendingStages ?? [];
    this.subagentListHandlers.forEach((h) => h(subagents, pendingStages));
  }

  private handleSessionActivity(params: Record<string, unknown>) {
    const sessionId = (params as any)?.sessionId as string;
    const event = (params as any)?.event as AgentStreamEvent;
    if (sessionId && event) {
      this.multiSessionHandlers.forEach((h) => h(sessionId, event));
    }
  }

  private handleSessionListUpdate(params: Record<string, unknown>) {
    const sessions = (params as any)?.sessions ?? [];
    this.subagentListHandlers.forEach((h) => h(sessions));
  }

  private handleInboxNotification(params: Record<string, unknown>) {
    this.inboxHandlers.forEach((h) => h(params));
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

  async spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    logger.debug('[spawnSession] calling ext method', {
      method: EXT_METHODS.SESSION_SPAWN,
      task,
      name,
    });
    try {
      const result = await this.connection.extMethod(
        EXT_METHODS.SESSION_SPAWN,
        {
          sessionId: this.sessionId,
          task,
          name,
        }
      );
      logger.debug('[spawnSession] result', result);
      return {
        sessionId: (result as any).sessionId,
        name: (result as any).name ?? name ?? '',
      };
    } catch (e) {
      logger.error('[spawnSession] failed', e);
      throw e;
    }
  }

  async sendMessage(sessionId: string, content: string): Promise<void> {
    await this.connection.extMethod(EXT_METHODS.MESSAGE_SEND, {
      sessionId,
      content,
    });
  }

  private handleAgentSwitched(params: Record<string, unknown>) {
    const payload = params as {
      agentName: string;
      previousAgentName?: string;
      welcomeMessage?: string;
    };
    this.broadcastStreamEvent({
      type: AgentEventType.AgentSwitched,
      agentName: payload.agentName,
      previousAgentName: payload.previousAgentName,
      welcomeMessage: payload.welcomeMessage,
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

      if (isSubagentEvent) {
        this.multiSessionHandlers.forEach((h) => h(sessionId, event));
      }
      this.broadcastStreamEvent(event);
    }
  }

  // ===========
  // Private helper methods
  // ===========

  private broadcastStreamEvent(event: AgentStreamEvent): void {
    this.updateHandlers.forEach((handler) => handler(event));
  }

  private convertAcpUpdateToEvent(
    update: AcpSessionUpdate
  ): AgentStreamEvent | null {
    switch (update.sessionUpdate) {
      case 'user_message_chunk': {
        switch (update.content.type) {
          case 'text':
            return {
              type: AgentEventType.UserMessage,
              id: crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
            };
          default:
            return null;
        }
      }

      case 'agent_message_chunk': {
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
            logger.debug('Unhandled content type:', update.content.type);
            return null;
        }
      }

      case 'tool_call': {
        // Extract diff content from ACP ToolCallContent
        const toolContent = ((update as any).content || [])
          .filter((c: any) => c.type === 'diff')
          .map((c: any) => ({
            type: 'diff' as const,
            path: c.path,
            newText: c.newText || '',
            oldText: c.oldText,
          }));

        // Extract locations from ACP
        const locations = ((update as any).locations || []).map((loc: any) => ({
          path: loc.path,
          line: loc.line,
        }));

        return {
          type: AgentEventType.ToolCall,
          id: update.toolCallId,
          name: update.title || 'unknown',
          kind: (update as any).kind,
          args: update.rawInput || {},
          toolContent: toolContent.length > 0 ? toolContent : undefined,
          locations: locations.length > 0 ? locations : undefined,
        };
      }

      case 'tool_call_update': {
        const toolCallUpdate = update as any;
        // Check if this is a completion update
        if (toolCallUpdate.status === ToolCallStatus.Completed) {
          return {
            type: AgentEventType.ToolCallFinished,
            id: toolCallUpdate.toolCallId,
            result: { status: 'success', output: toolCallUpdate.rawOutput },
          };
        }
        if (toolCallUpdate.status === ToolCallStatus.Failed) {
          return {
            type: AgentEventType.ToolCallFinished,
            id: toolCallUpdate.toolCallId,
            result: {
              status: 'error',
              error: toolCallUpdate.rawOutput || 'Tool execution failed',
            },
          };
        }
        return {
          type: AgentEventType.ToolCallUpdate,
          id: toolCallUpdate.toolCallId,
          content: { type: ContentType.Text, text: toolCallUpdate.content },
        };
      }

      case 'available_commands_update': {
        const commandsUpdate = update as any;
        return {
          type: AgentEventType.CommandsUpdate,
          commands: (commandsUpdate.availableCommands || []).map(
            (cmd: any) => ({
              name: cmd.name,
              description: cmd.description,
              meta: cmd._meta, // ACP uses _meta field
            })
          ),
        };
      }

      default:
        logger.debug('Unhandled session update type:', update.sessionUpdate);
        return null;
    }
  }
}
