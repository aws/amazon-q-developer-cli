import type { SessionActivityStatus } from '@kiro/acp-type-covenant';
import type { ContentBlock } from '@agentclientprotocol/sdk';
import type {
  SpecInvokeRequest,
  SpecInvokeResponse,
  SpecResolveSessionRequest,
  SpecResolveSessionResponse,
} from '@kiro/acp-type-covenant';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import type { ContextBreakdownData } from './context';
import type { AgentStreamEvent } from './agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from './commands';
import type {
  ModeChangedNotification,
  UiModeChangedNotification,
  UiModeDefaultChangedNotification,
  UiModeSessionStartNotification,
} from './generated/chat-cli';
import type { SessionEvent } from './multi-session';
import type { WorkflowConversationApi } from './workflow';
import type { WorkflowControlApi } from './workflow-history';
import type { InterruptMode } from '../constants/interrupt-mode';

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
  /**
   * Freshly-computed breakdown for the current agent. Absent on older
   * agents — callers fall back to the cached breakdown.
   */
  breakdown?: ContextBreakdownData;
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
  loadSession(
    sessionId: string,
    options?: { source?: 'local' | 'remote' }
  ): Promise<{
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
   * Forks the current session into a new branch (a `session/fork` request).
   * Optional — only KAS/V3 clients implement it; other engines omit it.
   */
  fork?(opts: {
    messageId?: string;
    createdReason: CreatedReason;
    title?: string;
  }): Promise<CommandResult>;

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
   * Set a session config option (agent mode / model / effort level).
   *
   * The V2 engine routes only `'mode'` here (model/effort go through the
   * executeCommand round-trip) and throws for the others; KAS handles all
   * three via `session/set_config_option` and re-emits the normalized
   * config-option events.
   */
  setConfigOption(
    configId: 'mode' | 'model' | 'effortLevel',
    value: string
  ): Promise<void>;

  /**
   * Switch the session mode via ACP `session/set_mode` (as opposed to the
   * `session/set_config_option` route used by {@link setConfigOption}).
   * The response is empty and KAS emits no `current_mode_update` afterwards,
   * so implementations must self-report the switch (KAS broadcasts the
   * AgentSwitched event on success). Implemented only by engines that route
   * mode changes through `session/set_mode` directly (currently KAS).
   *
   * @param modeId - TUI-facing mode id (mapped to the wire id internally)
   */
  setSessionMode?(modeId: string): Promise<void>;

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

  /** Synchronizes workflow notification delivery with the active interrupt mode. */
  setWorkflowNotificationDelivery?(delivery: InterruptMode): Promise<void>;

  /** Optional engine capability for workflow-owned child conversations. */
  readonly workflowConversation?: WorkflowConversationApi;

  /** Optional engine capability for workflow history and run controls. */
  readonly workflowControl?: WorkflowControlApi;

  /**
   * Registers a callback for crew roster snapshots.
   *
   * Each invocation delivers the COMPLETE current roster (live subagents
   * plus planned-but-unspawned DAG stages), not a delta; consumers
   * reconcile by replacement, treating rows absent from a snapshot as
   * terminated. This is the only channel carrying roster identity and
   * DAG/loop metadata. The main session never appears in it.
   *
   * @param handler - Receives the full subagent list and pending stages
   * @returns Unsubscribe function
   */
  onSubagentListUpdate?(
    handler: (subagents: any[], pendingStages?: any[]) => void
  ): () => void;

  /** Registers incremental session lifecycle and workflow-control events. */
  onSessionEvent?(handler: (event: SessionEvent) => void): () => void;

  /**
   * Registers a callback for the per-subagent turn stream: content,
   * thoughts, and tool-call lifecycle for every session OTHER than this
   * client's main session, tagged with the emitting subagent's id. The
   * main session's own stream never arrives here (it goes to `onUpdate`),
   * but subagent tool events are additionally mirrored onto `onUpdate`
   * with the subagent's id stamped, so `onUpdate` consumers must branch on
   * `event.sessionId`.
   *
   * @param handler - Receives the subagent session ID and its stream event
   * @returns Unsubscribe function
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

  /**
   * Sends a `uiModeSessionStart` telemetry event after the TUI resolves its UI mode at
   * startup. Fire-and-forget — implementations should not throw.
   */
  sendUiModeSessionStart?(payload: UiModeSessionStartNotification): void;

  /**
   * Sends a `uiModeChanged` telemetry event when the user toggles between lite and tui
   * mid-session. Caller is responsible for skipping no-op changes.
   * Fire-and-forget — implementations should not throw.
   */
  sendUiModeChanged?(payload: UiModeChangedNotification): void;

  /**
   * Sends a `uiModeDefaultChanged` telemetry event when /settings → display
   * writes a new value to the persisted chat.ui.mode setting. Caller is
   * responsible for skipping no-ops.
   * Fire-and-forget — implementations should not throw.
   */
  sendUiModeDefaultChanged?(payload: UiModeDefaultChangedNotification): void;

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
   * Resets an MCP server connection, optionally starting the OAuth flow.
   * When startOAuth is true, KAS creates a local redirect server and opens
   * the browser with the correct redirect URI.
   */
  resetMcpServer?(serverName: string, startOAuth: boolean): Promise<void>;

  recordSlashCommandInvocation?(command: string): void;
}

