import type { ContentBlock } from '@agentclientprotocol/sdk';
import type {
  SpecInvokeRequest,
  SpecInvokeResponse,
  SpecResolveSessionRequest,
  SpecResolveSessionResponse,
} from '@kiro/acp-type-covenant';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import type { ContextBreakdownData } from '../stores/app-store';
import type { AgentStreamEvent } from './agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from './commands';
import type { ModeChangedNotification } from './generated/chat-cli';

// ── KAS /context wire shapes ──────────────────────────────────────────
// TODO: Replace these inline definitions with the typed `ContextParams`
// / `ContextResponse` exports from `@kiro/acp-type-covenant` once the
// kiro-agent PR (https://github.com/kiro-team/kiro-agent/pull/844) lands
// and the package is bumped past 0.3.11.

/**
 * One entry in the agent's attached-files list returned by
 * `_kiro/session/context show` when no breakdown has been pushed yet.
 */
export interface KasContextEntry {
  path: string;
  category?: string;
  /**
   * False when the path no longer resolves on disk — the handler
   * surfaces these with a ⚠ glyph and downgrades the alert tone.
   */
  matched?: boolean;
}

/**
 * Response payload for the `show` subcommand.  Either populated with the
 * agent's attached-files list, or empty (the handler then renders a soft
 * "no context attached" warning).
 */
export interface KasContextShowResponse {
  entries?: KasContextEntry[];
  message?: string;
}

/**
 * Response payload for `add` / `remove` / `clear` subcommands.
 *
 * `success` here is the agent's *domain-level* success (e.g. add reports
 * `success: false` for "path not found"). Distinct from JSON-RPC success
 * — RPC failures throw and never reach this shape.
 */
export interface KasContextMutationResponse {
  success?: boolean;
  message?: string;
}

/**
 * Light abstraction over the Agent Client Protocol (ACP) for interacting with the Kiro CLI agent.
 */
export interface SessionClient {
  /**
   * The current session ID for the main agent (not subagents).
   */
  readonly sessionId?: string;

  /**
   * Initializes the session client connection (an `initialize` request in the ACP protocol).
   */
  initialize(): Promise<void>;

