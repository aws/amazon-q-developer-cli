import * as acp from '@agentclientprotocol/sdk';
import { KiroClient, type Stream } from '@kiro/client';
import type {
  SpecInvokeRequest,
  SpecInvokeResponse,
  SpecResolveSessionRequest,
  SpecResolveSessionResponse,
  SourceProviderList,
  SourceProviderResourcePage,
  SourceProviderResourcesRequest,
  KiroSessionListItemMeta,
} from '@kiro/acp-type-covenant';
import { logger } from '../utils/logger';
import {
  getTelemetryIdentity,
  isTelemetryEnabled,
} from '../utils/telemetry-identity';
import { buildKasSettings } from '../utils/kas-settings';
import { webToolsGovernanceFromState } from '../utils/governance-state';
import { readCliSettings, updateCliSetting } from '../utils/cli-settings';
import { Settings } from '../constants/settings';
import { maybeWrapStreamWithRecorder } from '../acp-recorder';
import { createGetAccessTokenCapability } from '../auth/acp-auth-callback';
import { createCopyUrlToClipboardCapability } from '../capabilities/copy-url-to-clipboard';
import { createSecretStorageCapabilities } from '../capabilities/secret-storage';
import { spawn } from 'node:child_process';
import type {
  ChatSlashCommandTelemetryPayload,
  ListSessionsResponse,
  ExecutionTarget,
  KiroAgentCapabilities,
  SessionsChangedNotification,
  KasContextShowResponse,
  KasContextMutationResponse,
} from '../types/session-client';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import type {
  ModeChangedNotification,
  UiModeChangedNotification,
  UiModeDefaultChangedNotification,
  UiModeSessionStartNotification,
} from '../types/generated/chat-cli';
import {
  AgentEventType,
  type AgentStreamEvent,
  type HooksUpdateEvent,
  type KiroMeta,
  type McpServerSnapshotEvent,
} from '../types/agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
} from '../types/commands';
import type {
  KasPermissionRequest,
  KasSubagentRoutingEmitter,
  KasSubagentRoutingStore,
} from '../stores/kas-subagent-routing';
import { parseToolsDidChange } from '../utils/kas-tools';
import { extractRpcErrorMessage } from '../utils/error-handling';
import { openUrlInBrowser } from '../utils/browser';
import { getCliVersion } from '../utils/version';
import { getKasCommands } from '../kas-commands';
import { features } from '../features';
import { readClipboardImage } from '../utils/clipboard-image';
import {
  modeFromId,
  recordTuiContextUsage,
  recordTuiModeActive,
  recordTuiCloudSession,
  recordTuiCloudSessionReady,
  recordTuiModelInvocation,
  recordTuiSessionStarted,
  recordTuiTokensConsumed,
  recordTuiTurnOutcome,
  recordTuiUserTurn,
  resultFromStatus,
  TuiToolCallObserver,
} from '../utils/tui-telemetry-observer';
import {
  parseModelsFromConfigOptions,
  parseAgentsFromConfigOptions,
  parseEffortsFromConfigOptions,
  deriveCurrentSelections,
  currentModeWelcomeMessage,
  toKasModeId,
  fromKasModeId,
  resolveInitialModel,
  type KasConfigOrigin,
} from '../utils/kas-config-options';
import { isKasShellCapability } from '../utils/shell-trust-options.js';
import {
  BaseAcpClient,
  buildStdioStreams,
  extractKasSessionInfoMeta,
  extractKiroMetaFromUpdate,
  normalizeKasTurnCompletion,
  stripMcpTitlePrefix,
  toolTelemetryStartFromEvent,
  toAgentProcess,
  type AcpSessionUpdate,
  type AgentProcess,
  type SessionResult,
} from './base';

// User-agent tokens attached to the KAS ACP clientInfo._meta. KAS appends these
// to the user agent it sends to the backend. `app/AmazonQ-For-CLI` is required
// for backend ALB routing + ClientMetadataUtil parsing; KAS derives the
// KiroCLI/<version>, KAS/, os/, and md/appVersion- segments itself.
const KAS_CLIENT_INFO_META = {
  userAgentTags: ['app/AmazonQ-For-CLI'],
} as const;

/**
 * Validate the opaque `agentCapabilities._meta.kiro` blob from the KAS
 * `initialize` handshake into a typed {@link KiroAgentCapabilities}. Every
 * field is optional and defensively checked: a malformed or absent blob yields
 * capabilities with every field left undefined, so the client treats remote
 * features as unadvertised and degrades to local (the dark-ship /
 * backward-compat safety gate).
 */
function parseKiroAgentCapabilities(raw: unknown): KiroAgentCapabilities {
  if (!raw || typeof raw !== 'object') return {};
  const r = raw as Record<string, unknown>;
  const stringArray = (v: unknown): string[] | undefined =>
    Array.isArray(v) && v.every((x) => typeof x === 'string')
      ? (v as string[])
      : undefined;
  return {
    executionTargets: stringArray(r.executionTargets),
    sessionSources: stringArray(r.sessionSources),
    sessionListScopes: stringArray(r.sessionListScopes),
    extensionMethods: stringArray(r.extensionMethods),
    sourceProviders:
      typeof r.sourceProviders === 'boolean' ? r.sourceProviders : undefined,
  };
}

function getKasVersion(kasServerPath: string): string {
  try {
    const { readFileSync } = require('node:fs');
    const { join, dirname } = require('node:path');
    const pkg = join(dirname(kasServerPath), '..', '..', 'package.json');
    return JSON.parse(readFileSync(pkg, 'utf8')).version ?? 'unknown';
  } catch {
    return 'unknown';
  }
}

/**
 * Read `meta.kiro` from a stream event, narrowing the discriminated union
 * to event variants that carry metadata (Content, ToolCall, ToolCallFinished).
 */
function extractKiroMetaFromEvent(
  event: AgentStreamEvent
): KiroMeta | undefined {
  return 'meta' in event && event.meta ? event.meta.kiro : undefined;
}

/**
 * KAS policy capability identifiers, emitted on
 * `_meta.kiro.consent.capability`. Source of truth: KAS
 * `packages/kiro-agent/src/policy/capabilities.ts` (`BUILTIN`).
 */
const KAS_CAPABILITIES = {
  /** Sub-agent spawn (e.g. `invoke_sub_agent`) — parent-session decision. */
  SUBAGENT: 'subagent',
} as const;

