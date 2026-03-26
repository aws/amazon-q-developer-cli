import { AcpClient } from './acp-client';
import { logger } from './utils/logger';
import { AgentEventType, type AgentStreamEvent } from './types/agent-events';
import type {
  SessionClient,
  ListSessionsResponse,
} from './types/session-client';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from './types/commands';

/**
 * Stateless Kiro class that only manages session client lifecycle.
 * All state is managed externally in the app store.
 */
export class Kiro {
  private sessionClient?: SessionClient;
  private _settings: Record<string, unknown> = {};
  private commandsHandler?: (
    commands: Array<{
      name: string;
      description: string;
      meta?: Record<string, unknown>;
    }>
  ) => void;
  private promptsHandler?: (
    prompts: Array<{
      name: string;
      description?: string;
      arguments: Array<{
        name: string;
        description?: string;
        required?: boolean;
      }>;
      serverName: string;
    }>
  ) => void;
  private modelHandler?: (model: { id: string; name: string }) => void;
  private agentHandler?: (agent: {
    name: string;
    welcomeMessage?: string;
  }) => void;
  private compactionHandler?: (event: AgentStreamEvent) => void;
  private settingsHandler?: (settings: Record<string, unknown>) => void;
  private historyHandler?: (event: AgentStreamEvent) => void;
  private globalUpdateUnsubscribe?: () => void;
  private pendingPrompt: Promise<void> | null = null;

  get sessionId(): string | undefined {
    return this.sessionClient?.sessionId;
  }

  get settings(): Record<string, unknown> {
    return this._settings;
  }

  onCommandsUpdate(
    handler: (
      commands: Array<{
        name: string;
        description: string;
        meta?: Record<string, unknown>;
      }>
    ) => void
  ): void {
    this.commandsHandler = handler;
  }

  onPromptsUpdate(
    handler: (
      prompts: Array<{
        name: string;
        description?: string;
        arguments: Array<{
          name: string;
          description?: string;
          required?: boolean;
        }>;
        serverName: string;
      }>
    ) => void
  ): void {
    this.promptsHandler = handler;
  }

  onModelUpdate(handler: (model: { id: string; name: string }) => void): void {
    this.modelHandler = handler;
  }

  onAgentUpdate(
    handler: (agent: { name: string; welcomeMessage?: string }) => void
  ): void {
    this.agentHandler = handler;
  }

  onCompactionStatus(handler: (event: AgentStreamEvent) => void): void {
    this.compactionHandler = handler;
  }

  onHistoryEvent(handler: (event: AgentStreamEvent) => void): void {
    this.historyHandler = handler;
  }

  async initialize(
    agentPath: string,
    extraAcpArgs: string[] = []
  ): Promise<void> {
    logger.debug('[kiro] initialize() called');

    if (process.env.KIRO_MOCK_ACP === 'true') {
      const { MockSessionClient, setMockSessionClient } =
        await import('./test-utils/MockSessionClient');
      const mockClient = new MockSessionClient();
      this.sessionClient = mockClient;
      setMockSessionClient(mockClient);
    } else {
      this.sessionClient = new AcpClient(agentPath, extraAcpArgs);
    }
    logger.debug('[kiro] AcpClient created');

    // Register handler for commands update before initialize
    this.globalUpdateUnsubscribe = this.sessionClient.onUpdate(
      (event: AgentStreamEvent) => {
        logger.debug('[kiro] global handler event:', event.type);
        if (
          event.type === AgentEventType.CommandsUpdate &&
          this.commandsHandler
        ) {
          this.commandsHandler(event.commands);
        }
        if (
          event.type === AgentEventType.PromptsUpdate &&
          this.promptsHandler
        ) {
          logger.debug(
            '[kiro] received PromptsUpdate event with',
            event.prompts.length,
            'prompts'
          );
          this.promptsHandler(event.prompts);
        }
        // Forward compaction, context usage, and compaction summary content events
        if (
          (event.type === AgentEventType.CompactionStatus ||
            event.type === AgentEventType.ContextUsage ||
            event.type === AgentEventType.Content) &&
          this.compactionHandler
        ) {
          this.compactionHandler(event);
        }
        // Handle backend-initiated agent switch (e.g. switch_to_execution)
        if (event.type === AgentEventType.AgentSwitched) {
          logger.debug('[kiro] AgentSwitched received:', event.agentName);
          if (this.agentHandler) {
            this.agentHandler({
              name: event.agentName,
              welcomeMessage: event.welcomeMessage,
            });
          }
        }
        // Forward historical content events (user messages, assistant text,
        // tool calls) so the store can populate the message list on resume.
        if (
          event.type === AgentEventType.UserMessage ||
          event.type === AgentEventType.Content ||
          event.type === AgentEventType.ToolCall ||
          event.type === AgentEventType.ToolCallUpdate ||
          event.type === AgentEventType.ToolCallFinished
        ) {
          if (this.historyHandler) {
            this.historyHandler(event);
          }
        }
      }
    );

    await this.sessionClient.initialize();

    // Fetch user settings before creating a session (needed for greeting display)
    try {
      this._settings = await this.sessionClient.listSettings();
    } catch (err) {
      logger.error('[kiro] Failed to fetch settings:', err);
    }
  }

