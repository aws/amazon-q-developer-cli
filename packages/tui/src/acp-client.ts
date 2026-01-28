import * as acp from '@agentclientprotocol/sdk';
import { logger } from './utils/logger';
import { spawn, type ChildProcess } from 'node:child_process';
import { Readable } from 'node:stream';
import type { SessionClient } from './types/session-client';
import {
  AgentEventType,
  ContentType,
  ApprovalOptionId,
  ToolCallStatus,
  type AgentStreamEvent,
} from './types/agent-events';
import type { CommandOptionsResponse, CommandResult, TuiCommand } from './types/commands';
import { v4 as uuidv4 } from 'uuid';

export type AcpSessionUpdate = acp.SessionNotification['update'];

/** Custom extension method names (without leading underscore - SDK strips it) */
const EXT_METHODS = {
  COMMANDS_AVAILABLE: 'kiro.dev/commands/available',
  COMMANDS_EXECUTE: 'kiro.dev/commands/execute',
  COMMANDS_OPTIONS: 'kiro.dev/commands/options',
  METADATA: 'kiro.dev/metadata',
} as const;

/**
 * ACP client implementation that converts ACP protocol to app domain types
 */
export class AcpClient implements acp.Client, SessionClient {
  private connection: acp.ClientSideConnection;
  public sessionId?: string;
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private agentProcess: ChildProcess;

  constructor(agentPath: string) {
    this.agentProcess = spawn(agentPath, ['acp'], {
      stdio: ['pipe', 'pipe', 'inherit'],
      env: process.env,
    });

    if (!this.agentProcess.stdout || !this.agentProcess.stdin) {
      throw new Error('Failed to create agent process stdio streams');
    }

    const stdin = this.agentProcess.stdin;
    const stdout = this.agentProcess.stdout;

    stdin.on('error', (err) => {
      logger.error('Agent stdin error:', err);
    });

    // Create a WritableStream that writes to stdin
    const writable = new WritableStream<Uint8Array>({
      async write(chunk) {
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
        stdin.destroy(reason instanceof Error ? reason : new Error(String(reason)));
      }
    });

    const readable = Readable.toWeb(stdout) as unknown as ReadableStream<Uint8Array>;
    const stream = acp.ndJsonStream(writable, readable);

    this.connection = new acp.ClientSideConnection((_agent) => this, stream);
  }

  // ===========
  // SessionClient interface methods
  // ===========

