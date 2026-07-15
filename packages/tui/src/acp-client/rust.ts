import * as acp from '@agentclientprotocol/sdk';
import { logger } from '../utils/logger';
import { maybeWrapStreamWithRecorder } from '../acp-recorder';
import { spawn } from 'node:child_process';
import type {
  ChatSlashCommandTelemetryPayload,
  ListSessionsResponse,
} from '../types/session-client';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import type {
  ModeChangedNotification,
  UiModeChangedNotification,
  UiModeDefaultChangedNotification,
  UiModeSessionStartNotification,
} from '../types/generated/chat-cli';
import { AgentEventType, type AgentStreamEvent } from '../types/agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from '../types/commands';
import { getCliVersion } from '../utils/version';
import {
  modeFromId,
  recordTuiModeActive,
  recordTuiModelInvocation,
  recordTuiSessionStarted,
  recordTuiTurnOutcome,
  recordTuiUserTurn,
  TuiToolCallObserver,
} from '../utils/tui-telemetry-observer';
import {
  BaseAcpClient,
  EXT_METHODS,
  buildStdioStreams,
  toolTelemetryStartFromEvent,
  toAgentProcess,
  type SessionResult,
} from './base';

/**
 * Map a V2 `prompt()` {@link acp.StopReason} to the catalog `result` enum for
 * `kiro_cli_user_turns`. The KAS observer's {@link resultFromStatus} keys on
 * KAS status strings, so V2 needs its own mapping off the ACP stop-reason
 * vocabulary (`end_turn | max_tokens | max_turn_requests | refusal |
 * cancelled`). A normal `end_turn` is a success; `cancelled` maps to cancelled;
 * `refusal` is a model-side failure; the token/turn limits are treated as
 * `_other_` (the turn did not fail per se, it hit a budget cap).
 */
function resultFromStopReason(
  stopReason: acp.StopReason | undefined
): 'success' | 'failed' | 'cancelled' | '_other_' {
  switch (stopReason) {
    case 'end_turn':
      return 'success';
    case 'cancelled':
      return 'cancelled';
    case 'refusal':
      return 'failed';
    case 'max_tokens':
    case 'max_turn_requests':
    default:
      return '_other_';
  }
}

/**
 * Map a V2 `prompt()` {@link acp.StopReason} into a status string the KAS
 * observer's {@link turnOutcomeReasonFromStatus} already understands, so the
 * `kiro_cli_turn_outcome_total` bucketing is shared across engines. Returns
 * undefined for `end_turn` (a success turn emits no outcome counter). The token
 * cap maps to `context_limit`; the turn-request cap and refusal map to
 * `model_error`; cancellation to `interrupted` (via "cancelled").
 */
function turnOutcomeStatusFromStopReason(
  stopReason: acp.StopReason | undefined
): string | undefined {
  switch (stopReason) {
    case 'end_turn':
      return undefined;
    case 'cancelled':
      return 'cancelled';
    case 'max_tokens':
      return 'context_limit';
    case 'max_turn_requests':
    case 'refusal':
      return 'model_error';
    default:
      return '_other_';
  }
}

/** Extract the current model `{id, name}` from the V2 Rust `models` field. */
function extractModel(
  models?: {
    currentModelId?: string;
    availableModels?: Array<{ modelId: string; name: string }>;
  } | null
): { id: string; name: string } | undefined {
  if (!models?.currentModelId || !models.availableModels) return undefined;
  const m = models.availableModels.find(
    (x) => x.modelId === models.currentModelId
  );
  return m ? { id: m.modelId, name: m.name } : undefined;
}

/** Extract the current agent from the V2 Rust `modes` field. */
function extractCurrentAgent(
  modes?: {
    currentModeId?: string;
    availableModes?: Array<{
      id: string;
      _meta?: Record<string, unknown> | null;
    }>;
  } | null
): { name: string; welcomeMessage?: string } | undefined {
  if (!modes?.currentModeId) return undefined;
  const currentMode = modes.availableModes?.find(
    (m) => m.id === modes.currentModeId
  );
  return {
    name: modes.currentModeId,
    welcomeMessage: currentMode?._meta?.welcomeMessage as string | undefined,
  };
}