function kasConsentRecordFromRequest(
  request: any
): Record<string, unknown> | undefined {
  const consent =
    request?._meta?.kiro?.consent ?? request?.toolCall?._meta?.kiro?.consent;
  return consent && typeof consent === 'object' && !Array.isArray(consent)
    ? (consent as Record<string, unknown>)
    : undefined;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function kasPermissionMeta(params: any): KiroMeta | undefined {
  return params?._meta?.kiro ?? params?.toolCall?._meta?.kiro;
}

function inferKasShellPermissionToolCall(request: any): {
  title?: string;
  rawInput?: Record<string, unknown>;
} {
  const consent = kasConsentRecordFromRequest(request);
  if (!isKasShellCapability(stringValue(consent?.capability))) {
    return {};
  }
  const command =
    stringValue(consent?.resource) ?? stringValue(consent?.triggeringResource);
  return {
    title: 'run_command',
    ...(command ? { rawInput: { command } } : {}),
  };
}

/**
 * `AgentProcess` implementation for test mode where no real subprocess
 * exists. stdin/stdout/stderr are real in-memory PassThrough streams so
 * `BaseAcpClient`'s stdio existence check passes and `pipeStderr` has a
 * stream to consume (it reads nothing, since nothing writes). `onExit`
 * never fires and registers no underlying listener, so repeated
 * subscriptions cannot accumulate.
 */
export function createNullAgentProcess(): AgentProcess {
  const { PassThrough } = require('node:stream');
  return {
    stdin: new PassThrough(),
    stdout: new PassThrough(),
    stderr: new PassThrough(),
    kill: () => true,
    onExit: () => () => {},
  };
}

// ─── KAS ACP client ──────────────────────────────────────────────────

export interface KasAcpClientOptions {
  kasSubagentRoutingStore: KasSubagentRoutingStore;
  spawnProcess?: typeof spawn;
  stream?: Stream;
  initialAgent?: string;
  initialModel?: string;
  version?: string;
  executionTarget?: ExecutionTarget;
  repos?: string[];
}

export class KasAcpClient extends BaseAcpClient {
  private kiroClient: KiroClient;
  private readonly kasSubagentRoutingStore: KasSubagentRoutingStore;
  private readonly kasSubagentRoutingEmitter: KasSubagentRoutingEmitter = {
    emitMain: (event) => this.broadcastStreamEvent(event),
    emitMultiSession: (sessionId, event) =>
      this.broadcastMultiSession(sessionId, event),
    emitSession: (event) => this.broadcastSessionEvent(event),
    emitSubagentList: (subagents, pendingStages) =>
      this.broadcastSubagentList(subagents, pendingStages),
  };
  private pendingOAuthServerNames: Set<string> = new Set();
  private chatSessionStartedSessions = new Set<string>();
  /** Correlates V3 ToolCall → ToolCallFinished into tool telemetry (KAS-only). */
  private readonly v3ToolCalls = new TuiToolCallObserver();

  /**
   * Snapshots of the current model id and agent (KAS "mode") id, kept solely
   * to tag outgoing KAS telemetry (`kas-chat-session-started`,
   * `kas-turn-completion`). The client emits telemetry synchronously from
   * session notifications and cannot read the store, so it records the last
   * derived selection here. These are telemetry tags only, not a cache of
   * menu state — the store owns the available options and current selection.
   */
  private telemetryCurrentModelId?: string;
  private telemetryCurrentModeId?: string;

  /**
   * Initial agent name (KAS "mode") to apply on the next `newSession`.
   * Sourced from the TUI's `--agent` CLI flag.  Mirrors V2's
   * `set_next_agent_name` semantics: only applied to brand-new sessions;
   * `loadSession` keeps the persisted agent.
   */
  private readonly initialAgent?: string;

  /**
   * Explicit `--model` CLI flag value to apply on `newSession`, if any.
   * When absent, `newSession` lazily reads the saved `chat.defaultModel` from
   * cli.json so a sticky default written mid-run is honored by later in-process
   * sessions. An explicit flag always takes precedence over the saved default.
   */
  private readonly initialModel?: string;

  /**
   * Execution target for the first `newSession`, from the `--cloud` CLI flag.
   * `{ kind: 'cloud-sandbox' }` when `--cloud` was passed, else undefined
   * (treated as local). Sent as `_meta.kiro.executionTarget` on `session/new`,
   * but only when KAS advertised the kind on the `initialize` handshake (see
   * `isExecutionTargetSupported`); otherwise the session degrades to local.
   * Omitting `--cloud` is byte-identical to today's behavior.
   */
  private readonly executionTarget?: ExecutionTarget;

  /**
   * Whether the active session was actually placed on a cloud sandbox — i.e. a
   * `cloud-sandbox` executionTarget was advertised by KAS AND sent on
   * `session/new`. Set in `newSession` from the SENT meta, so it is `false`
   * when `--cloud` degraded to local (cap not advertised) — cloud-only UI then
   * never appears against a local session. Dark-safe: stays
   * false on every released build (no cloud-sandbox cap).
   */
  private startedCloudSession = false;

  /**
   * Advisory warnings from the last `session/new` (`_meta.kiro.warnings`),
   * e.g. a requested repository dropped because no connected provider owns
   * it. Empty when the session bound cleanly; cleared on session load.
   */
  sessionNewWarnings: string[] = [];

  /**
   * Cloud provisioning telemetry: start time is the latency baseline; the two
   * emitted-flags make each outcome fire at most once. Inert without a cloud
   * session (dark-safe).
   */
  private cloudSessionStartMs?: number;
  private cloudReadyEmitted = false;
  private cloudProvisionFailedEmitted = false;

  /**
   * Repository selector(s) to bind on `session/new` for a cloud session, from
   * the `--repo` flag. Sent as `_meta.kiro.repositories` when a cloud-sandbox
   * placement is advertised; otherwise inert. With no `--repo` this is undefined
   * and the create starts an empty sandbox.
   */
  private readonly repos?: string[];

  /**
   * Kiro-namespaced capabilities advertised by KAS on the `initialize`
   * handshake (`agentCapabilities._meta.kiro`). Empty until `initialize()`
   * populates it; an older KAS leaves it empty and the client degrades
   * gracefully (never requests an unadvertised placement/source/scope/method).
   */
  private kiroCapabilities: KiroAgentCapabilities = {};

  /**
   * Version reported in the KAS `clientInfo` handshake and the
   * `KIRO_CUSTOM_USER_AGENT` passed to the KAS subprocess. Injectable for
   * tests; defaults to the launcher-forwarded CLI version (`getCliVersion()`)
   * so production behavior is unchanged.
   */
  private readonly version: string;

  /**
   * Construct a KAS ACP client.
   *
   * Without `options.stream`, spawn the KAS subprocess and wire its stdio as
   * the ACP `Stream`. The app-store routing actions are required in both
   * production and injected-stream paths.
   *
   * With `options.stream`: skip the subprocess spawn and use the provided
   * stream (for `acp_integ_tests/`'s mock transport). The client reports
   * `kill()`/`onExit()` through a no-op null agent process internally so
   * `BaseAcpClient` has a uniform interface to work against.
   *
   * With `options.initialAgent`: apply the given agent name as the KAS
   * `mode` config option on the first `newSession`.  Takes precedence
   * over the legacy `KIRO_MODE` env var (which is the propagation
   * channel for the KAS-only Rust mode flag).
   *
   * `agentProcess` is intentionally not a public option - mock callers
   * never need to inject a different one, and accepting it without a
   * stream would silently ignore it.
   */
  constructor(options: KasAcpClientOptions) {
    const kasSubagentRoutingStore = options.kasSubagentRoutingStore;
    if (options.stream) {
      super(createNullAgentProcess());
      this.kasSubagentRoutingStore = kasSubagentRoutingStore;
      this.initialAgent = options.initialAgent;
      this.initialModel = options.initialModel;
      this.version = options.version ?? getCliVersion();
      this.executionTarget = options.executionTarget;
      this.repos = options.repos;
      const finalStream = maybeWrapStreamWithRecorder(options.stream);
      this.kiroClient = new KiroClient({
        stream: finalStream,
        clientInfo: {
          name: 'kiro-cli',
          version: this.version,
          _meta: KAS_CLIENT_INFO_META,
        },
        capabilities: [createGetAccessTokenCapability()],
      });
      return;
    }

    // Resolve KAS server: env var override > installed npm package
    let kasServerPath = process.env.KIRO_KAS_SERVER_PATH;
    if (!kasServerPath) {
      // Walk up from this file to find node_modules/@kiro/agent
      const { existsSync } = require('node:fs');
      const { join, dirname } = require('node:path');
      const serverFile = 'node_modules/@kiro/agent/dist/server/acp-server.js';
      let dir = __dirname;
      for (let i = 0; i < 10; i++) {
        const candidate = join(dir, serverFile);
        if (existsSync(candidate)) {
          kasServerPath = candidate;
          break;
        }
        const parent = dirname(dir);
        if (parent === dir) break;
        dir = parent;
      }
      if (!kasServerPath) {
        throw new Error(
          'KAS agent not found. Install @kiro/agent or set KIRO_KAS_SERVER_PATH.'
        );
      }
    }
    const nodeBin = process.env.KIRO_KAS_NODE_PATH || 'node';
    logger.info(`[acp-client] Spawning KAS agent: ${nodeBin} ${kasServerPath}`);

    // Resolved before `super()` because the user-agent below is baked into
    // the subprocess env at spawn time, which precedes the `super()` call
    // that unblocks `this` access. Stored on the instance afterwards.
    const version = options.version ?? getCliVersion();

    const proc = (options.spawnProcess ?? spawn)(
      nodeBin,
      [
        '--experimental-wasm-modules',
        kasServerPath,
        '--transport=stdio',
        // Host-mediated OIDC refresh. KAS calls back to this client over
        // ACP via `_kiro/auth/getAccessToken` (handled by the capability
        // registered on `KiroClient` below). The refresh token stays in
        // chat-cli's SQLite store; KAS only ever sees access tokens.
        '--auth=acp-callback',
      ],
      {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: {
          ...process.env,
          NODE_CHANNEL_FD: undefined,
          NODE_CHANNEL_SERIALIZATION_MODE: undefined,
          KIRO_CUSTOM_USER_AGENT: `KiroCLI/${version} KAS/${getKasVersion(kasServerPath)} os/${process.platform} md/appVersion-${version} app/AmazonQ-For-CLI`,
        },
        // See RustAcpClient — Unix: detached so close() can kill -pgid and
        // reap MCP children together with KAS instead of leaking them as
        // orphans. Windows: windowsHide suppresses console window. P460297924.
        ...(process.platform !== 'win32'
          ? { detached: true }
          : { windowsHide: true }),
      }
    );
    super(toAgentProcess(proc));
    this.kasSubagentRoutingStore = kasSubagentRoutingStore;
    this.initialAgent = options.initialAgent;
    this.initialModel = options.initialModel;
    this.version = version;
    this.executionTarget = options.executionTarget;
    this.repos = options.repos;
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    const kasSettings = buildKasSettings();
    this.kiroClient = new KiroClient({
      stream: finalStream,
      clientInfo: {
        name: 'kiro-cli',
        version: this.version,
        _meta: KAS_CLIENT_INFO_META,
      },
      capabilities: [
        createGetAccessTokenCapability(),
        createCopyUrlToClipboardCapability(),
        ...createSecretStorageCapabilities(),
      ],
      clientMeta: {
        telemetryEnabled: isTelemetryEnabled(),
        ...(isTelemetryEnabled() && { telemetry: getTelemetryIdentity() }),
        knowledge: true,
        hooks: { enabled: true, v2: true },
        requirementsAnalysis: true,
        // ICECAP infra-safety capability. Gated to the internal cohort by the
        // Rust launcher, which exports KIRO_INFRA_SAFETY_ROLLOUT_ENABLED from the
        // Feature::InfraSafety rollout decision. Advertised only when enabled, in
        // lockstep with the infraSafety* settings in buildKasSettings().
        ...(process.env.KIRO_INFRA_SAFETY_ROLLOUT_ENABLED === '1' && {
          infrastructureSafety: true,
        }),
        ...(kasSettings && { settings: kasSettings }),
      },
    });
  }

  private sessionDisposables: Array<{ dispose: () => void }> = [];
  private retired = false;

  private assertActive(operation: string): void {
    if (this.retired) {
      throw new Error(`KAS client closed during ${operation}`);
    }
  }

  /**
   * Emit normalized model / agent / effort updates parsed from a KAS
   * `configOptions` payload (session/new, session/load,
   * session/set_config_option responses, and `config_option_update`
   * notifications). The TUI store, not this client, retains the parsed
   * lists — this method only converts the wire payload into structured
   * stream events, mirroring how skills/prompts are handled.
   *
   * Model and effort are emitted together because effort is a per-model
   * option: switching models can add or remove the effort levels, so an
   * absent effort entry alongside a present model clears the effort chip.
   * Agents are emitted independently (a payload may omit the mode select).
   */
  private emitConfigOptions(
    configOptions: unknown,
    origin: KasConfigOrigin,
    opts?: { emitCurrentAgent?: boolean }
  ): void {
    const models = parseModelsFromConfigOptions(configOptions);
    if (models) {
      this.telemetryCurrentModelId = models.currentModelId;
      const efforts = parseEffortsFromConfigOptions(configOptions);
      this.broadcastStreamEvent({
        type: AgentEventType.KasModelConfigUpdate,
        models: models.models,
        currentModelId: models.currentModelId,
        efforts: efforts?.efforts ?? [],
        currentLevel: efforts?.currentLevel ?? null,
        origin,
      });
    }
    const agents = parseAgentsFromConfigOptions(configOptions);
    if (agents) {
      if (agents.currentAgentId) {
        this.telemetryCurrentModeId = toKasModeId(agents.currentAgentId);
      }
      this.broadcastStreamEvent({
        type: AgentEventType.KasAgentsUpdate,
        agents: agents.agents,
      });
      // Mid-session mode changes (a client `/agent` swap or a KAS-initiated
      // `config_option_update`) carry the new selection here; new/load route
      // the current agent through the session result instead. The welcome is
      // resolved from the raw current-mode option so a hidden current agent
      // still surfaces one. Emitted unconditionally — the store fires the
      // welcome banner only on an actual agent change (see `setCurrentAgent`),
      // so re-asserting the same agent is a no-op there.
      if (opts?.emitCurrentAgent && agents.currentAgentId) {
        this.broadcastStreamEvent({
          type: AgentEventType.AgentSwitched,
          agentName: agents.currentAgentId,
          welcomeMessage: currentModeWelcomeMessage(configOptions),
        });
      }
    }
  }

  /** Disposable for the hooks notification subscription. */
  private hooksNotificationDisposable: { dispose: () => void } | null = null;
  /** Disposable for the tools notification subscription. */
  private toolsNotificationDisposable: { dispose: () => void } | null = null;

  private wireSessionListeners(sessionId: string): void {
    this.sessionDisposables.forEach((d) => d.dispose());
    // New/loaded session: drop any per-turn tool-call state from the previous
    // session so a stale start time can't match a same-id finish, and so
    // unmatched entries (cancelled turns) don't accumulate across sessions.
    this.v3ToolCalls.reset();
    // Drop subtask correlation state from the previous session so a stale
    // subtaskId can't misroute a new session's tool approval to a crew panel
    // that no longer exists. loadSession replays history AFTER this runs, so
    // any still-active stages get re-registered before their events arrive.
    this.kasSubagentRoutingStore.resetKasSubagentRouting();
    this.kasSteerBuffer.clear();
    this.sessionDisposables = [
      this.kiroClient.onSessionUpdate(sessionId, async (notification) => {
        const update = notification.update;
        // Keep the cached current mode in sync for /agent and agent-display
        // purposes.  ACP doesn't (yet) ship an available_modes_update, so
        // availableModes is refreshed only on session/new and session/load.
        //
        // We also broadcast an AgentSwitched stream event so the app store
        // updates its currentAgent / previousAgentName / welcomeMessage.
        // Without this broadcast, agent-initiated mode changes (e.g. a
        // spec-mode workflow handoff) silently update the cache but leave
        // the header chip and welcome banner stale.
        // KAS re-asserts the current mode via `current_mode_update`
        // (carrying only the id). Convert it to an `AgentSwitched` stream
        // event so the store updates currentAgent + welcome banner. The
        // welcome text is resolved store-side from `kasAvailableAgents`, so
        // the event carries only the id. Emitted unconditionally; the store
        // fires the welcome banner only on an actual agent change (see
        // `setCurrentAgent`), so KAS re-asserting the same mode (notably at
        // session start, where the session result already set the agent) is a
        // no-op there.
        if (update.sessionUpdate === 'current_mode_update') {
          const rawModeId = (update as { currentModeId: string }).currentModeId;
          this.telemetryCurrentModeId = rawModeId;
          const newModeId = fromKasModeId(rawModeId);
          if (newModeId) {
            this.broadcastStreamEvent({
              type: AgentEventType.AgentSwitched,
              agentName: newModeId,
            });
          }
        }
        // KAS pushes `config_option_update` when it autonomously changes a
        // config option (model fallback after rate limits, late model
        // enumeration once auth completes, or mirroring a client-initiated
        // change). Re-emit the normalized model/agent/effort events so the
        // store self-heals; the client retains no copy.
        if (
          (update as { sessionUpdate?: string }).sessionUpdate ===
          'config_option_update'
        ) {
          this.emitConfigOptions(
            (update as { configOptions?: unknown }).configOptions,
            'serverPush',
            { emitCurrentAgent: true }
          );
        }
        this.forwardKasTurnCompletionTelemetry(sessionId, update);
        // NOTE: `sessionId` here is the per-listener KAS session, which equals
        // this.sessionId for the main agent — so the converter's stamp guard
        // (notifSessionId !== this.sessionId) is a no-op on the KAS main path.
        // KAS discriminates subagent stages via meta.agentSubtaskId, extracted
        // from the RETURNED event below — NOT via a per-stage notification
        // sessionId. So a Failed-tool synthesized broadcast for a denied KAS
        // *stage* tool is not stamped by this thread-through; that is a separate,
        // narrower defect tracked apart from the Rust-engine fix. Threaded here
        // for signature consistency and to correctly stamp if a genuine
        // subagent-session listener is ever wired.
        const event = this.convertAcpUpdateToEvent(update, sessionId);
        const meta = event ? extractKiroMetaFromEvent(event) : undefined;

        if (!event) return;

        // A content-policy refusal arrives as a message chunk tagged with
        // _meta.kiro.refusal; surface it as ModelRefusal and drop the inline text.
        if (meta?.refusal) {
          this.broadcastStreamEvent({
            type: AgentEventType.ModelRefusal,
            stopReason: 'CONTENT_FILTERED',
            category: meta.refusal.category,
            explanation: meta.refusal.explanation,
            recommendedModel: meta.refusal.recommendedModel,
          });
          return;
        }
        this.kasSubagentRoutingStore.rememberKasToolCall(event);

        // Intercept pipeline metadata → emit subagent list update
        if (meta?.pipeline) {
          const pipelineToolCallId =
            event.type === AgentEventType.ToolCall ||
            event.type === AgentEventType.ToolCallFinished
              ? event.id
              : undefined;
          this.kasSubagentRoutingStore.handlePipelineStateUpdate(
            meta.pipeline,
            pipelineToolCallId,
            this.kasSubagentRoutingEmitter
          );
        }

        const routedToSubtask =
          this.kasSubagentRoutingStore.routeKasSubtaskEvent(
            event,
            meta,
            this.kasSubagentRoutingEmitter,
            this.sessionId
          );
        this.kasSubagentRoutingStore.forgetFinishedKasToolCallSnapshot(event);
        if (routedToSubtask) {
          return;
        }

        // V3 tool telemetry — only for main-session tool calls. This sits
        // AFTER the agentSubtaskId early-return above so sub-agent tool calls
        // (which are KAS-internal) never inflate kiro_cli_tool_call_total, which has no
        // is_subagent dimension to filter them. Mirrors how
        // forwardKasTurnCompletionTelemetry treats sub-agent turns.
        this.observeV3ToolCall(event);

        this.broadcastStreamEvent(event);
      }),
      this.kiroClient.onPermissionRequest(sessionId, async (request) => {
        return this.handleKasPermissionRequest(request, sessionId);
      }),
    ];
  }

  /**
   * Translate ToolCall / ToolCallFinished events into V3 tool telemetry
   * (tool_call_total + execution-duration histogram). KAS-only; never throws.
   */
  private observeV3ToolCall(event: AgentStreamEvent | null): void {
    if (event?.type === AgentEventType.ToolCall) {
      this.v3ToolCalls.start(event.id, toolTelemetryStartFromEvent(event));
      return;
    }
    if (event?.type !== AgentEventType.ToolCallFinished) return;

    this.v3ToolCalls.finish(event.id, {
      outcome: event.result?.status === 'error' ? 'error' : 'success',
      model: this.telemetryCurrentModelId ?? '',
    });
  }

  protected override handleExtSessionUpdate(
    params: Record<string, unknown>
  ): void {
    const update = params.update as Record<string, unknown> | undefined;
    if (update?.sessionUpdate === 'tool_call_chunk') {
      const kiroMeta = extractKiroMetaFromUpdate(update as AcpSessionUpdate);
      if (kiroMeta?.agentSubtaskId) {
        const chunk = update as {
          toolCallId: string;
          title: string;
          kind: string;
        };
        const event: AgentStreamEvent = {
          type: AgentEventType.ToolCall,
          id: chunk.toolCallId,
          name: stripMcpTitlePrefix(chunk.title) || chunk.title,
          kind: chunk.kind,
          args: {},
          sessionId: kiroMeta.agentSubtaskId,
          meta: { kiro: kiroMeta },
        };
        this.kasSubagentRoutingStore.rememberKasToolCall(event);
        this.kasSubagentRoutingStore.recordKasChunkToolCall(
          event.id,
          kiroMeta.agentSubtaskId
        );
        this.broadcastMultiSession(kiroMeta.agentSubtaskId, event);
        return;
      }
    }
    super.handleExtSessionUpdate(params);
  }

  protected override broadcastSynthesizedFailedToolCall(
    event: AgentStreamEvent
  ): void {
    this.kasSubagentRoutingStore.rememberKasToolCall(event);
    const meta = extractKiroMetaFromEvent(event);
    const routedToSubtask = this.kasSubagentRoutingStore.routeKasSubtaskEvent(
      event,
      meta,
      this.kasSubagentRoutingEmitter,
      this.sessionId
    );
    if (routedToSubtask) return;
    this.observeV3ToolCall(event);
    this.broadcastStreamEvent(event);
  }

  private handleKasPermissionRequest(
    request: KasPermissionRequest,
    originSessionId?: string
  ): Promise<acp.RequestPermissionResponse> {
    // KAS sends toolCallId at top level; normalize to ACP format and enrich with stage correlation
    const toolCallId = request.toolCallId || request.toolCall?.toolCallId || '';
    const kiroMeta = kasPermissionMeta(request);
    const consent = (
      kiroMeta as { consent?: { capability?: string } } | undefined
    )?.consent;
    const isSubagentSpawn = consent?.capability === KAS_CAPABILITIES.SUBAGENT;
    const shellPermission = inferKasShellPermissionToolCall(request);
    const enriched = this.kasSubagentRoutingStore.prepareKasPermissionRequest({
      request,
      toolCallId,
      metadataSubtaskId: stringValue(kiroMeta?.agentSubtaskId),
      isSubagentSpawn,
      fallbackTitle: shellPermission.title,
      fallbackRawInput: shellPermission.rawInput,
      originSessionId,
    });
    return this.handlePermissionRequest(enriched);
  }

  /**
   * Returns a rejection-promise that fires if the KAS agent process exits,
   * together with an `unsubscribe` to detach the listener. Callers MUST
   * invoke `unsubscribe()` in a `finally` so listeners never accumulate
   * across repeated prompts (and so mock-mode prompts are a strict no-op
   * on the `onExit` path, since the null agent process never emits).
   */
  private processExitPromise(): {
    promise: Promise<never>;
    unsubscribe: () => void;
  } {
    let unsubscribe = () => {};
    const promise = new Promise<never>((_resolve, reject) => {
      unsubscribe = this.agentProcess.onExit((code) => {
        reject(
          new Error(`KAS agent process exited unexpectedly (code ${code})`)
        );
      });
    });
    return { promise, unsubscribe };
  }

  async initialize(): Promise<void> {
    const initResult = await this.kiroClient.initialize();

    // Capture the Kiro-namespaced KAS capabilities advertised on the handshake
    // (agentCapabilities._meta.kiro, KAS ACP doc §4). `_meta` is the ACP
    // extensibility slot (its values are typed `unknown`), so
    // parseKiroAgentCapabilities validates every field. Absent (older KAS) ->
    // empty caps -> remote features degrade to local (the dark-ship /
    // backward-compat safety gate).
    this.kiroCapabilities = parseKiroAgentCapabilities(
      initResult.agentCapabilities?._meta?.kiro
    );
    logger.debug(
      '[acp-client] Kiro agent capabilities:',
      this.kiroCapabilities
    );

    // Subscribe to hooks registry changes. The agent pushes this
    // notification whenever hooks are loaded, reloaded, or the file watcher
    // detects a change. We broadcast a HooksUpdate event so the store-owned
    // snapshot and any open panel stay current.
    this.hooksNotificationDisposable = this.kiroClient.onExtNotification(
      '_kiro/hooks/didChange',
      (params: Record<string, unknown>) => {
        const rawHooks = Array.isArray(params.hooks) ? params.hooks : [];
        const hooks = this.projectHooks(rawHooks);
        this.broadcastStreamEvent({
          type: AgentEventType.HooksUpdate,
          hooks,
        });
      }
    );

    // Subscribe to session tool-listing changes. KAS pushes the full current
    // tag set (builtin category tags + per-tool MCP tags) on session
    // new/load and whenever the resolved tool set changes (MCP connect/reset,
    // powers activation, /agent swaps). We broadcast a ToolsUpdate event so
    // the store-owned /tools snapshot reflects the latest set.
    this.toolsNotificationDisposable = this.kiroClient.onExtNotification(
      '_kiro/tools/didChange',
      (params: Record<string, unknown>) => {
        const sessionId = params.sessionId as string | undefined;
        if (sessionId && this.sessionId && sessionId !== this.sessionId) {
          return;
        }
        const tools = parseToolsDidChange(params);
        this.broadcastStreamEvent({
          type: AgentEventType.ToolsUpdate,
          tools,
        });
      }
    );

    // Register ext notification handlers
    this.kiroClient.onExtNotification('_kiro/mcp/status', (params) => {
      this.handleMcpStatusNotification(params);
    });

    // Route KAS _kiro/* notifications to the same handlers used by the Rust backend path.
    this.kiroClient.onExtNotification(
      '_kiro/customAgent/not_found',
      (params) => {
        // Normalize the fallback id (e.g. KAS `vibe` -> `default`) before it
        // reaches the shared handler, so the "using <agent>" message shows the
        // canonical id. `requestedAgent` stays raw — it echoes back the user's
        // literal chat.defaultAgent value.
        const rawFallback = params.fallbackAgent as string | undefined;
        this.handleAgentNotFound({
          ...params,
          ...(rawFallback ? { fallbackAgent: fromKasModeId(rawFallback) } : {}),
        });
        // KAS doesn't send current_mode_update after fallback, so emit the
        // AgentSwitched here. Emitted unconditionally; the store updates
        // currentAgent + prompt bar and fires the welcome only on an actual
        // change (welcome resolved store-side).
        const fallback = params.fallbackAgent as string | undefined;
        if (fallback) {
          const newModeId = fromKasModeId(fallback);
          this.broadcastStreamEvent({
            type: AgentEventType.AgentSwitched,
            agentName: newModeId,
          });
        }
      }
    );
    // Agent config_error notifications suppressed — KAS recursive agent search
    // produces false positives (non-agent files in .kiro/agents/ subfolders).
    // Reverted flat-search fix needs follow-up: https://github.com/kiro-team/kiro-agent/pull/1191
    this.kiroClient.onExtNotification('_kiro/error/rate_limit', (params) => {
      this.handleRateLimitError(params);
    });
    this.kiroClient.onExtNotification(
      '_kiro/mcp/governance_disabled',
      (params) => {
        // Transform KAS reason enum to the apiFailure boolean the handler expects
        const reason = params.reason as string | undefined;
        const apiFailure = reason === 'api_failure' || reason === 'no_endpoint';
        this.handleMcpGovernanceDisabled({ ...params, apiFailure });
      }
    );
    // Unified governance state (kiro-agent #1145). MCP is handled above via its
    // dedicated notification, so here we only consume the web tools toggle.
    this.kiroClient.onExtNotification('_kiro/governance/state', (params) => {
      const webTools = webToolsGovernanceFromState(params);
      if (webTools) {
        this.handleWebToolsGovernanceDisabled(webTools);
      }
    });

    // Forward roster deltas as stream events; the app store owns the roster
    // state and the derived cloud status. Dark-safe: today's KAS pushes none.
    this.kiroClient.onExtNotification('_kiro/sessions/changed', (params) => {
      const delta = params as unknown as SessionsChangedNotification;
      // Cloud provision outcome (once per session, cloud-only): a `failed`
      // status is a provisioning failure; the first live status is `ready`
      // and closes the `started` → ready latency histogram.
      if (this.startedCloudSession) {
        for (const up of delta.upserted ?? []) {
          if (up?.sessionId !== this.sessionId || !up.status) continue;
          if (up.status === 'failed') {
            if (!this.cloudProvisionFailedEmitted) {
              this.cloudProvisionFailedEmitted = true;
              recordTuiCloudSession({ event: 'provision_failed' });
            }
          } else if (
            !this.cloudReadyEmitted &&
            // Only a session THIS process provisioned has a start baseline; a
            // reattach leaves `cloudSessionStartMs` unset, so its later status
            // deltas must not fire a spurious zero-duration `ready`.
            this.cloudSessionStartMs !== undefined &&
            (up.status === 'idle' ||
              up.status === 'in_progress' ||
              up.status === 'waiting_on_user' ||
              up.status === 'completed')
          ) {
            this.cloudReadyEmitted = true;
            recordTuiCloudSessionReady({
              durationSeconds: this.cloudSessionStartMs
                ? (Date.now() - this.cloudSessionStartMs) / 1000
                : 0,
            });
          }
        }
      }
      this.broadcastStreamEvent({
        type: AgentEventType.SessionRosterDelta,
        delta,
      });
    });

    const commands = getKasCommands().map((cmd) => ({
      name: cmd.name,
      description: cmd.description,
      meta: cmd.meta,
    }));

    this.broadcastStreamEvent({
      type: AgentEventType.KasCommandsDiscovered,
      commands,
    });

    logger.debug('[acp-client] KAS ACP handshake done');
  }

  /**
   * Whether the active session is genuinely running on a cloud sandbox (the
   * placement was advertised and sent) — false when `--cloud` degraded local.
   */
  isCloudSessionActive(): boolean {
    return this.startedCloudSession;
  }

  override close(): void {
    if (this.retired) return;
    this.retired = true;
    this.hooksNotificationDisposable?.dispose();
    this.hooksNotificationDisposable = null;
    this.toolsNotificationDisposable?.dispose();
    this.toolsNotificationDisposable = null;
    this.sessionDisposables.forEach((d) => d.dispose());
    this.sessionDisposables = [];
    this.v3ToolCalls.reset();
    super.close();
  }

  /**
   * Whether KAS advertised support for the given execution-target kind on the
   * `initialize` handshake. An absent capability (older KAS) == unsupported, so
   * callers degrade to a local session. This is the dark-ship safety gate.
   */
  private isExecutionTargetSupported(kind: string): boolean {
    return this.kiroCapabilities.executionTargets?.includes(kind) ?? false;
  }

  async newSession(): Promise<SessionResult> {
    this.assertActive('session creation');
    const initialMode = this.initialAgent ?? process.env.KIRO_MODE;
    // Build the `_meta.kiro` payload once, merging mode + execution target so
    // neither overwrites the other (two separate `_meta` spreads would drop one).
    const kiroMeta: Record<string, unknown> = {};
    if (initialMode) kiroMeta.modeId = toKasModeId(initialMode);
    // Effective placement; an unset target == local (the contract default).
    const target = this.executionTarget ?? { kind: 'local' };
    // State the placement EXPLICITLY (including `{kind:'local'}`) on the wire,
    // but ONLY once KAS has advertised it understands this executionTarget kind
    // on the `initialize` handshake. Rationale (T2):
    //  - Existing users / dark-ship: every released KAS today advertises no
    //    `executionTargets`, so `isExecutionTargetSupported` is false and we
    //    send NOTHING -- byte-identical to pre-feature behavior. No existing
    //    user is impacted until KAS actually ships the capability.
    //  - Forward-compat: once KAS does parse the field, stating local
    //    explicitly means our placement intent never rides on how a future KAS
    //    chooses to interpret an *absent* executionTarget (KAS-owned, could
    //    drift). Intent is pinned to what we say, not to someone else's default.
    //  - Fail-safe: a non-local placement KAS hasn't advertised degrades to a
    //    local session (omit) rather than sending something KAS can't honor.
    if (this.isExecutionTargetSupported(target.kind)) {
      kiroMeta.executionTarget = target;
    } else if (target.kind !== 'local') {
      logger.warn(
        `[acp-client] KAS did not advertise executionTarget '${target.kind}' ` +
          `(advertised: ${JSON.stringify(
            this.kiroCapabilities.executionTargets ?? []
          )}); starting a local session instead.`
      );
      if (target.kind === 'cloud-sandbox') {
        recordTuiCloudSession({ event: 'fell_back_local' });
      }
    }
    // A `cloud-sandbox` placement is a cloud session, so send `sessionSource:
    // 'remote'` alongside the cloud `executionTarget`. This block is only reachable
    // once KAS advertised `cloud-sandbox` (the gate above set
    // `kiroMeta.executionTarget`), so it is inert for existing users -- today's KAS
    // advertises nothing and we send the same bytes as before. `sessionSource` is
    // gated independently on its own advertised capability, so we never send a flag
    // KAS didn't advertise. With no repo bound this is the "New" (empty-workspace)
    // start; starting a session bound to a repo is handled separately.
    const intendedCloudSandbox =
      (kiroMeta.executionTarget as ExecutionTarget | undefined)?.kind ===
      'cloud-sandbox';
    if (intendedCloudSandbox) {
      if (this.kiroCapabilities.sessionSources?.includes('remote')) {
        kiroMeta.sessionSource = 'remote';
      }
      if (this.repos && this.repos.length > 0) {
        // Bind the selected repositories at session/new; they are fixed for the
        // session's life. KAS resolves each `name`|`owner/name` string and drops
        // an unresolvable one with a warning. No `isEmptyWorkspace` -- the
        // workspace is the bound repo(s).
        kiroMeta.repositories = this.repos;
      } else {
        // "New" empty sandbox: no repo bound, so tell KAS to start an empty
        // workspace and not validate a local cwd (it owns the sandbox cwd).
        kiroMeta.isEmptyWorkspace = true;
      }
    }
    const r = await this.kiroClient
      .newSession({
        cwd: process.cwd(),
        mcpServers: [],
        ...(Object.keys(kiroMeta).length > 0 && { _meta: { kiro: kiroMeta } }),
      })
      .catch((err) => {
        if (intendedCloudSandbox) {
          recordTuiCloudSession({ event: 'start_failed' });
        }
        throw err;
      });
    this.assertActive('session creation');
    this.startedCloudSession = intendedCloudSandbox;
    if (intendedCloudSandbox) {
      this.cloudSessionStartMs = Date.now();
      this.cloudReadyEmitted = false;
      this.cloudProvisionFailedEmitted = false;
      recordTuiCloudSession({ event: 'started' });
    }
    const sid = r.sessionId;
    this.sessionId = sid;
    logger.debug('KAS session created', { sessionId: sid });

    // KAS validates each requested repository against the FULL provider
    // catalog server-side and reports the ones it dropped via
    // `_meta.kiro.warnings` (e.g. `repository "x" was not found among your
    // connected source providers`). This is the authoritative bind outcome —
    // expose it so the UI can warn AND un-claim the repo from the footer.
    const newWarnings = (r as { _meta?: { kiro?: { warnings?: unknown } } })
      ._meta?.kiro?.warnings;
    this.sessionNewWarnings = Array.isArray(newWarnings)
      ? newWarnings.filter((w): w is string => typeof w === 'string')
      : [];
    if (this.sessionNewWarnings.length > 0) {
      logger.warn('[kas] session/new warnings:', this.sessionNewWarnings);
    }

    // Register BEFORE any async work to avoid race condition
    this.wireSessionListeners(sid);

    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: sid,
        configId: 'autopilot',
        value: 'on',
      });
    } catch (e) {
      logger.debug('Failed to set autopilot config:', e);
    }

    let configOptions = (r as { configOptions?: unknown }).configOptions;
    const savedDefault = readCliSettings()[Settings.CHAT_DEFAULT_MODEL];
    const modelToApply = resolveInitialModel({
      flagModel: this.initialModel ?? null,
      savedDefaultModel:
        typeof savedDefault === 'string' && savedDefault ? savedDefault : null,
    });
    if (modelToApply) {
      try {
        const modelResp = await this.kiroClient.setSessionConfigOption({
          sessionId: sid,
          configId: 'model',
          value: modelToApply,
        });
        configOptions =
          (modelResp as { configOptions?: unknown }).configOptions ??
          configOptions;
      } catch (e) {
        logger.debug('Failed to set default model:', e);
      }
    }

    this.emitConfigOptions(configOptions, 'newSession');
    const selections = deriveCurrentSelections(configOptions);
    return { sessionId: sid, ...selections };
  }

  async loadSession(
    sessionId: string,
    options?: { source?: 'local' | 'remote' }
  ): Promise<SessionResult> {
    this.assertActive('session load');
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;
    // A loaded session has no create-time bind warnings.
    this.sessionNewWarnings = [];

    // Register BEFORE loadSession to capture history replay events
    this.wireSessionListeners(sessionId);

    // session/load acts on ONE store (KAS rejects 'all' here). An explicit
    // source wins; else a cloud placement starts remote; else local first
    // with a remote retry when the local store reports not-found.
    const remoteCapable =
      this.kiroCapabilities.sessionSources?.includes('remote') ?? false;
    const loadFrom = (source?: 'remote') =>
      this.kiroClient.loadSession({
        sessionId,
        cwd: process.cwd(),
        mcpServers: [],
        ...(source && { _meta: { kiro: { sessionSource: source } } }),
      });
    const explicit = options?.source;
    const startRemote =
      remoteCapable &&
      (explicit === 'remote' ||
        (!explicit && this.executionTarget?.kind === 'cloud-sandbox'));
    const retryRemoteOnMiss = remoteCapable && !explicit && !startRemote;
    const r = await loadFrom(startRemote ? 'remote' : undefined)
      .catch((err) => {
        // Retry against the remote store only for a not-found: any other
        // failure (auth, transport) must surface as-is, not as a confusing
        // remote miss.
        const msg = extractRpcErrorMessage(err, '');
        if (!retryRemoteOnMiss || !/not found/i.test(msg)) throw err;
        logger.debug(
          '[acp-client] local session/load missed; retrying remote store:',
          err
        );
        return loadFrom('remote');
      })
      .catch((err) => {
        this.sessionId = previousSessionId;
        throw err;
      });
    this.assertActive('session load');
    logger.debug(
      '[acp-client] KAS loadSession completed for session:',
      sessionId
    );

    // A cloud placement is authoritative from `executionTarget` even when the
    // load resolved from a local cache (where `source` reads local). These
    // fields may sit at the `_meta` top level or under `_meta.kiro`; read both.
    type LoadMetaFields = {
      source?: unknown;
      executionTarget?: { kind?: unknown };
    };
    const rawMeta = (
      r as { _meta?: LoadMetaFields & { kiro?: LoadMetaFields } }
    )._meta;
    const loadMeta: LoadMetaFields | undefined = rawMeta?.kiro ?? rawMeta;
    const isCloudSession =
      loadMeta?.executionTarget?.kind === 'cloud-sandbox' ||
      loadMeta?.source === 'remote';
    if (isCloudSession) {
      // A resumed cloud session lights the cloud footer/commands
      // (isCloudSessionActive), just as a fresh cloud `newSession` does.
      this.startedCloudSession = true;
      recordTuiCloudSession({ event: 'reattached' });
    } else {
      // The active session IS local: the whole surface must read local (footer,
      // cloud-only commands), even when the process was launched `--cloud` or
      // the previous session was remote. Mode follows the session.
      this.startedCloudSession = false;
    }

    const configOptions = (r as { configOptions?: unknown }).configOptions;
    this.emitConfigOptions(configOptions, 'loadSession');
    const selections = deriveCurrentSelections(configOptions);
    return { sessionId, ...selections };
  }

  async prompt(messages: acp.ContentBlock[]): Promise<void> {
    if (!this.sessionId)
      throw new Error('cannot send prompt without an active session');

    this.emitChatSessionStartedOnce(this.sessionId);

    // Race prompt against process exit to detect KAS crashes
    const { promise: crashed, unsubscribe } = this.processExitPromise();
    // Defensive: the exit listener is detached in finally, but there is a
    // narrow window between Promise.race settling and the finally running
    // where a late rejection could surface as an unhandled rejection. The
    // unsubscribe() call in finally removes the listener; this catch
    // covers the race between scheduling finally and the listener firing.
    crashed.catch(() => {});
    try {
      await Promise.race([
        this.kiroClient.prompt({ prompt: messages, sessionId: this.sessionId }),
        crashed,
      ]);
    } finally {
      unsubscribe();
    }
  }

  async cancel(): Promise<void> {
    if (!this.sessionId) return;
    try {
      this.kiroClient.cancel(this.sessionId);
    } catch (e) {
      logger.error('Failed to send cancel notification:', e);
    }
  }

  async executeCommand(command: TuiCommand): Promise<CommandResult> {
    const name = command.command;
    switch (name) {
      case 'quit':
        return { success: true, message: 'Quitting' };
      case 'feedback':
        return kasFeedback(
          (command as Record<string, unknown>).args as
            | Record<string, string>
            | undefined
        );
      case 'help':
        return this.executeHelp();
      case 'clear':
        return this.executeClear();
      case 'plan': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        return this.executePlan(args?.value || args?.prompt);
      }
      case 'paste':
        return executePaste();
      case 'reply':
        return { success: true, message: '' };
      case 'usage': {
        const result = await this.callExtMethod('_kiro/account/getUsage');
        if (!result.success) return result;
        const response = result.data as
          | { success: boolean; message: string; data?: unknown }
          | undefined;
        return {
          success: response?.success ?? true,
          message: response?.message ?? '',
          data: response?.data,
        };
      }
      case 'context': {
        // /context is normally driven by the `handleContext` kas-handler, but
        // the /usage↔/context Tab switch calls executeCommand('context')
        // directly and expects the V2 Rust shape `{ data: { breakdown } }`.
        // Without this case it fell through to the default "not yet supported"
        // branch, so Tab from /usage silently did nothing in KAS mode.
        const response = await this.contextShow();
        return {
          success: true,
          message: response.message ?? '',
          data: response.breakdown
            ? { breakdown: response.breakdown }
            : undefined,
        };
      }
      case 'prompts': {
        // Owned by the `handlePrompts` kas-handler. Reaching this branch
        // means the dispatcher's KAS intercept was skipped (e.g. caller
        // bypassed `executeCommandWithArg`); fall through to a no-op.
        return { success: true, message: '' };
      }
      case 'knowledge': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const value = args?.value ?? 'show';
        return this.executeKnowledge(value);
      }
      case 'hooks':
        return this.executeHooks();
      case 'compact': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const compactAttemptId = this.startCompactCompletionAttempt();
        this.broadcastStreamEvent({
          type: AgentEventType.CompactionStatus,
          status: 'started',
          attemptId: compactAttemptId,
        });
        // KAS reports the compact outcome only as `{ success: boolean }` (see
        // the `_kiro/session/compact` covenant type) and resolves the RPC after
        // the operation finishes. It never emits a `summarization_started`, so
        // the eager 'started' above is what shows the spinner; we terminate it
        // from the RPC result, deriving purely from `success` — we never infer
        // a reason the agent didn't give us:
        //   • success → delayed fallback 'completed'. A real compaction also
        //     emits a `summarization_completed` session update carrying the
        //     summary; that update cancels the fallback so queued input cannot
        //     overtake the report in lite scrollback. If KAS has no report
        //     (for example a no-op compact), the fallback still clears the
        //     spinner.
        //   • failure → 'failed', surfacing a reason only when KAS actually
        //     provided one (e.g. a thrown error message); otherwise the UI just
        //     shows that compaction failed.
        this.callExtMethod('_kiro/session/compact', {
          ...(args?.value && { value: args.value }),
        })
          .then((result) => {
            // callExtMethod wraps any non-throwing RPC as
            // { success: true, data: <response> }, so KAS's real status is in
            // result.data.success.
            const body = result.data as { success?: boolean } | undefined;
            if (!result.success || body?.success === false) {
              if (!this.isCompactCompletionAttemptPending(compactAttemptId)) {
                return;
              }
              this.markCompactCompletionObserved(compactAttemptId);
              this.broadcastStreamEvent({
                type: AgentEventType.CompactionStatus,
                status: 'failed',
                attemptId: compactAttemptId,
                error: result.message || undefined,
              });
            } else {
              this.scheduleCompactCompletionFallback(compactAttemptId);
            }
          })
          // Defensive: callExtMethod swallows errors today, but guard against a
          // future change so a rejected promise can never strand the spinner.
          .catch((e) => {
            if (!this.isCompactCompletionAttemptPending(compactAttemptId)) {
              return;
            }
            this.markCompactCompletionObserved(compactAttemptId);
            this.broadcastStreamEvent({
              type: AgentEventType.CompactionStatus,
              status: 'failed',
              attemptId: compactAttemptId,
              error: e instanceof Error ? e.message : undefined,
            });
          });
        return { success: true, message: 'Compacting conversation...' };
      }
      case 'rewind': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const kasId = args?.messageId;

        if (!kasId) {
          return { success: false, message: 'No KAS message ID for this turn' };
        }

        try {
          const response = await this.kiroClient.sendExtMethod('session/fork', {
            sessionId: this.sessionId,
            cwd: process.cwd(),
            _meta: { kiro: { messageId: kasId, createdReason: 'rewind' } },
          });
          return {
            success: true,
            message: '',
            data: {
              sessionId: (response as any).sessionId,
              switchSession: true,
            },
          };
        } catch (err) {
          return {
            success: false,
            message: err instanceof Error ? err.message : 'Fork failed',
          };
        }
      }
      case 'code': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const value = args?.value ?? 'status';
        return this.executeCode(value);
      }
      default:
        return {
          success: false,
          message: `/${name} is not yet supported in KAS mode`,
        };
    }
  }

  /** /help — effect expects data.commands with { name, description, usage } */
  /** Attach KAS-assigned messageId to the most recent User message for session/fork. */

  private async executeHelp(): Promise<CommandResult> {
    const result = await this.callExtMethod('_kiro/help');
    if (!result.success) return result;
    const data = result.data as {
      commands?: Array<{ name: string; description: string }>;
    };
    return {
      success: true,
      message: 'Available commands',
      data: {
        commands: (data?.commands ?? []).map((c) => ({
          name: c.name,
          description: c.description,
          usage: c.name,
        })),
      },
    };
  }

  /**
   * /hooks — lists configured hooks from the agent's registry.
   */
  private async executeHooks(): Promise<CommandResult> {
    let result: CommandResult;
    try {
      result = await this.callExtMethod('_kiro/hooks/list', {
        trigger: 'all',
      });
    } catch (e) {
      logger.debug('[kas/hooks] failed to fetch hooks list:', e);
      return {
        success: false,
        message: 'Unable to fetch hooks — agent may be unavailable',
        data: { hooks: [], message: 'Unable to fetch hooks' },
      };
    }

    if (result.success && result.data) {
      const data = result.data as {
        hooks?: Array<{
          name?: string;
          trigger?: string;
          matcher?: string;
          action?: { type?: string; command?: string; prompt?: string };
        }>;
      };

      if (Array.isArray(data?.hooks)) {
        const hooks = this.projectHooks(data.hooks);
        return this.formatHooksResult(hooks);
      }
    }

    if (!result.success) {
      logger.debug('[kas/hooks] ext method returned failure:', result.message);
      return {
        success: false,
        message: result.message || 'Unable to fetch hooks',
        data: { hooks: [], message: 'Unable to fetch hooks' },
      };
    }

    return {
      success: true,
      message: 'No hooks configured',
      data: { hooks: [], message: 'No hooks configured' },
    };
  }

  /** Project raw hook data into the normalized display snapshot. */
  private projectHooks(
    rawHooks: Array<{
      name?: string;
      trigger?: string;
      matcher?: string;
      action?: { type?: string; command?: string; prompt?: string };
      _meta?: {
        trigger?: string;
        matcher?: string;
        source?: string;
        filePath?: string;
        enabled?: boolean;
      };
    }>
  ): HooksUpdateEvent['hooks'] {
    return rawHooks.map((h) => {
      const trigger = h._meta?.trigger ?? h.trigger ?? 'unknown';
      const matcher = h._meta?.matcher ?? h.matcher;
      const actionType = h.action?.type;
      const command =
        actionType === 'runCommand' || actionType === 'command'
          ? (h.action?.command ?? '')
          : actionType === 'askAgent' || actionType === 'agent'
            ? `[agent] ${(h.action?.prompt ?? '').slice(0, 60)}`
            : (h.name ?? 'unknown');
      return {
        ...(h.name ? { name: h.name } : {}),
        trigger,
        command,
        ...(matcher ? { matcher } : {}),
      };
    });
  }

  /** Format a hooks array into a CommandResult for the panel. */
  private formatHooksResult(hooks: HooksUpdateEvent['hooks']): CommandResult {
    if (hooks.length === 0) {
      return {
        success: true,
        message: 'No hooks configured',
        data: { hooks: [], message: 'No hooks configured' },
      };
    }
    const message = `${hooks.length} hook${hooks.length === 1 ? '' : 's'} configured`;
    return { success: true, message, data: { hooks, message } };
  }

  private async executeKnowledge(value: string): Promise<CommandResult> {
    const parts = value.trim().split(/\s+/);
    const subcommand = parts[0] || 'show';

    const params: Record<string, unknown> = { subcommand };

    if (subcommand === 'add' && parts.length >= 3) {
      params.name = parts[1];
      params.path = parts.slice(2).join(' ');
    } else if (subcommand === 'remove' && parts.length >= 2) {
      params.target = parts.slice(1).join(' ');
    } else if (subcommand === 'update' && parts.length >= 2) {
      params.path = parts.slice(1).join(' ');
    } else if (subcommand === 'cancel' && parts.length >= 2) {
      params.operationId = parts[1];
    }

    const result = await this.callExtMethod('_kiro/knowledge', params);
    if (!result.success) return result;

    const data = result.data as
      | { entries?: unknown[]; message?: string }
      | undefined;
    // 'show' returns entries (triggers panel). Mutations return message (triggers alert).
    if (subcommand === 'show') {
      return {
        success: true,
        message: '',
        data: { entries: data?.entries ?? [] },
      };
    }
    return {
      success: true,
      message: data?.message ?? '',
    };
  }

  /**
   * Routes /context subcommands (show/add/remove/clear) to the agent's
   * `_kiro/session/context` ACP ext method.
   *
   * Each public method below is a 1:1 typed wrapper over a single
   * `callExtMethod('_kiro/session/context', { subcommand, ... })` call —
   * no parsing, no flag handling, no aliases. All of that lives in
   * `kas-handlers/context.ts` so this client stays a thin
   * type-safe wrapper.
   *
   * The wire shape mirrors the (forthcoming) typed `ContextParams` in
   * `@kiro/acp-type-covenant`: a discriminated union on `subcommand`.
   * See the inline TODO in `types/session-client.ts` for the bump-then-
   * import follow-up.
   */
  async contextShow(): Promise<KasContextShowResponse> {
    const result = await this.callExtMethod('_kiro/session/context', {
      subcommand: 'show',
    });
    if (!result.success) {
      // RPC-level failure — surface as an exception so the handler's
      // try/catch path runs and shows a clean error alert.
      throw new Error(result.message || '/context show failed');
    }
    const data = result.data as KasContextShowResponse | undefined;
    return {
      entries: data?.entries ?? [],
      message: data?.message,
      breakdown: data?.breakdown,
    };
  }

  async contextAdd(
    path: string,
    opts?: { force?: boolean }
  ): Promise<KasContextMutationResponse> {
    const params: Record<string, unknown> = {
      subcommand: 'add',
      path,
    };
    if (opts?.force) params.force = true;
    return this.contextMutation(params);
  }

  async contextRemove(path: string): Promise<KasContextMutationResponse> {
    return this.contextMutation({ subcommand: 'remove', path });
  }

  async contextClear(): Promise<KasContextMutationResponse> {
    return this.contextMutation({ subcommand: 'clear' });
  }

  /**
   * Shared dispatch for add/remove/clear.  The agent encodes its own
   * domain-level success/failure inside the response payload (e.g. add
   * reports `success: false` for "path not found"), distinct from the
   * RPC-level success of the call. Surface the inner flag so the
   * handler can pick the right alert tone.
   */
  private async contextMutation(
    params: Record<string, unknown>
  ): Promise<KasContextMutationResponse> {
    const result = await this.callExtMethod('_kiro/session/context', params);
    if (!result.success) {
      throw new Error(result.message || `/context ${params.subcommand} failed`);
    }
    const data = result.data as KasContextMutationResponse | undefined;
    return {
      success: data?.success !== false,
      message: data?.message,
    };
  }

  async resetMcpServer(serverName: string, startOAuth: boolean): Promise<void> {
    await this.kiroClient.sendExtMethod('_kiro/mcp/resetServer', {
      serverName,
      startOAuth,
    });
  }

  private async executeCode(subcommand: string): Promise<CommandResult> {
    const validSubcommands = ['status', 'init', 'overview'];
    const cmd = subcommand.trim().split(/\s+/)[0] || 'status';
    if (!validSubcommands.includes(cmd)) {
      return {
        success: false,
        message: `Unknown subcommand '${cmd}'. Use: ${validSubcommands.join(', ')}`,
      };
    }
    // TODO: 'logs' is not yet supported by kiro-agent (returns empty array)
    const result = await this.callExtMethod('_kiro/codeIntelligence', {
      subcommand: cmd,
    });
    if (!result.success) return result;

    // After init, fetch status to show the panel
    if (cmd === 'init') {
      const statusResult = await this.callExtMethod('_kiro/codeIntelligence', {
        subcommand: 'status',
      });
      if (statusResult.success) return this.formatCodeResponse(statusResult);
      return statusResult;
    }

    return this.formatCodeResponse(result);
  }

  // TODO: Import CodeIntelligenceResponse from @kiro/acp-type-covenant once released (post v0.3.33)
  private formatCodeResponse(result: CommandResult): CommandResult {
    const response = result.data as
      | {
          success?: boolean;
          message?: string;
          overview?: string;
          status?: {
            initialized: boolean;
            languages: string[];
            lspServers: Array<{
              name: string;
              languages: string[];
              status: string;
              isAvailable: boolean;
            }>;
          };
        }
      | undefined;

    if (!response?.success) {
      return { success: false, message: response?.message ?? 'Command failed' };
    }

    // overview/summary → inject as prompt
    if (response.overview) {
      return {
        success: true,
        message: '',
        data: {
          executePrompt: `Here is the codebase overview:\n\n${response.overview}\n\nAnalyze this codebase structure and provide a summary of the project architecture.`,
          label: '/code overview',
        },
      };
    }

    // status → transform to CodePanelData shape
    if (response.status) {
      const { status } = response;
      const statusStr = status.initialized ? 'initialized' : 'not_initialized';
      const message = status.initialized
        ? 'Workspace initialized'
        : 'Workspace not initialized. Run /code init to initialize.';
      return {
        success: true,
        message: '',
        data: {
          status: statusStr,
          message,
          // TODO: The following fields are static placeholders. They will be populated
          // by kiro-agent once upstream changes land there (subsequent PRs).
          rootPath: process.cwd(),
          detectedLanguages: status.languages,
          projectMarkers: [],
          lsps: (status.lspServers ?? []).map((s) => ({
            name: s.name,
            languages: s.languages,
            status: s.status,
            isAvailable: s.isAvailable,
            initDurationMs: null,
            workspaceFolders: [process.cwd()],
          })),
          configPath: '.kiro/settings/lsp.json',
          docUrl: 'https://kiro.dev/docs/cli/code-intelligence/',
        },
      };
    }

    // logs — just show the message
    return { success: true, message: response.message ?? '' };
  }

  /**
   * Normalize backend-initiated agent switches (e.g. a spec-workflow handoff)
   * so the chip / welcome banner never surface a raw KAS wire id like `vibe`.
   * The base implementation is identity (V2 ids need no translation).
   */
  protected override handleAgentSwitched(
    params: Record<string, unknown>
  ): void {
    const p = params as { agentName?: string; previousAgentName?: string };
    super.handleAgentSwitched({
      ...params,
      ...(p.agentName ? { agentName: fromKasModeId(p.agentName) } : {}),
      ...(p.previousAgentName
        ? { previousAgentName: fromKasModeId(p.previousAgentName) }
        : {}),
    });
  }

  /**
   * Handle `_kiro/mcp/status` notification from KAS.
   * Normalizes the wire data and publishes store-owned display snapshots.
   */
  private handleMcpStatusNotification(params: Record<string, unknown>): void {
    const servers = params.servers as
      | Array<{
          name: string;
          status: 'connecting' | 'connected' | 'failed' | 'disabled';
          authType?: 'oauth';
          tools?: Array<{
            name: string;
            description?: string;
            disabled: boolean;
          }>;
          failedAuthorization?: boolean;
          authorizationUrl?: string;
          errorMessage?: string;
        }>
      | undefined;

    const serverSnapshot: McpServerSnapshotEvent['servers'] = (
      servers ?? []
    ).map((server) => {
      let status: McpServerSnapshotEvent['servers'][number]['status'];
      switch (server.status) {
        case 'connected':
          status = 'running';
          break;
        case 'connecting':
          status = 'loading';
          break;
        case 'failed':
          status = server.failedAuthorization ? 'auth-required' : 'failed';
          break;
        case 'disabled':
          status = 'disabled';
          break;
        default:
          status = 'failed';
      }

      return {
        name: server.name,
        status,
        toolCount: server.tools?.length ?? 0,
      };
    });
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerSnapshot,
      servers: serverSnapshot,
    });

    if (servers) {
      // Broadcast OAuth URL for servers that need authentication,
      // and clear pending OAuth for servers that have connected.
      const stillPendingAuth = new Set<string>();
      for (const server of servers) {
        if (server.failedAuthorization && server.authorizationUrl) {
          stillPendingAuth.add(server.name);
          this.pendingOAuthServerNames.add(server.name);
          this.broadcastStreamEvent({
            type: AgentEventType.McpOauthRequest,
            serverName: server.name,
            oauthUrl: server.authorizationUrl,
          });
        }
      }
      // Emit McpServerInitialized only for servers transitioning from
      // pending-OAuth to connected (not for every connected server on
      // every notification).
      for (const server of servers) {
        if (
          this.pendingOAuthServerNames.has(server.name) &&
          !stillPendingAuth.has(server.name) &&
          server.status === 'connected'
        ) {
          this.pendingOAuthServerNames.delete(server.name);
          this.broadcastStreamEvent({
            type: AgentEventType.McpServerInitialized,
            serverName: server.name,
          });
        }
      }
    }

    // Publish registry servers separately from the configured-server snapshot.
    const registryServers =
      (params.registryServers as Array<{
        name: string;
        version?: string;
        description?: string;
        enabled?: boolean;
      }>) ?? [];
    const registrySnapshot = registryServers.map((s) => ({
      name: s.name,
      status: 'disabled' as const,
      toolCount: 0,
      version: s.version,
      description: s.description,
      enabled: s.enabled,
    }));
    this.broadcastStreamEvent({
      type: AgentEventType.McpRegistrySnapshot,
      registryServers: registrySnapshot,
    });

    logger.debug(
      '[kas] handleMcpStatusNotification: published',
      serverSnapshot.length,
      'configured,',
      registrySnapshot.length,
      'registry servers'
    );
  }

  /**
   * /clear — compose from ACP primitives: create a fresh session
   * (session/new) and let the TUI reset its screen.  KAS intentionally
   * does not expose a higher-level "clear conversation" extension method,
   * so the client composes this behavior itself.
   */
  private async executeClear(): Promise<CommandResult> {
    try {
      const session = await this.newSession();
      return {
        success: true,
        message: 'Conversation cleared',
        data: {
          sessionId: session.sessionId,
          currentModel: session.currentModel,
          currentAgent: session.currentAgent,
        },
      };
    } catch (e) {
      return {
        success: false,
        message:
          e instanceof Error ? e.message : 'Failed to clear conversation',
      };
    }
  }

  /** /plan — switch to plan mode, optionally send trailing prompt */
  private async executePlan(prompt?: string): Promise<CommandResult> {
    try {
      await this.setConfigOption('mode', 'kiro_planner');
    } catch (e) {
      return {
        success: false,
        message:
          e instanceof Error ? e.message : 'Failed to switch to plan mode',
      };
    }
    return {
      success: true,
      message: 'Switched to kiro_planner',
      data: { agent: { name: 'kiro_planner' }, ...(prompt && { prompt }) },
    };
  }

  private async callExtMethod(
    method: string,
    extra?: Record<string, unknown>
  ): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      const result = await this.kiroClient.sendExtMethod(method, {
        sessionId: this.sessionId,
        ...extra,
      });
      return { success: true, message: '', data: result };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Command failed',
      };
    }
  }

  /**
   * Resolve (or create) the ACP session the agent uses to work on a spec
   * feature.  See `_kiro/spec/resolveSession` in `@kiro/acp-type-covenant`.
   *
   * Strategy `'reuse'` returns the session previously associated with the
   * feature (via `SpecSessionTracker`), or creates a new one.  Strategy
   * `'fresh'` always creates a new session.
   *
   * Note: the returned session ID is owned by the agent.  The client does
   * not switch its active `sessionId` here — `_kiro/spec/invoke` routes
   * updates to whichever session it started execution on, and the client
   * re-subscribes separately when it wants to render them.
   */
  async resolveSpecSession(
    request: SpecResolveSessionRequest
  ): Promise<SpecResolveSessionResponse> {
    return this.kiroClient.sendExtMethod('_kiro/spec/resolveSession', request);
  }

  /**
   * Invoke a spec operation (`executeTask`, `runAllTasks`, or
   * `generateDocument`) on the agent.  The agent drives execution
   * autonomously from here — the client observes progress via the usual
   * session-update and `_kiro/spec/taskStatusChanged` notifications.
   */
  async invokeSpec(request: SpecInvokeRequest): Promise<SpecInvokeResponse> {
    return this.kiroClient.sendExtMethod('_kiro/spec/invoke', request);
  }

  async getCommandOptions(
    commandName: string,
    _partial: string
  ): Promise<CommandOptionsResponse> {
    if (!this.sessionId) return { options: [] };
    const name = commandName.replace(/^\//, '');
    switch (name) {
      case 'feedback':
        return KAS_FEEDBACK_OPTIONS;
      // /model, /agent, /effort, and /prompts options are owned by their
      // respective kas-handlers, which read the typed AppState slices
      // (kasAvailableModels / kasAvailableAgents / kasAvailableEfforts /
      // prompts) and never round-trip through here.
      default:
        return { options: [] };
    }
  }

  /**
   * Set a session config option (model / agent-mode / effort). Thin
   * passthrough to KAS's `session/set_config_option`: it performs the write
   * and re-emits the normalized config-option events from the response so the
   * store self-heals. Agent ids are mapped to KAS wire ids. Validation and
   * user messaging live in the kas-handlers, which read the resulting store
   * state.
   */
  async setConfigOption(
    configId: 'mode' | 'model' | 'effortLevel',
    value: string
  ): Promise<void> {
    if (!this.sessionId) return;
    const wireValue = configId === 'mode' ? toKasModeId(value) : value;
    const response = await this.kiroClient.setSessionConfigOption({
      sessionId: this.sessionId,
      configId,
      value: wireValue,
    });
    this.emitConfigOptions(
      (response as { configOptions?: unknown }).configOptions,
      'clientInitiated',
      { emitCurrentAgent: true }
    );
  }

  async listSessions(cwd: string): Promise<ListSessionsResponse> {
    try {
      // Request the remote dimensions under `_meta.kiro`, each gated on its OWN
      // advertised capability. When neither is advertised, `_meta.kiro` is omitted
      // and the wire is byte-identical to a plain listing.
      const caps = this.kiroCapabilities;
      const kiroMeta: Record<string, unknown> = {};
      // Which store: span BOTH local + remote when a remote store is advertised;
      // else local-only (the default). Requesting 'remote'/'all' unadvertised is a
      // typed error server-side, so gate on the cap.
      if (caps.sessionSources?.includes('remote'))
        kiroMeta.sessionSource = 'all';
      // Breadth: the user slice comes from the (user-scoped) remote store; request
      // 'both' only when 'user' scope is advertised, else 'workspace' (the default).
      if (caps.sessionListScopes?.includes('user')) kiroMeta.listScope = 'both';

      const params: acp.ListSessionsRequest = { cwd };
      if (Object.keys(kiroMeta).length > 0) {
        params._meta = { kiro: kiroMeta };
      }
      const r = await this.kiroClient.listSessions(params);
      logger.debug('[kas] listSessions raw:', JSON.stringify(r));

      // Graceful degradation (e.g. the remote store is down on a both+all query):
      // KAS returns the local rows plus a `_meta.kiro.warnings` entry. Surface it;
      // don't fail the listing (local is authoritative).
      const warnings = (r as { _meta?: { kiro?: { warnings?: unknown } } })
        ._meta?.kiro?.warnings;
      if (Array.isArray(warnings) && warnings.length > 0) {
        logger.warn('[kas] session/list warnings:', JSON.stringify(warnings));
      }

      return {
        sessions: r.sessions.map((s) => {
          // Per-row remote dimensions ride `_meta.kiro`, typed by the covenant
          // list-item meta. Absent == local; `executionTarget` is the WHERE the
          // picker surfaces, `source` is the store to route a later load/delete
          // to, `status` a cold snapshot.
          const k: Partial<KiroSessionListItemMeta> =
            (s._meta as { kiro?: KiroSessionListItemMeta } | undefined)?.kiro ??
            {};
          return {
            sessionId: s.sessionId,
            cwd: s.cwd,
            title: s.title ?? undefined,
            updatedAt:
              s.updatedAt ??
              (s._meta as { createdAt?: string } | undefined)?.createdAt ??
              k.createdAt,
            executionTarget: k.executionTarget,
            source: k.source,
            status: k.status,
          };
        }),
      };
    } catch (e) {
      logger.debug('[kas] listSessions failed:', e);
      return { sessions: [] };
    }
  }

  async listSettings(): Promise<Record<string, unknown>> {
    return readCliSettings();
  }

  async setSetting(key: string, value: unknown): Promise<void> {
    await updateCliSetting(key, value);
  }

  async terminateSession(_sessionId: string): Promise<void> {}

  async spawnSession(
    _task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }> {
    logger.debug('spawnSession not yet supported in KAS mode');
    return { sessionId: '', name: name ?? '' };
  }

  /**
   * Gate for the `_kiro/sourceProviders/*` pull methods (repo picker): the
   * `sourceProviders` capability must be advertised AND the method must appear
   * in `extensionMethods` (consulted together as a fail-safe). Dark-safe:
   * today's KAS advertises neither, so callers get `undefined` and never issue
   * the ext call. Remove the gate once `sourceProviders` is always advertised.
   */
  private isSourceProvidersMethodAvailable(method: string): boolean {
    return (
      this.kiroCapabilities.sourceProviders === true &&
      (this.kiroCapabilities.extensionMethods?.includes(method) ?? false)
    );
  }

  /**
   * List the account's source providers + connection state via
   * `_kiro/sourceProviders/list`. Returns `undefined` when the surface is
   * unavailable (capability not advertised) or the call fails -- the repo picker
   * treats that as "no in-CLI picker" and points at the web portal.
   */
  async listSourceProviders(): Promise<SourceProviderList | undefined> {
    if (!this.isSourceProvidersMethodAvailable('_kiro/sourceProviders/list')) {
      return undefined;
    }
    // One retry after a short pause: the first call right after the handshake
    // can race the agent's own token refresh (observed as a transient
    // UnauthorizedException server-side) — a blip must not disable the picker
    // and the cloud-entry gate for the whole session.
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        return await this.kiroClient.sendExtMethod(
          '_kiro/sourceProviders/list',
          {}
        );
      } catch (e) {
        logger.debug(
          `[kas] sourceProviders/list failed (attempt ${attempt + 1}):`,
          e
        );
        if (attempt === 0) {
          await new Promise((r) => setTimeout(r, 1500));
        }
      }
    }
    return undefined;
  }

  /**
   * Page one provider's repositories via `_kiro/sourceProviders/listResources`.
   * Returns `undefined` when unavailable or on failure. The selected
   * resource's `name` is what binds to a session via `_meta.kiro.repositories`.
   */
  async listSourceProviderResources(
    request: SourceProviderResourcesRequest
  ): Promise<SourceProviderResourcePage | undefined> {
    if (
      !this.isSourceProvidersMethodAvailable(
        '_kiro/sourceProviders/listResources'
      )
    ) {
      return undefined;
    }
    try {
      return await this.kiroClient.sendExtMethod(
        '_kiro/sourceProviders/listResources',
        request
      );
    } catch (e) {
      logger.debug('[kas] sourceProviders/listResources failed:', e);
      return undefined;
    }
  }

  /**
   * KAS does not implement the `_message/send` ext method that the Rust
   * backend uses for the wake/reply path. KAS achieves the same wake/reply
   * semantics via the standard ACP `session/prompt` RPC, so we shim
   * `sendMessage` to call `kiroClient.prompt(...)` directly. This keeps
   * `SessionViewScreen` reply (the only `kiro.sendMessage` call site
   * outside of init replay) working when KAS eventually grows crew
   * session support.
   *
   * The base class implementation routes through `extRequest →
   * kiroClient.sendExtMethod('_message/send', ...)` which would throw
   * `Unknown ext method: _message/send` against today's KAS. Once KAS
   * implements `_message/send`, this override can be removed and the
   * base will work uniformly across engines.
   */
  override async sendMessage(
    sessionId: string,
    content: string
  ): Promise<void> {
    await this.kiroClient.prompt({
      prompt: [{ type: 'text', text: content }],
      sessionId,
    });
  }

  protected async extRequest<T = unknown>(
    method: string,
    params: Record<string, unknown>
  ): Promise<T> {
    return (await this.kiroClient.sendExtMethod(method, params)) as T;
  }

  // KAS host-side telemetry logs used the OTLP /v1/logs path KUTS doesn't
  // support; KAS owns its own server-side telemetry now. These methods stay to
  // satisfy the SessionClient contract but no-op — Prometheus metrics come from
  // the recordTui* calls in emitChatSessionStartedOnce / forwardKasTurnCompletion.
  sendProcessHealthMetrics(_payload: ProcessHealthSnapshot): void {}

  sendModeChanged(_payload: ModeChangedNotification): void {}

  sendChatSlashCommandTelemetry(
    _payload: ChatSlashCommandTelemetryPayload
  ): void {}

  private emitChatSessionStartedOnce(sessionId: string): void {
    if (this.chatSessionStartedSessions.has(sessionId)) return;
    this.chatSessionStartedSessions.add(sessionId);
    const mode = modeFromId(this.telemetryCurrentModeId);
    recordTuiSessionStarted({
      mode,
      version: this.version,
    });
    recordTuiModeActive({ mode });
  }

  private forwardKasTurnCompletionTelemetry(
    sessionId: string,
    update: AcpSessionUpdate
  ): void {
    if (update.sessionUpdate !== 'session_info_update') return;
    const meta = extractKasSessionInfoMeta(update);
    if (meta?.kind !== 'turn_completion') return;
    const payload = normalizeKasTurnCompletion(
      meta,
      sessionId,
      this.telemetryCurrentModelId
    );
    if (!payload) return;

    // Missing duration stays undefined (not 0) so it doesn't pollute the
    // histogram. is_subagent is false: main session only.
    const mode = modeFromId(this.telemetryCurrentModeId);
    const model = payload.modelId ?? '';
    const isSubagent = false;
    recordTuiUserTurn({
      model,
      result: resultFromStatus(payload.status),
      isSubagent,
      mode,
      chatConversationType: 'acp',
      durationSeconds:
        payload.turnDurationMs != null
          ? payload.turnDurationMs / 1000
          : undefined,
    });

    // §C4 backfill — emit the product metrics on the V3 path too (clean v2-vs-v3 split).
    recordTuiModelInvocation({ model });
    recordTuiTokensConsumed({
      model,
      isSubagent,
      tokens: {
        input_uncached: payload.uncachedInputTokens,
        input_cache_read: payload.cacheReadInputTokens,
        input_cache_write: payload.cacheWriteInputTokens,
        output: payload.outputTokens,
      },
    });
    if (payload.contextUsagePercentage != null) {
      recordTuiContextUsage({
        model,
        isSubagent,
        percentage: payload.contextUsagePercentage,
      });
    }
    recordTuiTurnOutcome({ status: payload.status, model, mode });
  }

  sendUiModeSessionStart(_payload: UiModeSessionStartNotification): void {
    // TODO: implement KAS-side telemetry when KAS supports ext notifications
  }

  sendUiModeChanged(_payload: UiModeChangedNotification): void {
    // TODO: implement KAS-side telemetry when KAS supports ext notifications
  }

  sendUiModeDefaultChanged(_payload: UiModeDefaultChangedNotification): void {
    // TODO: implement KAS-side telemetry when KAS supports ext notifications
  }
}