  /**
   * Creates a new agent session (a `session/new` request in the ACP protocol).
   *
   * @returns Promise resolving to session info including ID and current model
   */
  newSession(): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }>;

  /**
   * Loads an existing agent session by ID (a `session/load` request in the ACP protocol).
   *
   * @param sessionId - The session ID to load
   * @returns Promise resolving to session info including ID and current model/agent
   */
  loadSession(sessionId: string): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }>;

  /**
   * Lists sessions, optionally filtered by working directory.
   *
   * @param cwd - Working directory to filter by
   */
  listSessions(cwd: string): Promise<ListSessionsResponse>;

  /**
   * Registers a callback to receive events about the agent's execution during a prompt turn lifecycle.
   *
   * This includes:
   * - session/update events (agent text, thought, and tool calls)
   * - tool use approval requests
   * - hook execution
   *
   * @param handler - Callback function that receives AgentStreamEvent objects
   * @returns Unsubscribe function to remove the handler
   */
  onUpdate(handler: (event: AgentStreamEvent) => void): () => void;

  /**
   * Sends a new prompt to the agent (a `session/prompt` request in the ACP protocol).
   *
   * Resolves when the full turn lifecycle completes as defined in the ACP specification:
   * https://agentclientprotocol.com/protocol/prompt-turn#the-prompt-turn-lifecycle
   *
   * Updates about the agent's execution, including permission requests, will be sent
   * through the onUpdate callback during the turn lifecycle.
   *
   * @param message - Array of content blocks to send as the prompt
   */
  prompt(message: ContentBlock[]): Promise<void>;

  /**
   * Cancels the current agent operation (a `session/cancel` notification in the ACP protocol).
   */
  cancel(): Promise<void>;

  /**
   * Fetches options for a slash command (extension method).
   *
   * @param commandName - The command name (e.g., "/model")
   * @param partial - Partial input for filtering options
   */
  getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse>;

  /**
   * Executes a slash command via extension method.
   *
   * @param command - The strongly-typed TuiCommand to execute
   */
  executeCommand(command: TuiCommand): Promise<CommandResult>;

  /**
   * Closes the session client connection and cleans up resources.
   *
   * For a real implementation, this would terminate the ACP process.
   */
  close(): void;

  /**
   * Terminates a session, unloading it from memory in the ACP process.
   *
   * @param sessionId - The session ID to terminate
   */
  terminateSession(sessionId: string): Promise<void>;

  /**
   * Sets the agent mode/persona.
   *
   * @param modeId - The mode ID to switch to
   */
  setMode(modeId: string): Promise<void>;

  /**
   * Fetches the global user settings from the backend.
   * Returns a flat map using the same dotted key names as the settings file
   *
   * Note - v1 supports local and global settings, currently unsupported with chat-cli-v2
   */
  listSettings(): Promise<Record<string, unknown>>;

  /**
   * Sets a single user setting via the backend, which performs a locked
   * read-modify-write on the settings file.  This avoids race conditions
   * when both the TUI and Rust backend write settings concurrently.
   *
   * @param key - Dotted setting key (e.g. "chat.disableTrustAllConfirmation")
   * @param value - The value to set
   */
  setSetting(key: string, value: unknown): Promise<void>;

  /**
   * Spawns a new session.
   *
   * @param task - The task for the session
   * @param name - Optional name for the session
   */
  spawnSession?(task: string, name?: string): Promise<{ sessionId: string }>;

  /**
   * Sends a message to an existing session, waking it if idle.
   *
   * Used for replies into a persistent subagent / crew session (e.g. from
   * the session-view screen). Routes through `_message/send → wake_session`,
   * which starts (or resumes) a full turn on the target session.
   *
   * For mid-turn steering of the *active* session, use `steerMessage()`
   * instead — steering has different semantics (queue for injection at the
   * next drain point rather than start a new turn).
   *
   * @param sessionId - The session ID
   * @param content - The message content
   */
  sendMessage(sessionId: string, content: string): Promise<void>;

  /**
   * Queues a steering message for mid-turn injection on an active session.
   *
   * The backend holds the queue and drains it at the next tool boundary or
   * at end-of-turn. Safe to call repeatedly while the agent is busy; multiple
   * messages concatenate on the backend.
   *
   * @param sessionId - The session ID
   * @param content - The steering content
   */
  steerMessage(sessionId: string, content: string): Promise<void>;

  /**
   * Clears the queued steering message without consuming it.
   *
   * Complements `steerMessage()` — lets the TUI remove a pending steer
   * without also cancelling the in-flight turn.
   *
   * @param sessionId - The session ID
   */
  clearSteering(sessionId: string): Promise<void>;

  /**
   * Registers a callback for subagent list updates.
   *
   * @param handler - Callback function that receives subagent list
   */
  onSubagentListUpdate?(
    handler: (subagents: any[], pendingStages?: any[]) => void
  ): () => void;

  /**
   * Registers a callback for session events.
   *
   * @param handler - Callback function that receives session events
   */
  onSessionEvent?(handler: (event: any) => void): () => void;

  /**
   * Registers a callback for multi-session updates.
   *
   * @param handler - Callback function that receives session ID and event
   */
  onMultiSessionUpdate?(
    handler: (sessionId: string, event: AgentStreamEvent) => void
  ): () => void;

  /**
   * Resolves (or creates) the ACP session the agent uses to work on a
   * spec feature.  Implemented only by engines that support the
   * `_kiro/spec/resolveSession` extension method (currently KAS).
   *
   * Callers should check that the method exists before invoking it; the
   * `Kiro` class wraps this with a friendlier "spec workflow not
   * supported" error when the method is absent.
   */
  resolveSpecSession?(
    request: SpecResolveSessionRequest
  ): Promise<SpecResolveSessionResponse>;

  /**
   * Invokes a spec operation (`executeTask`, `runAllTasks`, or
   * `generateDocument`) on the agent.  Implemented only by engines that
   * support the `_kiro/spec/invoke` extension method (currently KAS).
   *
   * See {@link resolveSpecSession} for caller responsibilities.
   */
  invokeSpec?(request: SpecInvokeRequest): Promise<SpecInvokeResponse>;

  /**
   * Sends process health metrics to the telemetry pipeline.
   * Fire-and-forget — implementations should not throw.
   */
  sendProcessHealthMetrics?(payload: ProcessHealthSnapshot): void;

  /**
   * Sends a `modeChanged` telemetry event when the active agent (= ACP session mode) changes.
   * Caller is responsible for skipping no-op changes (`fromMode === toMode`).
   * Fire-and-forget — implementations should not throw.
   */
  sendModeChanged?(payload: ModeChangedNotification): void;

  // ── KAS /context ext methods ───────────────────────────────────────
  // Each method maps 1:1 to a `_kiro/session/context` call with the
  // matching `subcommand`. Implemented only by engines that support the
  // ext method (currently KAS); the `Kiro` wrapper checks for presence
  // and throws "Context management is not supported" when absent.

  contextShow?(): Promise<KasContextShowResponse>;
  contextAdd?(
    path: string,
    opts?: { force?: boolean }
  ): Promise<KasContextMutationResponse>;
  contextRemove?(path: string): Promise<KasContextMutationResponse>;
  contextClear?(): Promise<KasContextMutationResponse>;

  /**
   * Latest context-usage breakdown pushed via `session_info_update`
   * notifications, or `null` if none has been pushed yet.
   *
   * Used by the /context handler to short-circuit the panel open without
   * a round-trip to the agent. KAS-only.
   */
  getCachedContextBreakdown?(): ContextBreakdownData | null;
}

/**
 * TODO - duplicated type until we modify this flow to use a session/list compatible sacp implementation.
 */
export interface ListSessionsResponse {
  sessions: SessionInfoEntry[];
  nextCursor?: string;
}

/**
 * TODO - duplicated type until we modify this flow to use a session/list compatible sacp implementation.
 */
export interface SessionInfoEntry {
  sessionId: string;
  cwd: string;
  title?: string;
  updatedAt?: string;
  messageCount?: number;
}
