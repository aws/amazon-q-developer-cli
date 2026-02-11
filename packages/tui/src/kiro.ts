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
  private agentHandler?: (agent: { name: string }) => void;
  private compactionHandler?: (event: AgentStreamEvent) => void;
  private globalUpdateUnsubscribe?: () => void;

  get sessionId(): string | undefined {
    return this.sessionClient?.sessionId;
  }

  onCommandsUpdate(handler: (commands: Array<{ name: string; description: string; meta?: Record<string, unknown> }>) => void): void {
    this.commandsHandler = handler;
  }

  onModelUpdate(handler: (model: { id: string; name: string }) => void): void {
    this.modelHandler = handler;
  }

  onAgentUpdate(handler: (agent: { name: string }) => void): void {
    this.agentHandler = handler;
  }

  onCompactionStatus(handler: (event: AgentStreamEvent) => void): void {
    this.compactionHandler = handler;
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
    this.globalUpdateUnsubscribe = this.sessionClient.onUpdate((event: AgentStreamEvent) => {
      if (event.type === AgentEventType.CommandsUpdate && this.commandsHandler) {
        this.commandsHandler(event.commands);
      }
      // Forward compaction and context usage events (arrive after command returns)
      if ((event.type === AgentEventType.CompactionStatus || event.type === AgentEventType.ContextUsage) && this.compactionHandler) {
        this.compactionHandler(event);
      }
    });

    await this.sessionClient.initialize();
    const sessionResult = await this.sessionClient.newSession();
    
    // Notify about current model if available
    if (sessionResult.currentModel && this.modelHandler) {
      this.modelHandler(sessionResult.currentModel);
    }
    
    // Notify about current agent if available
    if (sessionResult.currentAgent && this.agentHandler) {
      this.agentHandler(sessionResult.currentAgent);
    }
    
    logger.info('Kiro initialized successfully');
  }

  /**
     * Stream a message to the backend, invoking `onEvent` for each event.
     *
     * Returns a Promise that resolves when the prompt completes (all events
     * delivered) or rejects on error / abort.
     */
    async streamMessage(
      content: string,
      signal: AbortSignal,
      onEvent: (event: AgentStreamEvent) => void,
    ): Promise<void> {
      if (!this.sessionClient) {
        throw new Error('Kiro not initialized');
      }

      logger.info('[stream] streamMessage called', { contentLength: content.length });

      const INITIAL_RESPONSE_TIMEOUT_MS = 90_000;
      let receivedFirstEvent = false;
      let timeoutId: ReturnType<typeof setTimeout> | null = null;

      return new Promise<void>((resolve, reject) => {
        let settled = false;
        const settle = (fn: () => void) => {
          if (settled) return;
          settled = true;
          if (timeoutId) { clearTimeout(timeoutId); timeoutId = null; }
          // Defer unsubscribe so that in-flight notification handlers in the
          // ACP SDK can finish broadcasting before we remove our listener.
          //
          // The ACP SDK's Connection.#receive loop fires #processMessage
          // without awaiting it, so JSON-RPC notifications and the prompt
          // response are processed concurrently.  When the response resolves
          // the prompt promise synchronously, this settle() callback runs
          // before pending notification microtasks have called
          // broadcastStreamEvent.  Deferring the unsubscribe by one macrotask
          // gives those handlers time to deliver their events.
          setTimeout(() => unsubscribe(), 0);
          fn();
        };

        // Handle abort signal
        const onAbort = () => {
          logger.info('[stream] signal aborted, cancelling');
          settle(() => {
            this.cancel().catch(() => {});
            reject(new DOMException('Aborted', 'AbortError'));
          });
        };
        if (signal.aborted) { onAbort(); return; }
        signal.addEventListener('abort', onAbort, { once: true });

        const updateHandler = (event: AgentStreamEvent) => {
          // Allow events to be delivered even after settled — the prompt
          // response and notifications race in the ACP SDK, so late
          // notifications must still reach the store.  The store's event
          // handler is idempotent so duplicate delivery is harmless.
          receivedFirstEvent = true;
          // Clear the initial-response timeout once we get any event
          if (timeoutId) { clearTimeout(timeoutId); timeoutId = null; }
          try {
            onEvent(event);
          } catch (err) {
            logger.error('[stream] onEvent threw', err);
          }
        };

        const unsubscribe = this.sessionClient!.onUpdate(updateHandler);

        // Start initial-response timeout
        timeoutId = setTimeout(() => {
          if (!receivedFirstEvent && !settled) {
            logger.error('[stream] initial response timeout reached');
            settle(() =>
              reject(
                new Error(
                  'Agent not responding. The backend may be misconfigured or unresponsive. Press Ctrl+C to cancel.'
                )
              )
            );
          }
        }, INITIAL_RESPONSE_TIMEOUT_MS);

        this.sessionClient!
          .prompt([{ type: 'text', text: content }])
          .then(() => {
            settle(() => resolve());
          })
          .catch((err) => {
            let errorMessage = 'Unknown error';
            if (err instanceof Error) {
              const errData = (err as any).data;
              if (typeof errData === 'string' && errData) {
                errorMessage = errData;
              } else if (err.message && err.message !== 'Internal error') {
                errorMessage = err.message;
              } else {
                errorMessage = err.message || 'Unknown error';
              }
            } else if (typeof err === 'object' && err !== null) {
              if ('data' in err && typeof err.data === 'string' && err.data) {
                errorMessage = err.data;
              } else if ('message' in err && typeof err.message === 'string') {
                errorMessage = err.message;
              }
            } else if (typeof err === 'string') {
              errorMessage = err;
            }
            logger.error('[stream] prompt failed:', errorMessage);
            settle(() => reject(new Error(errorMessage)));
          });
      });
    }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    return this.sessionClient.executeCommand(command);
  }

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionClient) return;
    await this.sessionClient.setMode(modeId);
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
    if (this.globalUpdateUnsubscribe) {
      this.globalUpdateUnsubscribe();
      this.globalUpdateUnsubscribe = undefined;
    }
    if (this.sessionClient) {
      this.sessionClient.close();
      this.sessionClient = undefined;
    }
  }
}
