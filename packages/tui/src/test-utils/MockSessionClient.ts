import type { ContentBlock } from '@agentclientprotocol/sdk';
import type {
  SessionClient,
  ListSessionsResponse,
} from '../types/session-client';
import type {
  AgentStreamEvent,
  ApprovalRequestInfo,
} from '../types/agent-events';
import { AgentEventType } from '../types/agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from '../types/commands';
import { listSessionsForCwd } from '../utils/sessions.js';

// Global reference for test commands
let mockSessionClientInstance: MockSessionClient | null = null;

/**
 * Registers a MockSessionClient instance for access by test commands.
 *
 * @param client - The MockSessionClient instance to register
 */
export const setMockSessionClient = (client: MockSessionClient) => {
  mockSessionClientInstance = client;
};

/**
 * Retrieves the currently registered MockSessionClient instance.
 *
 * @returns The registered MockSessionClient or null if not in test mode
 */
export const getMockSessionClient = () => mockSessionClientInstance;

/**
 * MockSessionClient is a test implementation of the SessionClient interface.
 *
 * It replaces the real AcpClient during testing, allowing tests to inject
 * mock session events (content chunks, tool calls, approval requests) without
 * requiring a real ACP backend process. This enables fast, deterministic testing
 * of the TUI's response to various agent events.
 *
 * The mock client is automatically instantiated by Kiro when KIRO_TEST_MODE
 * is enabled, and tests can inject events via TestCase.mockSessionUpdate().
 */
export class MockSessionClient implements SessionClient {
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private eventQueue: AgentStreamEvent[] = [];
  private _promptActive = false;
  private _turnResolve: (() => void) | null = null;
  private _turnTimeout: ReturnType<typeof setTimeout> | null = null;
  public sessionId?: string;

  async initialize(): Promise<void> {
    // No-op for mock
  }

