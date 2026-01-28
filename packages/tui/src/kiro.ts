import { AcpClient } from './acp-client';
import { logger } from './utils/logger';
import { AgentEventType, type AgentStreamEvent } from './types/agent-events';
import type { SessionClient } from './types/session-client';
import type { CommandOptionsResponse, CommandResult, TuiCommand } from './types/commands';

/**
 * Stateless Kiro class that only manages session client lifecycle.
 * All state is managed externally in the app store.
 */
export class Kiro {
  private sessionClient?: SessionClient;
  private commandsHandler?: (commands: Array<{ name: string; description: string; meta?: Record<string, unknown> }>) => void;
  private modelHandler?: (model: { id: string; name: string }) => void;

  get sessionId(): string | undefined {
    return this.sessionClient?.sessionId;
  }

  onCommandsUpdate(handler: (commands: Array<{ name: string; description: string; meta?: Record<string, unknown> }>) => void): void {
    this.commandsHandler = handler;
  }

  onModelUpdate(handler: (model: { id: string; name: string }) => void): void {
    this.modelHandler = handler;
  }

  async initialize(agentPath: string): Promise<void> {
    logger.debug('Kiro initializing with agent:', agentPath);

    if (process.env.KIRO_MOCK_ACP === 'true') {
      const { MockSessionClient, setMockSessionClient } =
        await import('./test-utils/MockSessionClient');
      const mockClient = new MockSessionClient();
      this.sessionClient = mockClient;
      setMockSessionClient(mockClient);
    } else {
      this.sessionClient = new AcpClient(agentPath);
    }

    // Register handler for commands update before initialize
    this.sessionClient.onUpdate((event: AgentStreamEvent) => {
      if (event.type === AgentEventType.CommandsUpdate && this.commandsHandler) {
        this.commandsHandler(event.commands);
      }
    });

    await this.sessionClient.initialize();
    const sessionResult = await this.sessionClient.newSession();
    
    // Notify about current model if available
    if (sessionResult.currentModel && this.modelHandler) {
      this.modelHandler(sessionResult.currentModel);
    }
    
    logger.info('Kiro initialized successfully');
  }

  async *sendMessageStream(
    content: string,
    signal: AbortSignal
  ): AsyncGenerator<AgentStreamEvent> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }

    logger.info('sendMessageStream called', { content, sessionId: this.sessionId });

    const events: AgentStreamEvent[] = [];
    let promptCompleted = false;

    const updateHandler = (event: AgentStreamEvent) => {
      events.push(event);
    };

    const unsubscribe = this.sessionClient.onUpdate(updateHandler);

    try {
      const promptPromise = this.sessionClient
        .prompt([{ type: 'text', text: content }])
        .then(() => {
          promptCompleted = true;
        });

      while (!promptCompleted || events.length > 0) {
        if (signal.aborted) {
          await this.cancel();
          throw new DOMException('Aborted', 'AbortError');
        }

        if (events.length > 0) {
          yield events.shift()!;
        } else {
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      }

      await promptPromise;
    } finally {
      unsubscribe();
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    return this.sessionClient.executeCommand(command);
  }

  async getCommandOptions(commandName: string, partial: string = ''): Promise<CommandOptionsResponse> {
    if (!this.sessionClient) {
      return { options: [] };
    }
    return this.sessionClient.getCommandOptions(commandName, partial);
  }

  async cancel(): Promise<void> {
    if (!this.sessionClient) return;
    await this.sessionClient.cancel();
  }

  close(): void {
    if (this.sessionClient) {
      this.sessionClient.close();
      this.sessionClient = undefined;
    }
  }
}
