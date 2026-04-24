import type { ContentBlock } from '@agentclientprotocol/sdk';
import type { AgentStreamEvent } from './agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from './commands';

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
   * Sends a message to a session.
   *
   * @param sessionId - The session ID
   * @param content - The message content
   */
  sendMessage?(sessionId: string, content: string): Promise<void>;

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