  async newSession(): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    this.sessionId = 'mock-session-id';
    const agentName = process.env.KIRO_MOCK_AGENT_NAME;
    const welcomeMessage = process.env.KIRO_MOCK_WELCOME_MESSAGE;
    return {
      sessionId: this.sessionId,
      currentModel: { id: 'mock-model', name: 'Mock Model' },
      ...(agentName
        ? { currentAgent: { name: agentName, welcomeMessage } }
        : {}),
    };
  }

  async loadSession(sessionId: string): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    this.sessionId = sessionId;
    return {
      sessionId,
      currentModel: { id: 'mock-model', name: 'Mock Model' },
    };
  }

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  prompt(_messages: ContentBlock[]): Promise<void> {
    this._promptActive = true;
    // Drain any pre-queued events synchronously so they're delivered before
    // settle() can fire. Since kiro.ts subscribes the per-prompt handler
    // BEFORE calling prompt(), events broadcast here reach that handler.
    while (this.eventQueue.length > 0) {
      const event = this.eventQueue.shift()!;
      this.processEvent(event);
    }

    // Return a deferred promise that keeps isProcessing=true in the store.
    // Tests call completeTurn() via IPC to resolve it when they're done
    // interacting mid-turn (e.g., pressing Ctrl+O for subagent panels).
    //
    // Auto-resolves after 2000ms as a safety net so no test hangs forever.
    // Tests that need the turn to end quickly for assertions on committed
    // message content should call completeTurn() explicitly.
    return new Promise<void>((resolve) => {
      this._turnResolve = () => {
        this._promptActive = false;
        this._turnResolve = null;
        if (this._turnTimeout) {
          clearTimeout(this._turnTimeout);
          this._turnTimeout = null;
        }
        resolve();
      };
      // Auto-resolve safety net. Default 2s. Tests that need a long-lived
      // turn (e.g. approval flow with the 2s APPROVAL_IDLE_MS typing-guard
      // followed by user response) can opt into a longer window via
      // KIRO_TEST_MOCK_TURN_TIMEOUT_MS. Setting this to 0 or a negative
      // value disables the safety net entirely.
      const overrideMs = Number(process.env.KIRO_TEST_MOCK_TURN_TIMEOUT_MS);
      const timeoutMs = Number.isFinite(overrideMs) ? overrideMs : 2000;
      if (timeoutMs > 0) {
        this._turnTimeout = setTimeout(() => {
          this._turnTimeout = null;
          if (this._turnResolve) {
            this._turnResolve();
          }
        }, timeoutMs);
      }
    });
  }

  /**
   * Explicitly resolves the pending prompt(), ending the mock turn.
   * This causes streamMessage() to resolve, which calls flush() to commit
   * buffered content to the store, then sets isProcessing=false.
   *
   * Call this when your test needs to:
   * - Verify committed message content (requires turn end + flush)
   * - Complete the turn before starting another interaction
   */
  completeTurn(): void {
    if (this._turnResolve) {
      this._turnResolve();
    }
  }

  async cancel(): Promise<void> {
    // Resolve any pending prompt so the process can exit cleanly.
    this.completeTurn();
  }

  close(): void {
    // No-op for mock
  }

  async terminateSession(_sessionId: string): Promise<void> {
    // No-op for mock
  }

  async sendMessage(_sessionId: string, _content: string): Promise<void> {
    // No-op for mock — used by tests that only care about the call itself.
  }

  async steerMessage(_sessionId: string, _content: string): Promise<void> {
    // No-op for mock.
  }

  async clearSteering(_sessionId: string): Promise<void> {
    // No-op for mock.
  }

  async setConfigOption(
    _configId: 'mode' | 'model' | 'effortLevel',
    _value: string
  ): Promise<void> {
    // No-op for mock
  }

  async listSettings(): Promise<Record<string, unknown>> {
    return {};
  }

  async setSetting(_key: string, _value: unknown): Promise<void> {
    // No-op for mock
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    if (process.env.KIRO_TEST_SESSIONS_DIR) {
      const entries = listSessionsForCwd(cwd);
      return {
        sessions: entries.map((e) => ({
          sessionId: e.sessionId,
          cwd: e.cwd,
          title: e.summary,
          updatedAt: e.updatedAt,
          messageCount: e.msgCount,
        })),
      };
    }
    return { sessions: [] };
  }

  // Test methods
  injectEvent(event: AgentStreamEvent): void {
    if (this._promptActive) {
      // Broadcast immediately if prompt is active (handlers are listening).
      // This simulates a real ACP client delivering events in real-time
      // during a streaming response.
      this.processEvent(event);
    } else {
      this.eventQueue.push(event);
    }
  }

  private processEvent(event: AgentStreamEvent): void {
    // Add resolve function for approval requests
    if (event.type === AgentEventType.ApprovalRequest) {
      const eventWithResolve = {
        ...event,
        value: {
          ...event.value,
          resolve: (response) => {
            // Mock resolve - could log or trigger other test behavior
            console.log('Mock approval resolved:', response);
          },
        } as ApprovalRequestInfo,
      };
      this.broadcastEvent(eventWithResolve);
    } else if (event.type === AgentEventType.QuestionRequest) {
      // Same treatment for questions: the resolve callback can't cross the
      // IPC boundary, so reconstitute it here.
      const eventWithResolve = {
        ...event,
        value: {
          ...event.value,
          resolve: (response: unknown) => {
            console.log('Mock question resolved:', response);
          },
        },
      };
      this.broadcastEvent(eventWithResolve);
    } else {
      this.broadcastEvent(event);
    }
  }

  private broadcastEvent(event: AgentStreamEvent): void {
    this.updateHandlers.forEach((handler) => handler(event));
  }

  async getCommandOptions(
    _commandName: string,

    _partial: string
  ): Promise<CommandOptionsResponse> {
    return { options: [] };
  }

  async executeCommand(_command: TuiCommand): Promise<CommandResult> {
    return { success: true, message: 'Mock command executed' };
  }
}