/** Tagged union describing what `/agent …` should do.  Produced by
 *  parseAgentSubcommand and consumed by the `/agent` kas-handler. */
export type ParsedAgentCommand =
  | { kind: 'list' }
  | { kind: 'swap'; name: string }
  | { kind: 'create'; name: string | undefined }
  | { kind: 'edit'; name: string | undefined };

/** Parse the `args` payload passed to `executeCommand({ command: 'agent' })`
 *  into a structured subcommand.
 *
 *  Grammar:
 *    ""                           → { kind: 'list' }
 *    "swap <name>"                → { kind: 'swap',   name: '<name>' }
 *    "create" | "create <name>"   → { kind: 'create', name: '<name>' | undefined }
 *    "edit"   | "edit <name>"     → { kind: 'edit',   name: '<name>' | undefined }
 *    "<name>"                     → { kind: 'swap',   name: '<name>' }  (menu shorthand)
 *
 *  The raw text comes from either `args.agentName` (direct test harness
 *  usage) or `args.value` (the dispatcher's generic selection-UI shape).
 *
 *  Exported for unit testing; not part of the public client API. */
export function parseAgentSubcommand(
  args: Record<string, string> | undefined
): ParsedAgentCommand {
  const raw = (args?.agentName ?? args?.value ?? '').trim();
  if (!raw) return { kind: 'list' };

  const spaceIdx = raw.indexOf(' ');
  const verb = (spaceIdx === -1 ? raw : raw.slice(0, spaceIdx)).toLowerCase();
  const rest = spaceIdx === -1 ? '' : raw.slice(spaceIdx + 1).trim();

  switch (verb) {
    case 'create':
      return { kind: 'create', name: rest || undefined };
    case 'edit':
      return { kind: 'edit', name: rest || undefined };
    case 'swap':
      // Bare "swap" with no name is a user error; surfaced by executeCommand.
      return { kind: 'swap', name: rest };
    default:
      // Menu shorthand: a bare value with no verb swaps to that agent.
      return { kind: 'swap', name: raw };
  }
}

