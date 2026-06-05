import { createAcpClient } from './acp-client';
import { logger } from './utils/logger';
import { extractRpcErrorMessage } from './utils/error-handling';
import { AgentEventType, type AgentStreamEvent } from './types/agent-events';
import {
  isFileWriteToolName,
  isWriteOperation,
  extractToolPath,
  matchSpecArtifactPath,
  type SpecArtifactPathMatch,
} from './utils/spec-artifact-path';
import type { ProcessHealthSnapshot } from './utils/process-health-collector';
import type {
  SessionClient,
  ListSessionsResponse,
  KasContextShowResponse,
  KasContextMutationResponse,
} from './types/session-client';
import type { ModeChangedNotification } from './types/generated/chat-cli';
import type { ContextBreakdownData } from './stores/app-store';
import type {
  CommandOptionsResponse,
  CommandResult,
  CommandMeta,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
  TuiCommand,
} from './types/commands';
import type { KasCommand } from './kas-commands';
import type {
  SpecInvokeRequest,
  SpecInvokeResponse,
  SpecResolveSessionRequest,
  SpecResolveSessionResponse,
} from '@kiro/acp-type-covenant';

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
      meta?: CommandMeta;
    }>
  ) => void;
  private kasCommandsHandler?: (commands: KasCommand[]) => void;
  private promptsHandler?: (prompts: PromptEntry[]) => void;
  private skillsHandler?: (skills: SkillEntry[]) => void;
  private steeringHandler?: (steering: SteeringEntry[]) => void;
  private modelHandler?: (model: { id: string; name: string }) => void;
  private agentHandler?: (agent: {
    name: string;
    welcomeMessage?: string;
  }) => void;
  private compactionHandler?: (event: AgentStreamEvent) => void;
  private settingsHandler?: (settings: Record<string, unknown>) => void;
  private subagentListHandler?: (
    subagents: any[],
    pendingStages: any[]
  ) => void;
  private sessionEventHandler?: (event: any) => void;
  private multiSessionHandler?: (sessionId: string, event: any) => void;
  private inboxHandler?: (notification: any) => void;
  private historyHandler?: (event: AgentStreamEvent) => void;
  private turnSummaryHandler?: (event: AgentStreamEvent) => void;
  private initNotificationHandler?: (event: AgentStreamEvent) => void;
  private artifactWriteHandler?: (match: SpecArtifactPathMatch) => void;
  private artifactFinishHandler?: (match: SpecArtifactPathMatch) => void;
  /**
   * Track in-flight write tool calls by id so we can correlate a
   * `ToolCallFinished` event back to the spec-artifact path that the
   * `ToolCall` carried. The `ToolCallFinished` payload is indexed only
   * by id and does not include the original args.
   */
  private artifactWriteCallsById: Map<string, SpecArtifactPathMatch> =
    new Map();
  private approvalHandler?: (event: AgentStreamEvent) => void;
  private globalUpdateUnsubscribe?: () => void;
  private pendingPrompt: Promise<void> | null = null;
  private _promptActive = false;

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
        meta?: CommandMeta;
      }>
    ) => void
  ): void {
    this.commandsHandler = handler;
  }

  onKasCommandsDiscovered(handler: (commands: KasCommand[]) => void): void {
    this.kasCommandsHandler = handler;
  }

  onPromptsUpdate(handler: (prompts: PromptEntry[]) => void): void {
    this.promptsHandler = handler;
  }

  onSkillsUpdate(handler: (skills: SkillEntry[]) => void): void {
    this.skillsHandler = handler;
  }

  onSteeringUpdate(handler: (steering: SteeringEntry[]) => void): void {
    this.steeringHandler = handler;
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

  /**
   * Register a handler for notifications that arrive during initialization
   * (before any prompt), such as MCP server failures and agent config errors.
   */
  onInitNotification(handler: (event: AgentStreamEvent) => void): void {
    this.initNotificationHandler = handler;
  }

  /**
   * Register a handler for approval requests that arrive outside of an active
   * sendMessage() call — e.g. from background sessions spawned via /spawn.
   */
  onApprovalRequest(handler: (event: AgentStreamEvent) => void): void {
    this.approvalHandler = handler;
  }

  onSubagentListUpdate(
    handler: (subagents: any[], pendingStages: any[]) => void
  ): void {
    this.subagentListHandler = handler;
    if (this.sessionClient && 'onSubagentListUpdate' in this.sessionClient) {
      (this.sessionClient as any).onSubagentListUpdate(handler);
    }
  }

  onSessionEvent(handler: (event: any) => void): void {
    this.sessionEventHandler = handler;
  }

  onMultiSessionUpdate(handler: (sessionId: string, event: any) => void): void {
    this.multiSessionHandler = handler;
    if (this.sessionClient && 'onMultiSessionUpdate' in this.sessionClient) {
      (this.sessionClient as any).onMultiSessionUpdate(handler);
    }
  }

  onInboxNotification(handler: (notification: any) => void): void {
    this.inboxHandler = handler;
    if (this.sessionClient && 'onInboxNotification' in this.sessionClient) {
      (this.sessionClient as any).onInboxNotification(handler);
    }
  }

  async spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    return (this.sessionClient as any).spawnSession(task, name);
  }

  /**
   * Resolve (or create) the ACP session the agent uses for a spec feature.
   *
   * KAS-only: V1 engines have no spec workflow.  Throws when the active
   * session client doesn't implement the `_kiro/spec/resolveSession`
   * extension method.
   */
  async resolveSpecSession(
    request: SpecResolveSessionRequest
  ): Promise<SpecResolveSessionResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.resolveSpecSession) {
      throw new Error(
        'Spec workflow is not supported by the current agent engine'
      );
    }
    return this.sessionClient.resolveSpecSession(request);
  }

  /**
   * Invoke a spec operation (`executeTask`, `runAllTasks`, or
   * `generateDocument`) on the agent.  See `_kiro/spec/invoke` in
   * `@kiro/acp-type-covenant`.
   *
   * KAS-only: throws when the active session client doesn't implement
   * the `_kiro/spec/invoke` extension method.
   */
  async invokeSpec(request: SpecInvokeRequest): Promise<SpecInvokeResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.invokeSpec) {
      throw new Error(
        'Spec workflow is not supported by the current agent engine'
      );
    }
    return this.sessionClient.invokeSpec(request);
  }

  // ── KAS /context ext methods ─────────────────────────────────────────
  // Each method below is a KAS-only thin wrapper around the matching
  // `SessionClient.context*` typed method. Throws "Context management
  // not supported" when the underlying session client doesn't expose the
  // ext method (i.e. V1/V2-Rust engines).

  async contextShow(): Promise<KasContextShowResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.contextShow) {
      throw new Error(
        'Context management is not supported by the current agent engine'
      );
    }
    return this.sessionClient.contextShow();
  }

  async contextAdd(
    path: string,
    opts?: { force?: boolean }
  ): Promise<KasContextMutationResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.contextAdd) {
      throw new Error(
        'Context management is not supported by the current agent engine'
      );
    }
    return this.sessionClient.contextAdd(path, opts);
  }

  async contextRemove(path: string): Promise<KasContextMutationResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.contextRemove) {
      throw new Error(
        'Context management is not supported by the current agent engine'
      );
    }
    return this.sessionClient.contextRemove(path);
  }

  async contextClear(): Promise<KasContextMutationResponse> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    if (!this.sessionClient.contextClear) {
      throw new Error(
        'Context management is not supported by the current agent engine'
      );
    }
    return this.sessionClient.contextClear();
  }

  /**
   * Latest context-usage breakdown pushed via `session_info_update`,
   * or `null` if the underlying session client doesn't expose one
   * (V1/V2-Rust) or hasn't received one yet (KAS, pre-first-event).
   */
  getCachedContextBreakdown(): ContextBreakdownData | null {
    return this.sessionClient?.getCachedContextBreakdown?.() ?? null;
  }

  async sendMessage(sessionId: string, content: string): Promise<void> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    this.onSessionMessageSent?.(sessionId);
    return (this.sessionClient as any).sendMessage(sessionId, content);
  }

  onSessionMessageSent?: (sessionId: string) => void;

  sendProcessHealthMetrics(payload: ProcessHealthSnapshot): void {
    if (!this.sessionClient) return;
    this.sessionClient.sendProcessHealthMetrics?.(payload);
  }

  sendModeChanged(payload: ModeChangedNotification): void {
    if (!this.sessionClient) return;
    this.sessionClient.sendModeChanged?.(payload);
  }

  async terminateSession(sessionId: string): Promise<void> {
    if (!this.sessionClient) return;
    return (this.sessionClient as any).terminateSession(sessionId);
  }

  onHistoryEvent(handler: (event: AgentStreamEvent) => void): void {
    this.historyHandler = handler;
  }

  onTurnSummary(handler: (event: AgentStreamEvent) => void): void {
    this.turnSummaryHandler = handler;
  }

  /**
   * Register a handler for agent-initiated writes to spec artifact files
   * under `.kiro/specs/<feature>/{requirements,design,tasks}.md`. Fires
   * once per `tool_call` notification whose tool name is a write tool
   * and whose `args.path` matches the spec-artifact path pattern.
   *
   * The handler is engine-agnostic but the consuming layer (index.tsx)
   * is responsible for KAS gating — V1 / V2-Rust never write the
   * relevant tool calls in a way that should drive this UI, but we
   * filter on engine at registration time as defence-in-depth.
   */
  onArtifactWrite(handler: (match: SpecArtifactPathMatch) => void): void {
    this.artifactWriteHandler = handler;
  }

  /**
   * Register a callback that fires once a tracked spec-artifact write
   * tool call completes (i.e. when the agent's `ToolCallFinished`
   * arrives for an id we previously saw write to a spec artifact).
   *
   * Use this to re-parse the on-disk artifact when the parser failed
   * mid-stream — by `ToolCallFinished` the file has been fully flushed
   * to disk and the second parse is reliable.
   */
  onArtifactFinish(handler: (match: SpecArtifactPathMatch) => void): void {
    this.artifactFinishHandler = handler;
  }

  async initialize(
    agentPath: string,
    extraAcpArgs: string[] = [],
    kasOptions?: { initialAgent?: string }
  ): Promise<void> {
    logger.debug('[kiro] initialize() called');

    if (process.env.KIRO_MOCK_ACP === 'true') {
      const { MockSessionClient, setMockSessionClient } =
        await import('./test-utils/MockSessionClient');
      const mockClient = new MockSessionClient();
      this.sessionClient = mockClient;
      setMockSessionClient(mockClient);
    } else {
      this.sessionClient = createAcpClient(agentPath, extraAcpArgs, kasOptions);
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
          event.type === AgentEventType.KasCommandsDiscovered &&
          this.kasCommandsHandler
        ) {
          this.kasCommandsHandler(event.commands);
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
        if (event.type === AgentEventType.SkillsUpdate && this.skillsHandler) {
          logger.debug(
            '[kiro] received SkillsUpdate event with',
            event.skills.length,
            'skills'
          );
          this.skillsHandler(event.skills);
        }
        if (
          event.type === AgentEventType.SteeringUpdate &&
          this.steeringHandler
        ) {
          logger.debug(
            '[kiro] received SteeringUpdate event with',
            event.steering.length,
            'steering docs'
          );
          this.steeringHandler(event.steering);
        }
        // Forward compaction, context usage, and compaction summary content events
        if (
          (event.type === AgentEventType.CompactionStatus ||
            event.type === AgentEventType.ContextUsage ||
            event.type === AgentEventType.EffortUpdate ||
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
          if (event.model && this.modelHandler) {
            this.modelHandler({ id: event.model, name: event.model });
          }
        }
        // Forward init-time notifications (MCP failures, agent errors, OAuth) to the store
        if (
          (event.type === AgentEventType.McpServerInitFailure ||
            event.type === AgentEventType.McpOauthRequest ||
            event.type === AgentEventType.McpServerInitialized ||
            event.type === AgentEventType.McpGovernanceDisabled ||
            event.type === AgentEventType.AgentNotFound ||
            event.type === AgentEventType.AgentConfigError) &&
          this.initNotificationHandler
        ) {
          this.initNotificationHandler(event);
        }
        // Forward approval requests from background sessions (e.g. /spawn)
        // so they surface in the UI even when no sendMessage() is active.
        // Skip when a prompt is active — the per-message handler already covers it.
        if (
          event.type === AgentEventType.ApprovalRequest &&
          this.approvalHandler &&
          !this._promptActive
        ) {
          this.approvalHandler(event);
        }
        // Forward historical content events (user messages, assistant text,
        // tool calls) so the store can populate the message list on resume.
        if (event.type === AgentEventType.TurnSummary) {
          if (this.turnSummaryHandler) {
            this.turnSummaryHandler(event);
          }
        }
        if (
          event.type === AgentEventType.UserMessage ||
          event.type === AgentEventType.Content ||
          event.type === AgentEventType.Thought ||
          event.type === AgentEventType.ToolCall ||
          event.type === AgentEventType.ToolCallUpdate ||
          event.type === AgentEventType.ToolCallFinished
        ) {
          if (this.historyHandler) {
            this.historyHandler(event);
          }
        }
        // Spec artifact write detection. KAS-gated at registration: in
        // non-KAS engines no handler is attached so this branch is dead.
        if (
          event.type === AgentEventType.ToolCall &&
          this.artifactWriteHandler
        ) {
          if (isFileWriteToolName(event.name) && isWriteOperation(event.args)) {
            const path = extractToolPath(event.args);
            if (path) {
              const match = matchSpecArtifactPath(path, process.cwd());
              if (match) {
                // Log only on a successful spec-artifact match. Keeps the
                // debug stream readable during agent streaming and avoids
                // serialising args.argKeys for every unrelated ToolCall.
                logger.debug('[kiro] spec-artifact write', {
                  name: event.name,
                  sessionId: event.sessionId,
                  path,
                  artifact: match.artifact,
                  featureName: match.featureName,
                });
                // Track the in-flight call so we can re-parse on
                // ToolCallFinished. Stored even when no finish handler
                // is registered — the lookup is cheap and harmless.
                this.artifactWriteCallsById.set(event.id, match);
                this.artifactWriteHandler(match);
              }
            }
          }
        }
        // Re-parse on tool completion so the final on-disk content
        // becomes the source of truth (the mid-stream parse may have
        // observed a half-written file with a missing closing fence,
        // truncated heading, etc.). The store-side action is idempotent
        // so a duplicate parse on the happy path is harmless.
        if (event.type === AgentEventType.ToolCallFinished) {
          const match = this.artifactWriteCallsById.get(event.id);
          if (match) {
            this.artifactWriteCallsById.delete(event.id);
            this.artifactFinishHandler?.(match);
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

    // Wire up handlers after creating sessionClient
    if (this.sessionEventHandler && 'onSessionEvent' in this.sessionClient) {
      (this.sessionClient as any).onSessionEvent(this.sessionEventHandler);
    }
    if (
      this.multiSessionHandler &&
      'onMultiSessionUpdate' in this.sessionClient
    ) {
      (this.sessionClient as any).onMultiSessionUpdate(
        this.multiSessionHandler
      );
    }
    if (
      this.subagentListHandler &&
      'onSubagentListUpdate' in this.sessionClient
    ) {
      (this.sessionClient as any).onSubagentListUpdate(
        this.subagentListHandler
      );
    }
    if (this.inboxHandler && 'onInboxNotification' in this.sessionClient) {
      (this.sessionClient as any).onInboxNotification(this.inboxHandler);
    }

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

    const INITIAL_RESPONSE_TIMEOUT_MS =
      Number(process.env.KIRO_INITIAL_RESPONSE_TIMEOUT_MS) || 600_000;
    let receivedFirstEvent = false;
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    return new Promise<void>((resolve, reject) => {
      let settled = false;
      // Track when the most recent stream event arrived so the stuck-turn
      // watchdog (defined below) can log the silence gap. Initialized to
      // Promise-creation time and bumped from updateHandler on every
      // non-historical event.
      let lastEventAt = Date.now();
      let watchdogId: ReturnType<typeof setInterval> | null = null;
      const settle = (reason: string, fn: () => void) => {
        if (settled) return;
        settled = true;
        // Silent-stop diagnostic: which path settled the turn (abort,
        // prompt-resolved, prompt-error, initial-response-timeout) and how
        // long it had been since the last event arrived. If a real session
        // reports a silent stop, the absence of any settle log for the
        // affected turn (combined with the watchdog's repeated "no events
        // for Nms" warnings) is the fingerprint.
        const sinceLastEvent = Date.now() - lastEventAt;
        logger.debug(
          `[stream] settle path=${reason} sinceLastEvent=${sinceLastEvent}ms receivedFirstEvent=${receivedFirstEvent}`
        );
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
        if (watchdogId) {
          clearInterval(watchdogId);
          watchdogId = null;
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
        setTimeout(() => {
          this._promptActive = false;
          unsubscribe();
        }, 0);
        fn();
      };

      // Handle abort signal
      const onAbort = () => {
        logger.debug('[stream] signal aborted, cancelling');
        settle('abort', () => {
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
        lastEventAt = Date.now();
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

      // Subscribe before setting the flag — if an event arrives between these
      // two lines, double-delivery (both handlers fire) is harmless since the
      // store's ApprovalRequest handler is idempotent. Lost delivery is not.
      const unsubscribe = this.sessionClient!.onUpdate(updateHandler);
      this._promptActive = true;

      // Start initial-response timeout
      timeoutId = setTimeout(() => {
        if (!receivedFirstEvent && !settled) {
          logger.error(
            '[stream] initial response timeout reached after',
            INITIAL_RESPONSE_TIMEOUT_MS,
            'ms — sending session/cancel to backend'
          );
          // Send cancel to backend so it clears the pending prompt state.
          // Without this, the backend still thinks a prompt is in progress
          // and will reject the next request with "Prompt already in progress".
          this.sessionClient
            ?.cancel()
            .catch((cancelErr) => {
              logger.error(
                '[stream] failed to send cancel on timeout:',
                cancelErr
              );
            })
            .finally(() => {
              settle('initial-response-timeout', () =>
                reject(
                  new Error(
                    'Agent not responding. The backend may be misconfigured or unresponsive. Press Ctrl+C to cancel.'
                  )
                )
              );
            });
        }
      }, INITIAL_RESPONSE_TIMEOUT_MS);

      // Stuck-turn watchdog — passive logger, no behavior change.
      // Fires every STUCK_TURN_LOG_INTERVAL_MS while the turn is in flight;
      // when no events have arrived in that window it emits a warn log
      // including the gap. If the silent-stop bug fires, the log will show
      // a cliff: events flowing normally, then repeated "no events for Nms"
      // entries with no corresponding settle log. Pairs with the backend
      // lag/notification loggers to triangulate where in the pipeline updates
      // went missing.
      const STUCK_TURN_LOG_INTERVAL_MS = 30_000;
      watchdogId = setInterval(() => {
        if (settled) return;
        const sinceLastEvent = Date.now() - lastEventAt;
        if (sinceLastEvent >= STUCK_TURN_LOG_INTERVAL_MS) {
          logger.warn(
            `[stream] no events for ${sinceLastEvent}ms — turn may be stuck (TUI still showing isProcessing=true, receivedFirstEvent=${receivedFirstEvent})`
          );
        }
      }, STUCK_TURN_LOG_INTERVAL_MS);

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
          settle('prompt-resolved', () => resolve());
        })
        .catch((err) => {
          const errorMessage = extractRpcErrorMessage(err);
          logger.error('[stream] prompt failed:', errorMessage);
          settle('prompt-error', () => reject(new Error(errorMessage)));
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

  async setSetting(key: string, value: unknown): Promise<void> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    return this.sessionClient.setSetting(key, value);
  }

  async newSession(): Promise<{
    sessionId: string;
    currentModel?: { id: string; name: string };
    currentAgent?: { name: string; welcomeMessage?: string };
  }> {
    if (!this.sessionClient) {
      throw new Error('Kiro not initialized');
    }
    const previousSessionId = this.sessionId;
    logger.debug('[kiro] creating new session');
    const result = await this.sessionClient.newSession();
    logger.debug('[kiro] new session created', {
      sessionId: result.sessionId,
    });
    if (previousSessionId) {
      logger.debug('[kiro] terminating previous session', {
        previousSessionId,
      });
      await this.sessionClient.terminateSession(previousSessionId);
    }
    return result;
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
      // Race against a timeout so we don't hang forever if KAS never responds.
      let timer: ReturnType<typeof setTimeout>;
      await Promise.race([
        this.pendingPrompt,
        new Promise<void>((r) => {
          timer = setTimeout(r, 5000);
        }),
      ]).finally(() => clearTimeout(timer!));
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
