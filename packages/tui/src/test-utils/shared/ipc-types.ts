/**
 * Shared IPC interfaces for test communication between test runners and application processes.
 * Used by both integration tests (TUI only) and E2E tests (TUI + Rust backend).
 */

import type { AppState } from '../../stores/app-store';
import type { AgentStreamEvent } from '../../types/agent-events';
import type { MockStreamItem } from '../../../e2e_tests/types/chat-cli';

// Import AgentSnapshot type when available from E2E type generation
// TODO: This will be generated from Rust AgentSnapshot struct
export interface AgentSnapshot {
  // Placeholder - will be replaced with generated types
  sessionId?: string;
  messageCount?: number;
}

export type TestCommand =
  | { kind: 'GET_STORE' }
  | { kind: 'GET_AGENT_STATE' }
  | { kind: 'PUSH_SEND_MESSAGE_RESPONSE'; session_id: string; events: MockStreamItem[] | null }
  | { kind: 'MOCK_SESSION_UPDATE'; event: AgentStreamEvent }
  | { kind: 'MOCK_ERROR'; error: string }
  | { kind: 'HEAP_SNAPSHOT'; filename: string }
  | { kind: 'MEMORY_USAGE' }
  | { kind: 'FORCE_GC' };

export interface MemoryUsageData {
  rss: number;
  heapUsed: number;
  heapTotal: number;
  external: number;
  arrayBuffers: number;
}

export type TestResponse =
  | { kind: 'GET_STORE'; data: AppState }
  | { kind: 'GET_AGENT_STATE'; data: AgentSnapshot }
  | { kind: 'PUSH_SEND_MESSAGE_RESPONSE' }
  | { kind: 'MOCK_SESSION_UPDATE' }
  | { kind: 'MOCK_ERROR' }
  | { kind: 'HEAP_SNAPSHOT'; filename: string }
  | { kind: 'MEMORY_USAGE'; data: MemoryUsageData }
  | { kind: 'FORCE_GC' }
  | { kind: 'ERROR'; error: string };

export interface TestMessageCommand {
  /** Request id */
  id: string;
  kind: 'command';
  data: TestCommand;
}

export interface TestMessageResponse {
  /** Request id */
  id: string;
  kind: 'response';
  data: TestResponse;
}

export type TestMessage = TestMessageCommand | TestMessageResponse;