/** Static feedback options for /feedback selection menu. */
const KAS_FEEDBACK_OPTIONS: CommandOptionsResponse = {
  options: [
    {
      value: 'general',
      label: 'General feedback',
      description: 'Share general thoughts or suggestions',
    },
    {
      value: 'feature',
      label: 'Feature request',
      description: 'Request a new feature or improvement',
    },
    {
      value: 'issue',
      label: 'Report an issue',
      description: 'Report a bug or problem',
    },
  ],
};

/** External (public) feedback intake — GitHub. */
const EXTERNAL_FEEDBACK_URLS: Record<string, string> = {
  general: 'https://github.com/kirodotdev/Kiro/issues/new/choose',
  feature:
    'https://github.com/kirodotdev/Kiro/issues/new?template=feature_request.yml',
  issue: 'https://github.com/kirodotdev/Kiro/issues',
};

/** Internal (Amazon) feedback intake — Taskei templates. */
const INTERNAL_FEEDBACK_URLS: Record<string, string> = {
  general:
    'https://taskei.amazon.dev/tasks/create?template=f5ac492c-9ec3-4a2d-8abb-2f486c7222eb',
  feature:
    'https://taskei.amazon.dev/tasks/create?template=a05ddcbb-e4c6-4783-8eca-ef46ae5d7ef6',
  issue:
    'https://taskei.amazon.dev/tasks/create?template=5389200f-f825-4261-98ec-04bc84572fab',
};

