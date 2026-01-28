import type { ContentBlock } from '@agentclientprotocol/sdk';
import type { SessionClient } from '../types/session-client';
import type {
  AgentStreamEvent,
  ApprovalRequestInfo,
} from '../types/agent-events';
import { AgentEventType } from '../types/agent-events';
import type { CommandOptionsResponse, CommandResult, TuiCommand } from '../types/commands';

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
  public sessionId?: string;

  async initialize(): Promise<void> {
    // No-op for mock
  }

  async newSession(): Promise<{ sessionId: string; currentModel?: { id: string; name: string } }> {
    this.sessionId = 'mock-session-id';
    return { sessionId: this.sessionId, currentModel: { id: 'mock-model', name: 'Mock Model' } };
  }

  async loadSession(sessionId: string): Promise<void> {
    this.sessionId = sessionId;
  }

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  async prompt(messages: ContentBlock[]): Promise<void> {
    // Process any queued events
    setTimeout(() => {
      while (this.eventQueue.length > 0) {
        const event = this.eventQueue.shift()!;
        this.processEvent(event);
      }
    }, 10);
  }

  async cancel(): Promise<void> {
    // No-op for mock
  }

  close(): void {
    // No-op for mock
  }

  // Test methods
  injectEvent(event: AgentStreamEvent): void {
    this.eventQueue.push(event);
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
    } else {
      this.broadcastEvent(event);
    }
  }

  private broadcastEvent(event: AgentStreamEvent): void {
    this.updateHandlers.forEach((handler) => handler(event));
  }

  async getCommandOptions(_commandName: string, _partial: string): Promise<CommandOptionsResponse> {
    return { options: [] };
  }

  async executeCommand(_command: TuiCommand): Promise<CommandResult> {
    return { success: true, message: 'Mock command executed' };
  }
}