  async createSession(resumeSessionId?: string): Promise<void> {
    if (!this.sessionClient) throw new Error('connect() must be called first');

    // Use loadSession if resuming, otherwise create new session
    const sessionResult = resumeSessionId
      ? await this.sessionClient.loadSession(resumeSessionId)
      : await this.sessionClient.newSession();

    // Notify about current model if available
    if (sessionResult.currentModel && this.modelHandler) {
      this.modelHandler(sessionResult.currentModel);
    }

    // Notify about current agent if available
    if (sessionResult.currentAgent && this.agentHandler) {
      this.agentHandler(sessionResult.currentAgent);
    }

    logger.debug(
      resumeSessionId
        ? `Kiro initialized with resumed session: ${resumeSessionId}`
        : 'Kiro initialized successfully'
    );
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
    images?: Array<{ base64: string; mimeType: string }>
  ): Promise<void> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }

    logger.debug('[stream] streamMessage called', {
      contentLength: content.length,
    });

    const INITIAL_RESPONSE_TIMEOUT_MS = 90_000;
    let receivedFirstEvent = false;
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    return new Promise<void>((resolve, reject) => {
      let settled = false;
      const settle = (fn: () => void) => {
        if (settled) return;
        settled = true;
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
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
        logger.debug('[stream] signal aborted, cancelling');
        settle(() => {
          reject(new DOMException('Aborted', 'AbortError'));
        });
      };
      if (signal.aborted) {
        onAbort();
        return;
      }
      signal.addEventListener('abort', onAbort, { once: true });

      const updateHandler = (event: AgentStreamEvent) => {
        // Allow events to be delivered even after settled — the prompt
        // response and notifications race in the ACP SDK, so late
        // notifications must still reach the store.  The store's event
        // handler is idempotent so duplicate delivery is harmless.
        //
        // Filter out UserMessage events — those are historical replays
        // from the backend during session load, not live prompt responses.
        // Processing them here would duplicate the already-loaded history.
        if (event.type === AgentEventType.UserMessage) {
          return;
        }
        receivedFirstEvent = true;
        // Clear the initial-response timeout once we get any event
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
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

      const contentBlocks: Array<
        | { type: 'text'; text: string }
        | { type: 'image'; data: string; mimeType: string }
      > = [];
      if (images?.length) {
        for (const img of images) {
          contentBlocks.push({
            type: 'image',
            data: img.base64,
            mimeType: img.mimeType,
          });
        }
      }
      contentBlocks.push({ type: 'text', text: content });

      const promptPromise = this.sessionClient!.prompt(contentBlocks as any)
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

      // Track the prompt RPC so cancel can wait for the backend to actually
      // clear pending_prompt_response before we send the next prompt.
      this.pendingPrompt = promptPromise.then(
        () => {},
        () => {}
      );
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

  async getCommandOptions(
    commandName: string,
    partial: string = ''
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionClient) {
      return { options: [] };
    }
    return this.sessionClient.getCommandOptions(commandName, partial);
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    if (!this.sessionClient) {
      return { sessions: [] };
    }
    return this.sessionClient.listSessions(cwd);
  }

  async loadSession(
    sessionId: string,
    onHistoryEvent?: (event: AgentStreamEvent) => void
  ): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    const previousSessionId = this.sessionId;
    // Register a direct onUpdate subscriber to capture history events
    // that arrive before the loadSession RPC response.
    const unsubscribe = onHistoryEvent
      ? this.sessionClient.onUpdate(onHistoryEvent)
      : undefined;
    try {
      logger.debug('[kiro] calling loadSession', { sessionId });
      const result = await this.sessionClient.loadSession(sessionId);
      logger.debug('[kiro] loadSession returned', { sessionId });
      // Only terminate the previous session after successful load
      if (previousSessionId) {
        logger.debug('[kiro] terminating previous session', {
          previousSessionId,
        });
        await this.sessionClient.terminateSession(previousSessionId);
      }
      return result;
    } finally {
      // Defer unsubscribe so in-flight notifications can still be delivered
      if (unsubscribe) {
        setTimeout(unsubscribe, 0);
      }
    }
  }

  async cancel(): Promise<void> {
    if (!this.sessionClient) return;
    await this.sessionClient.cancel();
    if (this.pendingPrompt) {
      await this.pendingPrompt;
      this.pendingPrompt = null;
    }
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