/**
 * Where a session's agent runs. The wire union serves `{ kind: 'local' }` and
 * `{ kind: 'cloud-sandbox' }` only; `remote-control` is not on the wire yet — never send it.
 * Absent / `{ kind: 'local' }` == today's behavior.
 */
export type ExecutionTarget =
  | { kind: 'local' }
  | { kind: 'cloud-sandbox' }
  | { kind: 'remote-control'; host?: unknown };

/**
 * Which store a session record was discovered from. Distinct from the engine
 * `SessionSource` (V1/V2/Kas) — this is the KAS ACP discovery axis.
 */
export type SessionDiscoverySource = 'local' | 'remote';

// Roster + activity-status contract: defined by the covenant; re-exported here
// so TUI consumers keep one import hub for session-client types.
export type {
  SessionActivityStatus,
  ProvisioningFailureCode,
  SessionRosterEntry,
  SessionRosterUpsert,
  SessionsChangedNotification,
} from '@kiro/acp-type-covenant';

/**
 * Kiro-namespaced capabilities advertised by KAS on the `initialize` handshake,
 * under `agentCapabilities._meta.kiro`. Not all are remote-specific (e.g.
 * `sessionListScopes: ['workspace']` is local-first). All optional: an older KAS that
 * predates these caps omits them, and the client must then degrade gracefully (never
 * request an unadvertised placement/source/scope/method). Gate each flow on its own
 * flag — never one global "remote is on" switch.
 */
export interface KiroAgentCapabilities {
  /** Execution placements KAS accepts on `session/new` (e.g. 'local', 'cloud-sandbox'). */
  executionTargets?: string[];
  /** Discovery stores KAS can list from (e.g. 'local', 'remote'). */
  sessionSources?: string[];
  /** List scopes KAS supports (e.g. 'workspace', 'user'). */
  sessionListScopes?: string[];
  /**
   * The `_kiro/*` extension methods this KAS serves. The two `_kiro/sourceProviders/*`
   * methods appear here only when {@link sourceProviders} is true; gate a `_kiro/*`
   * call on this list before issuing it.
   */
  extensionMethods?: string[];
  /** Whether the `_kiro/sourceProviders/*` surface (repo picker) is available. */
  sourceProviders?: boolean;
}

/**
 * TODO - duplicated type until we modify this flow to use a session/list compatible sacp implementation.
 */
export interface ListSessionsResponse {
  sessions: SessionInfoEntry[];
  nextCursor?: string;
  /**
   * Set by `KasAcpClient.listSessions` when the underlying list RPC failed
   * (as opposed to a genuinely empty result). Lets callers distinguish a read
   * failure from "no sessions" and abort instead of mutating state.
   */
  failed?: boolean;
}

/** Reason a session was forked. A closed set produced by the TUI. */
export type CreatedReason = 'tangent' | 'rewind';

/**
 * TODO - duplicated type until we modify this flow to use a session/list compatible sacp implementation.
 */
export interface SessionInfoEntry {
  sessionId: string;
  cwd: string;
  title?: string;
  updatedAt?: string;
  messageCount?: number;
  /**
   * Where the session runs. Absent == local (today's behavior). Populated for
   * cloud sessions once KAS reports it on list entries.
   */
  executionTarget?: ExecutionTarget;
  /** Which store surfaced this record (local vs remote). Absent == local. */
  source?: SessionDiscoverySource;
  /** Liveness snapshot at list time; live updates via `_kiro/sessions/changed`. */
  status?: SessionActivityStatus;
  /** Parent session id when this session was forked (tangent/rewind/subagent). */
  parentSessionId?: string;
}
