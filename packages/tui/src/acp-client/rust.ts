import * as acp from '@agentclientprotocol/sdk';
import { logger } from '../utils/logger';
import { maybeWrapStreamWithRecorder } from '../acp-recorder';
import { spawn } from 'node:child_process';
import { createDecipheriv, randomBytes, randomUUID } from 'node:crypto';
import { rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ListSessionsResponse } from '../types/session-client';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import { TurnFailureReason as TurnFailureReasonValue } from '../types/generated/telemetry';
import type {
  ModeChangedNotification,
  UiModeChangedNotification,
  UiModeDefaultChangedNotification,
  UiModeSessionStartNotification,
} from '../types/generated/chat-cli';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from '../types/commands';
import { getCliVersion } from '../utils/version';
import {
  modeFromId,
  recordTuiSessionStarted,
  recordTuiSlashCommand,
  recordTuiUserTurn,
  type TurnFailureReason,
} from '../utils/tui-telemetry-observer';
import {
  BaseAcpClient,
  EXT_METHODS,
  buildStdioStreams,
  toAgentProcess,
  type SessionResult,
  type TelemetryIdentitySetter,
} from './base';

/**
 * Map a V2 `prompt()` {@link acp.StopReason} to the catalog `result` enum for
 * `kiro_cli_user_turns`. The KAS observer's {@link resultFromStatus} keys on
 * KAS status strings, so V2 needs its own mapping off the ACP stop-reason
 * vocabulary (`end_turn | max_tokens | max_turn_requests | refusal |
 * cancelled`). A normal `end_turn` is a success; `cancelled` maps to cancelled;
 * `refusal` represents a user/tool rejection in the V2 agent loop and maps to
 * cancellation; execution limits are failures.
 */
function resultFromStopReason(
  stopReason: acp.StopReason | undefined
): 'success' | 'failed' | 'cancelled' {
  switch (stopReason) {
    case 'end_turn':
      return 'success';
    case 'cancelled':
    case 'refusal':
      return 'cancelled';
    case 'max_tokens':
    case 'max_turn_requests':
    default:
      return 'failed';
  }
}

/**
 * Map a V2 stop reason to the bounded turn-failure vocabulary.
 */
function failureReasonFromStopReason(
  stopReason: acp.StopReason | undefined
): TurnFailureReason | undefined {
  switch (stopReason) {
    case 'end_turn':
    case 'cancelled':
    case 'refusal':
      return undefined;
    case 'max_tokens':
    case 'max_turn_requests':
      return 'execution_limit';
    default:
      return 'unknown';
  }
}

const TURN_FAILURE_REASONS = new Set<string>(
  Object.values(TurnFailureReasonValue)
);