  async initialize(): Promise<void> {
    logger.debug('Initializing ACP connection');

    const initResult = await this.connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
    });

    logger.debug('ACP connection initialized', {
      protocolVersion: initResult.protocolVersion,
    });
  }

  async newSession(): Promise<{ sessionId: string; currentModel?: { id: string; name: string } }> {
    const sessionResult = await this.connection.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });

    this.sessionId = sessionResult.sessionId;
    logger.debug('ACP session created', { sessionId: this.sessionId, models: sessionResult.models });

    // Extract current model info from session response
    let currentModel: { id: string; name: string } | undefined;
    if (sessionResult.models?.currentModelId && sessionResult.models?.availableModels) {
      const modelInfo = sessionResult.models.availableModels.find(
        (m) => m.modelId === sessionResult.models?.currentModelId
      );
      if (modelInfo) {
        currentModel = { id: modelInfo.modelId, name: modelInfo.name };
      }
    }

    return { sessionId: this.sessionId, currentModel };
  }

  async loadSession(sessionId: string): Promise<void> {
    this.sessionId = sessionId;
    // Implementation depends on ACP protocol support for loading sessions
  }

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId) {
      throw new Error('cannot send prompt without an active session');
    }

    logger.debug('sending prompt: ', messages);
    await this.connection.prompt({
      prompt: messages,
      sessionId: this.sessionId,
    });
    logger.debug('prompt completed');
  }

  async cancel(): Promise<void> {
    // Cancel implementation
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionId) {
      return { success: false, message: 'No active session' };
    }

    try {
      // extMethod already prepends '_', so don't include it
      const result = await this.connection.extMethod(EXT_METHODS.COMMANDS_EXECUTE, {
        sessionId: this.sessionId,
        command,
      });
      return result as unknown as CommandResult;
    } catch (e) {
      return { success: false, message: e instanceof Error ? e.message : 'Command failed' };
    }
  }

  async getCommandOptions(commandName: string, partial: string): Promise<CommandOptionsResponse> {
    if (!this.sessionId) {
      return { options: [] };
    }

    try {
      const result = await this.connection.extMethod(EXT_METHODS.COMMANDS_OPTIONS, {
        sessionId: this.sessionId,
        command: commandName.replace(/^\//, ''),
        partial,
      });
      return result as unknown as CommandOptionsResponse;
    } catch {
      return { options: [] };
    }
  }

  close(): void {
    this.agentProcess.kill();
  }

  // ===========
  // acp.Client interface methods
  // ===========

  async requestPermission(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    const response = await new Promise<acp.RequestPermissionResponse>((resolve) => {
      const event: AgentStreamEvent = {
        type: AgentEventType.ApprovalRequest,
        value: {
          toolCall: { toolCallId: params.toolCall?.toolCallId || '' },
          permissionOptions: (params.options || []).map((opt) => ({
            kind: opt.kind as ApprovalOptionId,
            name: opt.name,
            optionId: opt.optionId,
          })),
          resolve: (userResponse) => {
            const acpResponse: acp.RequestPermissionResponse =
              userResponse.outcome === 'selected'
                ? {
                    outcome: {
                      outcome: 'selected' as const,
                      optionId: userResponse.optionId,
                    },
                  }
                : { outcome: { outcome: 'cancelled' as const } };
            resolve(acpResponse);
          },
        },
      };
      this.broadcastStreamEvent(event);
    });
    
    return response;
  }

  async sessionUpdate(params: acp.SessionNotification): Promise<void> {
    logger.debug('Session update received', params);

    const { update } = params;
    if (update) {
      const event = this.convertAcpUpdateToEvent(update);
      if (event) {
        this.broadcastStreamEvent(event);
      }
    }
  }

  async writeTextFile?(
    params: acp.WriteTextFileRequest
  ): Promise<acp.WriteTextFileResponse> {
    throw new Error('writeTextFile not implemented');
  }

  async readTextFile?(
    params: acp.ReadTextFileRequest
  ): Promise<acp.ReadTextFileResponse> {
    throw new Error('readTextFile not implemented');
  }

  async createTerminal?(
    params: acp.CreateTerminalRequest
  ): Promise<acp.CreateTerminalResponse> {
    throw new Error('createTerminal not implemented');
  }

  async terminalOutput?(
    params: acp.TerminalOutputRequest
  ): Promise<acp.TerminalOutputResponse> {
    throw new Error('terminalOutput not implemented');
  }

  async releaseTerminal?(
    params: acp.ReleaseTerminalRequest
  ): Promise<acp.ReleaseTerminalResponse | void> {
    throw new Error('releaseTerminal not implemented');
  }

  async waitForTerminalExit?(
    params: acp.WaitForTerminalExitRequest
  ): Promise<acp.WaitForTerminalExitResponse> {
    throw new Error('waitForTerminalExit not implemented');
  }

  async killTerminal?(
    params: acp.KillTerminalCommandRequest
  ): Promise<acp.KillTerminalResponse | void> {
    throw new Error('killTerminal not implemented');
  }

  async extMethod?(
    method: string,
    params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    throw new Error('extMethod not implemented');
  }

  async extNotification?(
    method: string,
    params: Record<string, unknown>
  ): Promise<void> {
    logger.debug('Extension notification received:', method, params);

    // Handle custom commands available notification (SDK strips leading _)
    if (method === EXT_METHODS.COMMANDS_AVAILABLE) {
      const commands = (params.commands as Array<{ name: string; description: string; meta?: Record<string, unknown> }>) || [];
      this.broadcastStreamEvent({
        type: AgentEventType.CommandsUpdate,
        commands: commands.map((cmd) => ({
          name: cmd.name,
          description: cmd.description,
          meta: cmd.meta,
        })),
      });
    } else if (method === EXT_METHODS.METADATA) {
      const percent = (params.contextUsagePercentage as number | undefined) ?? null;
      if (percent !== null) {
        this.broadcastStreamEvent({
          type: AgentEventType.ContextUsage,
          percent,
        });
      }
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
      case 'agent_message_chunk': {
        switch (update.content.type) {
          case 'text':
            return {
              type: AgentEventType.Content,
              id: uuidv4(),
              content: { type: ContentType.Text, text: update.content.text },
            };
          case 'image':
            return {
              type: AgentEventType.Content,
              id: uuidv4(),
              content: { type: ContentType.Image, image: update.content },
            };
          default:
            logger.debug('Unhandled content type:', update.content.type);
            return null;
        }
      }

      case 'tool_call': {
        return {
          type: AgentEventType.ToolCall,
          id: update.toolCallId,
          name: update.title || 'unknown',
          args: update.rawInput || {},
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
            result: { status: 'error', error: toolCallUpdate.rawOutput || 'Tool execution failed' },
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
          commands: (commandsUpdate.availableCommands || []).map((cmd: any) => ({
            name: cmd.name,
            description: cmd.description,
            meta: cmd._meta,  // ACP uses _meta field
          })),
        };
      }

      default:
        logger.debug('Unhandled session update type:', update.sessionUpdate);
        return null;
    }
  }
}