/**
 * Resolve the feedback URL: internal (Amazon) users → Taskei, everyone else →
 * GitHub. Unknown kinds fall back to `general`. Pure for testability.
 */
export function resolveFeedbackUrl(kind: string, isInternal: boolean): string {
  const urls = isInternal ? INTERNAL_FEEDBACK_URLS : EXTERNAL_FEEDBACK_URLS;
  return urls[kind] ?? urls.general!;
}

function kasFeedback(args?: Record<string, string>): CommandResult {
  const kind = args?.value || 'general';
  const url = resolveFeedbackUrl(kind, features.isInternalUser);
  if (openUrlInBrowser(url)) {
    return { success: true, message: 'Opening in browser...' };
  }
  return {
    success: false,
    message: `Could not open browser. Copy the URL: ${url}`,
    data: { url },
  };
}

/**
 * /paste — read an image from the system clipboard and return it in the
 * shape expected by the `pasteImage` effect handler (base64 PNG + dims).
 *
 * The Rust backend handles this server-side via the `arboard` crate; in
 * KAS mode the agent is a plain TypeScript ACP server with no clipboard
 * access, so the TUI composes the command itself. The returned image is
 * forwarded as a ContentBlock on the next prompt by the effect handler.
 */
export function executePaste(): CommandResult {
  const result = readClipboardImage();
  if (!result.ok) {
    return { success: false, message: result.error.message };
  }
  return {
    success: true,
    message: 'Image pasted from clipboard',
    data: {
      data: result.image.data,
      mimeType: result.image.mimeType,
      width: result.image.width,
      height: result.image.height,
      sizeBytes: result.image.sizeBytes,
    },
  };
}