function failureReasonFromPromptResponse(
  response: acp.PromptResponse
): TurnFailureReason | undefined {
  const reason = (
    response as acp.PromptResponse & {
      _meta?: { kiro?: { turnFailureReason?: unknown } };
    }
  )._meta?.kiro?.turnFailureReason;
  return typeof reason === 'string' && TURN_FAILURE_REASONS.has(reason)
    ? (reason as TurnFailureReason)
    : undefined;
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

const TELEMETRY_IDENTITY_METHOD = '_kiro.dev/telemetry/identityChanged';

function createTelemetryIdentityKeyFile(key: Buffer): string {
  const path = join(tmpdir(), `kiro-tui-telemetry-${randomUUID()}.key`);
  writeFileSync(path, key, { flag: 'wx', mode: 0o600 });
  return path;
}

export class RustAcpClient extends BaseAcpClient implements acp.Client {
  private connection: acp.ClientSideConnection;
  private readonly telemetryIdentityKey: Buffer;
  private readonly telemetryIdentityKeyFile: string;
  /**
   * Version reported in the ACP `clientInfo` handshake. Injectable so tests
   * can assert the forwarded version without re-importing the module to bust
   * a cached module-level constant. Defaults to the launcher-forwarded CLI
   * version (`getCliVersion()`), so production behavior is unchanged.
   */
  private readonly version: string;

  // V2 client-experience telemetry: same metric set as KAS minus
  // host-authoritative economics (tokens/cost/context_usage), which §H.6 leaves
  // to the Rust host to avoid double-counting.
  /** Dedup guard so kiro_cli_chat_session_started_total fires once per session id. */
  private readonly v2SessionStartedSessions = new Set<string>();
  /** Current TUI mode id, captured from session results + setMode. */
  private v2CurrentMode = 'interactive';
  private readonly initialTrustPosture: 'prompt_on_demand' | 'trust_all_tools';

  constructor(
    agentPath: string,
    extraAcpArgs: string[] = [],
    version: string = getCliVersion(),
    telemetryIdentityKey: Buffer = randomBytes(32),
    telemetryIdentitySetter?: TelemetryIdentitySetter
  ) {
    const telemetryIdentityKeyFile =
      createTelemetryIdentityKeyFile(telemetryIdentityKey);
    const backendEnv = { ...process.env };
    delete backendEnv['KIRO_USER_ID'];
    delete backendEnv['KIRO_TUI_TELEMETRY_KEY'];
    delete backendEnv['KIRO_TUI_TELEMETRY_KEY_FILE'];
    backendEnv['KIRO_TUI_TELEMETRY_KEY_FILE'] = telemetryIdentityKeyFile;
    let proc;
    try {
      proc = spawn(agentPath, ['acp', ...extraAcpArgs], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: backendEnv,
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
    } catch (err) {
      rmSync(telemetryIdentityKeyFile, { force: true });
      throw err;
    }
    super(toAgentProcess(proc), telemetryIdentitySetter);
    this.version = version;
    this.initialTrustPosture = extraAcpArgs.includes('--trust-all-tools')
      ? 'trust_all_tools'
      : 'prompt_on_demand';
    this.telemetryIdentityKey = telemetryIdentityKey;
    this.telemetryIdentityKeyFile = telemetryIdentityKeyFile;
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    this.connection = new acp.ClientSideConnection(() => this, finalStream);
  }

  private cleanupTelemetryIdentityKeyFile(): void {
    rmSync(this.telemetryIdentityKeyFile, { force: true });
  }

  override close(): void {
    this.cleanupTelemetryIdentityKeyFile();
    super.close();
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
    this.cleanupTelemetryIdentityKeyFile();
    this.sessionId = r.sessionId;
    logger.debug('ACP session created', { sessionId: this.sessionId });
    // Drop any tool-call state stranded by a prior session so it cannot
    // cross-match into this one (mirrors KasAcpClient's per-session reset).
    const currentModel = extractModel(r.models);
    const currentAgent = extractCurrentAgent(r.modes);
    this.captureV2SessionContext(currentAgent);
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
    this.cleanupTelemetryIdentityKeyFile();
    logger.debug('[acp-client] loadSession completed for session:', sessionId);
    // Drop any tool-call state stranded by the prior session (mirrors
    // KasAcpClient's per-session reset).
    const currentModel = extractModel(r.models);
    const currentAgent = extractCurrentAgent(r.modes);
    this.captureV2SessionContext(currentAgent);
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
    currentAgent: { name: string } | undefined
  ): void {
    if (currentAgent?.name) this.v2CurrentMode = currentAgent.name;
  }

  /** Emit the V2 chat-session counter once per session id. */
  private emitV2SessionStartedOnce(sessionId: string): void {
    if (this.v2SessionStartedSessions.has(sessionId)) return;
    this.v2SessionStartedSessions.add(sessionId);
    const mode = modeFromId(this.v2CurrentMode);
    recordTuiSessionStarted({
      mode,
      version: this.version,
      engine: 'v2',
      trustPosture: this.initialTrustPosture,
    });
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId)
      throw new Error('cannot send prompt without an active session');
    if (this.connection.signal.aborted) {
      this.observeV2RejectedTurn();
      throw new Error('Agent connection closed unexpectedly');
    }

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
    this.startFirstVisibleResponse({
      mode: this.v2CurrentMode,
      version: this.version,
      engine: 'v2',
    });
    try {
      let response: acp.PromptResponse;
      try {
        response = await Promise.race([
          this.connection.prompt({
            prompt: messages,
            sessionId: this.sessionId,
          }),
          connectionClosed,
        ]);
      } catch (err) {
        this.observeV2RejectedTurn();
        throw err;
      }
      this.observeV2TurnCompletion(
        response.stopReason,
        (performance.now() - startMs) / 1000,
        failureReasonFromPromptResponse(response)
      );
    } finally {
      this.cancelFirstVisibleResponse();
    }
  }

  private observeV2RejectedTurn(): void {
    recordTuiUserTurn({
      result: 'failed',
      isSubagent: false,
      mode: modeFromId(this.v2CurrentMode),
      version: this.version,
      failureReason: 'internal_error',
      engine: 'v2',
    });
  }

  /**
   * Emit the client-owned V2 top-level turn metrics.
   */
  private observeV2TurnCompletion(
    stopReason: acp.StopReason | undefined,
    durationSeconds: number,
    responseFailureReason?: TurnFailureReason
  ): void {
    const mode = modeFromId(this.v2CurrentMode);
    const isSubagent = false;
    const stopResult = resultFromStopReason(stopReason);
    const responseDefinesFailure =
      responseFailureReason !== undefined && stopResult !== 'cancelled';
    const failureReason = responseDefinesFailure
      ? responseFailureReason
      : failureReasonFromStopReason(stopReason);
    recordTuiUserTurn({
      result: responseDefinesFailure ? 'failed' : stopResult,
      isSubagent,
      mode,
      version: this.version,
      ...(failureReason === undefined ? {} : { failureReason }),
      durationSeconds:
        Number.isFinite(durationSeconds) && durationSeconds >= 0
          ? durationSeconds
          : undefined,
      engine: 'v2',
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
      const result = (await this.connection.extMethod(
        this.ext(EXT_METHODS.COMMANDS_EXECUTE),
        {
          sessionId: this.sessionId,
          command,
        }
      )) as unknown as CommandResult;
      return result;
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

  recordSlashCommandInvocation(command: string): void {
    recordTuiSlashCommand({
      command,
      version: this.version,
      engine: 'v2',
    });
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

  private decryptTelemetryIdentity(
    params: Record<string, unknown>
  ): string | undefined {
    const nonceValue = params['nonce'];
    const ciphertextValue = params['ciphertext'];
    if (typeof nonceValue !== 'string' || typeof ciphertextValue !== 'string')
      return undefined;
    try {
      const nonce = Buffer.from(nonceValue, 'base64url');
      const sealed = Buffer.from(ciphertextValue, 'base64url');
      if (nonce.length !== 12 || sealed.length < 16) return undefined;
      const ciphertext = sealed.subarray(0, -16);
      const tag = sealed.subarray(-16);
      const decipher = createDecipheriv(
        'aes-256-gcm',
        this.telemetryIdentityKey,
        nonce
      );
      decipher.setAAD(Buffer.from(TELEMETRY_IDENTITY_METHOD));
      decipher.setAuthTag(tag);
      return Buffer.concat([
        decipher.update(ciphertext),
        decipher.final(),
      ]).toString('utf8');
    } catch {
      return undefined;
    }
  }

  async extNotification?(
    method: string,
    params: Record<string, unknown>
  ): Promise<void> {
    // ACP SDK >=0.16 passes raw method names; older versions strip the leading '_'.
    const wireMethod = method.startsWith('_') ? method : `_${method}`;
    const key = wireMethod.substring(1);
    const handler = this.extNotificationHandlers[key];
    if (!handler) return;
    if (wireMethod === TELEMETRY_IDENTITY_METHOD) {
      const userId = this.decryptTelemetryIdentity(params);
      if (userId === undefined) return;
      await handler(userId.length === 0 ? { clear: true } : { userId });
      return;
    }
    await handler(params);
  }
}