// ─── Rust ACP client ─────────────────────────────────────────────────

export class RustAcpClient extends BaseAcpClient implements acp.Client {
  private connection: acp.ClientSideConnection;
  /**
   * Version reported in the ACP `clientInfo` handshake. Injectable so tests
   * can assert the forwarded version without re-importing the module to bust
   * a cached module-level constant. Defaults to the launcher-forwarded CLI
   * version (`getCliVersion()`), so production behavior is unchanged.
   */
  private readonly version: string;

  // V2 client-experience telemetry (engine=v2): same metric set as KAS minus
  // host-authoritative economics (tokens/cost/context_usage), which §H.6 leaves
  // to the Rust host to avoid double-counting.
  private readonly v2ToolCalls = new TuiToolCallObserver(undefined, 'v2');
  /** Dedup guard so kiro_cli_chat_session_started_total fires once per session id. */
  private readonly v2SessionStartedSessions = new Set<string>();
  /**
   * Best-effort current model id, captured from session results +
   * KasModelConfigUpdate events, written verbatim as the `model` attribute on V2
   * metrics. Undefined → emitted as the empty string.
   */
  private v2CurrentModelId?: string;
  /** Current TUI mode id, captured from session results + setMode. */
  private v2CurrentMode = 'interactive';

  constructor(
    agentPath: string,
    extraAcpArgs: string[] = [],
    version: string = getCliVersion()
  ) {
    const proc = spawn(agentPath, ['acp', ...extraAcpArgs], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
      // Unix: run in its own process group so close() can signal -pgid and
      // take down any MCP servers / subprocesses the agent spawned. Without
      // this a TUI crash or SIGTERM leaks the entire MCP tree as ppid=1
      // orphans.
      // Windows: detached allocates a visible console window (libuv maps it
      // to CREATE_NEW_PROCESS_GROUP). Use windowsHide instead. See P460297924.
      ...(process.platform !== 'win32'
        ? { detached: true }
        : { windowsHide: true }),
    });
    super(toAgentProcess(proc));
    this.version = version;
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    this.connection = new acp.ClientSideConnection(() => this, finalStream);
  }

  /** SDK >=0.16 no longer prepends '_' to ext methods; the Rust sacp backend expects it. */
  private ext(method: string): string {
    return method.startsWith('_') ? method : `_${method}`;
  }

  async initialize(): Promise<void> {
    const initResult = await this.connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
      clientInfo: { name: 'kiro-tui', version: this.version },
    });
    logger.debug(
      '[acp-client] ACP handshake done, protocolVersion:',
      initResult.protocolVersion
    );
  }

  async newSession(): Promise<SessionResult> {
    const r = await this.connection.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });
    this.sessionId = r.sessionId;
    logger.debug('ACP session created', { sessionId: this.sessionId });
    // Drop any tool-call state stranded by a prior session so it cannot
    // cross-match into this one (mirrors KasAcpClient's per-session reset).
    this.v2ToolCalls.reset();
    const currentModel = extractModel(r.models);
    const currentAgent = extractCurrentAgent(r.modes);
    this.captureV2SessionContext(currentModel, currentAgent);
    this.emitV2SessionStartedOnce(r.sessionId);
    return {
      sessionId: r.sessionId,
      currentModel,
      currentAgent,
    };
  }

  async loadSession(sessionId: string): Promise<SessionResult> {
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;
    const r = await this.connection
      .loadSession({ sessionId, cwd: process.cwd(), mcpServers: [] })
      .catch((err) => {
        this.sessionId = previousSessionId;
        throw err;
      });
    logger.debug('[acp-client] loadSession completed for session:', sessionId);
    // Drop any tool-call state stranded by the prior session (mirrors
    // KasAcpClient's per-session reset).
    this.v2ToolCalls.reset();
    const currentModel = extractModel(r.models);
    const currentAgent = extractCurrentAgent(r.modes);
    this.captureV2SessionContext(currentModel, currentAgent);
    this.emitV2SessionStartedOnce(sessionId);
    return {
      sessionId,
      currentModel,
      currentAgent,
    };
  }

  /**
   * Capture the model/mode a session opened with so later V2 turn/tool metrics
   * label correctly. Both are best-effort: a missing model → empty `model`
   * string; a missing mode keeps the prior value (default `interactive`).
   */
  private captureV2SessionContext(
    currentModel: { id: string; name: string } | undefined,
    currentAgent: { name: string } | undefined
  ): void {
    if (currentModel?.id) this.v2CurrentModelId = currentModel.id;
    if (currentAgent?.name) this.v2CurrentMode = currentAgent.name;
  }

  /** Emit V2 session-start metrics (session_started + mode_active) once per session id. */
  private emitV2SessionStartedOnce(sessionId: string): void {
    if (this.v2SessionStartedSessions.has(sessionId)) return;
    this.v2SessionStartedSessions.add(sessionId);
    const mode = modeFromId(this.v2CurrentMode);
    recordTuiSessionStarted({
      mode,
      version: this.version,
      engine: 'v2',
    });
    recordTuiModeActive({ mode, engine: 'v2' });
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId)
      throw new Error('cannot send prompt without an active session');
    if (this.connection.signal.aborted)
      throw new Error('Agent connection closed unexpectedly');

    const connectionClosed = new Promise<never>((_resolve, reject) => {
      if (this.connection.signal.aborted) {
        reject(new Error('Agent connection closed unexpectedly'));
        return;
      }
      this.connection.signal.addEventListener(
        'abort',
        () => reject(new Error('Agent connection closed unexpectedly')),
        { once: true }
      );
    });
    connectionClosed.catch(() => {});

    const startMs = performance.now();
    try {
      const response = await Promise.race([
        this.connection.prompt({ prompt: messages, sessionId: this.sessionId }),
        connectionClosed,
      ]);
      // V2 carries only a stopReason here (no modelId/tokens/duration), so we
      // measure wall-clock duration client-side and label model from the session.
      this.observeV2TurnCompletion(
        response?.stopReason,
        (performance.now() - startMs) / 1000
      );
    } finally {
      // Clear in-flight tool calls at turn end so a stranded entry (finish never
      // arrived) can't cross-match a same-id finish in a later turn.
      this.v2ToolCalls.reset();
    }
  }

  /**
   * Emit V2 turn metrics (user_turns + duration, model_invocations, turn_outcome),
   * engine='v2'. Economics are §H.6-omitted; is_subagent is always false (the
   * main prompt() turn is the only turn on this wire).
   */
  private observeV2TurnCompletion(
    stopReason: acp.StopReason | undefined,
    durationSeconds: number
  ): void {
    const model = this.v2CurrentModelId ?? '';
    const mode = modeFromId(this.v2CurrentMode);
    const isSubagent = false;
    recordTuiUserTurn({
      model,
      result: resultFromStopReason(stopReason),
      isSubagent,
      mode,
      chatConversationType: 'acp',
      durationSeconds:
        Number.isFinite(durationSeconds) && durationSeconds >= 0
          ? durationSeconds
          : undefined,
      engine: 'v2',
    });
    recordTuiModelInvocation({ model, engine: 'v2' });
    recordTuiTurnOutcome({
      status: turnOutcomeStatusFromStopReason(stopReason),
      model,
      mode,
      engine: 'v2',
    });
  }

  /**
   * V2 tool-call + context observation, driven off the shared converted event
   * (see {@link BaseAcpClient.observeTurnTelemetry}). Mirrors
   * KasAcpClient.observeV3ToolCall — origin is decided once at ToolCall time,
   * the finish emits `kiro_cli_tool_call_total` + `kiro_cli_tool_execution_duration_ms`
   * via the `engine='v2'` observer. Also opportunistically tracks the current
   * model id from KasModelConfigUpdate so the `model` label stays current after a
   * model swap. NOTE: V2 sub-agent delegations only emit if the parent
   * `orchestrate_subagent` ToolCall carries `_meta.kiro.pipeline`; if the V2
   * host does not stamp it, the delegation counter simply does not fire (we do
   * not fabricate it).
   */
  protected override observeTurnTelemetry(event: AgentStreamEvent): void {
    if (event.type === AgentEventType.KasModelConfigUpdate) {
      if (event.currentModelId) this.v2CurrentModelId = event.currentModelId;
      return;
    }
    if (event.type === AgentEventType.ToolCall) {
      this.v2ToolCalls.start(event.id, toolTelemetryStartFromEvent(event));
      return;
    }
    if (event.type !== AgentEventType.ToolCallFinished) return;
    this.v2ToolCalls.finish(event.id, {
      outcome:
        event.result?.status === 'error'
          ? 'error'
          : event.result?.status === 'cancelled'
            ? 'cancelled'
            : 'success',
      model: this.v2CurrentModelId ?? '',
    });
  }

  async cancel(): Promise<void> {
    if (!this.sessionId) return;
    try {
      await this.connection.cancel({ sessionId: this.sessionId });
    } catch (e) {
      logger.error('Failed to send cancel notification:', e);
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      return (await this.connection.extMethod(
        this.ext(EXT_METHODS.COMMANDS_EXECUTE),
        {
          sessionId: this.sessionId,
          command,
        }
      )) as unknown as CommandResult;
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Command failed',
      };
    }
  }

  async getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionId) return { options: [] };
    try {
      return (await this.connection.extMethod(
        this.ext(EXT_METHODS.COMMANDS_OPTIONS),
        {
          sessionId: this.sessionId,
          command: commandName.replace(/^\//, ''),
          partial,
        }
      )) as unknown as CommandOptionsResponse;
    } catch {
      return { options: [] };
    }
  }

  async setConfigOption(
    configId: 'mode' | 'model' | 'effortLevel',
    value: string
  ): Promise<void> {
    if (!this.sessionId) return;
    // V2 routes model/effort through the executeCommand/getCommandOptions
    // round-trip, so only the agent-mode config option flows through here.
    if (configId !== 'mode') {
      throw new Error(
        `setConfigOption('${configId}') is not supported by the V2 engine`
      );
    }
    await this.connection.setSessionMode({
      sessionId: this.sessionId,
      modeId: value,
    });
    // Keep the V2 telemetry mode in sync so later turn/mode metrics bucket
    // against the active mode rather than the one the session opened with.
    this.v2CurrentMode = value;
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    return (await this.connection.extMethod(this.ext('kiro.dev/session/list'), {
      cwd,
    })) as unknown as ListSessionsResponse;
  }

  async listSettings(): Promise<Record<string, unknown>> {
    return (await this.connection.extMethod(
      this.ext('kiro.dev/settings/list'),
      {}
    )) as unknown as Record<string, unknown>;
  }

  async setSetting(key: string, value: unknown): Promise<void> {
    await this.connection.extMethod(this.ext('kiro.dev/settings/set'), {
      key,
      value,
    });
  }

  async terminateSession(sessionId: string): Promise<void> {
    try {
      await this.connection.extMethod(this.ext('kiro.dev/session/terminate'), {
        sessionId,
      });
    } catch (err) {
      logger.warn('terminateSession failed (best-effort)', { sessionId, err });
    }
  }

  async spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    const result = await this.connection.extMethod(
      this.ext(EXT_METHODS.SESSION_SPAWN),
      {
        sessionId: this.sessionId,
        task,
        name,
      }
    );
    return {
      sessionId: (result as any).sessionId,
      name: (result as any).name ?? name ?? '',
    };
  }

  protected async extRequest<T = unknown>(
    method: string,
    params: Record<string, unknown>
  ): Promise<T> {
    return (await this.connection.extMethod(this.ext(method), params)) as T;
  }

  sendProcessHealthMetrics(payload: ProcessHealthSnapshot): void {
    this.connection
      .extNotification(this.ext('kiro.dev/telemetry/processHealth'), {
        ...payload,
        agentKind: 'v2',
      } as unknown as Record<string, unknown>)
      .catch(() => {});
  }

  sendModeChanged(payload: ModeChangedNotification): void {
    this.connection
      .extNotification(
        this.ext('kiro.dev/telemetry/modeChanged'),
        payload as unknown as Record<string, unknown>
      )
      .catch(() => {});
  }

  sendChatSlashCommandTelemetry(
    payload: ChatSlashCommandTelemetryPayload
  ): void {
    this.connection
      .extNotification(this.ext('kiro.dev/telemetry/chatSlashCommand'), {
        ...payload,
        sessionId: this.sessionId,
      } as unknown as Record<string, unknown>)
      .catch(() => {});
  }

  sendUiModeSessionStart(payload: UiModeSessionStartNotification): void {
    this.connection
      .extNotification(
        this.ext('kiro.dev/telemetry/uiModeSessionStart'),
        payload as unknown as Record<string, unknown>
      )
      .catch(() => {});
  }

  sendUiModeChanged(payload: UiModeChangedNotification): void {
    this.connection
      .extNotification(
        this.ext('kiro.dev/telemetry/uiModeChanged'),
        payload as unknown as Record<string, unknown>
      )
      .catch(() => {});
  }

  sendUiModeDefaultChanged(payload: UiModeDefaultChangedNotification): void {
    this.connection
      .extNotification(
        this.ext('kiro.dev/telemetry/uiModeDefaultChanged'),
        payload as unknown as Record<string, unknown>
      )
      .catch(() => {});
  }

  // ── acp.Client interface ──

  async requestPermission(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    return this.handlePermissionRequest(params);
  }

  async sessionUpdate(params: acp.SessionNotification): Promise<void> {
    this.handleSessionUpdate(params);
  }

  async writeTextFile?(
    _p: acp.WriteTextFileRequest
  ): Promise<acp.WriteTextFileResponse> {
    throw new Error('not implemented');
  }
  async readTextFile?(
    _p: acp.ReadTextFileRequest
  ): Promise<acp.ReadTextFileResponse> {
    throw new Error('not implemented');
  }
  async createTerminal?(
    _p: acp.CreateTerminalRequest
  ): Promise<acp.CreateTerminalResponse> {
    throw new Error('not implemented');
  }
  async terminalOutput?(
    _p: acp.TerminalOutputRequest
  ): Promise<acp.TerminalOutputResponse> {
    throw new Error('not implemented');
  }
  async releaseTerminal?(
    _p: acp.ReleaseTerminalRequest
  ): Promise<acp.ReleaseTerminalResponse | void> {
    throw new Error('not implemented');
  }
  async waitForTerminalExit?(
    _p: acp.WaitForTerminalExitRequest
  ): Promise<acp.WaitForTerminalExitResponse> {
    throw new Error('not implemented');
  }
  async killTerminal?(
    _p: acp.KillTerminalRequest
  ): Promise<acp.KillTerminalResponse | void> {
    throw new Error('not implemented');
  }
  async extMethod?(
    _method: string,
    _params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    throw new Error('not implemented');
  }

  async extNotification?(
    method: string,
    params: Record<string, unknown>
  ): Promise<void> {
    // ACP SDK >=0.16 passes raw method names; older versions strip the leading '_'.
    const key = method.startsWith('_') ? method.substring(1) : method;
    const handler = this.extNotificationHandlers[key];
    if (handler) handler(params);
  }
}
