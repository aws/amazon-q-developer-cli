import * as acp from '@agentclientprotocol/sdk';
import { logger } from '../utils/logger';
import { parseSessionRepositories } from '../utils/session-repositories';
import { isUserCancelledReason } from '../constants/tool-failure-reasons';
import type { ChildProcess } from 'node:child_process';
import type {
  SessionClient,
  ListSessionsResponse,
} from '../types/session-client';
import type { ContextBreakdownData } from '../types/context';
import type { ProcessHealthSnapshot } from '../utils/process-health-collector';
import type {
  ModeChangedNotification,
  UiModeChangedNotification,
  UiModeDefaultChangedNotification,
  UiModeSessionStartNotification,
} from '../types/generated/chat-cli';
import {
  AgentEventType,
  ContentType,
  ApprovalOptionId,
  ToolCallStatus,
  type AgentStreamEvent,
  type ApprovalRequestEvent,
  type KiroMeta,
  type MeteringUsage,
  type RejectedAgentConfig,
  type ToolCallOrigin,
} from '../types/agent-events';
import type {
  CommandOptionsResponse,
  CommandResult,
  PromptEntry,
  PromptSource,
  SkillEntry,
  SkillSource,
  SteeringEntry,
  SteeringSource,
  TuiCommand,
} from '../types/commands';
import {
  TuiFirstVisibleResponseObserver,
  type Engine,
  type TuiToolCallStart,
} from '../utils/tui-telemetry-observer';
import type { SessionEvent } from '../types/multi-session';
import {
  decodeExtSessionUpdate,
  extractKiroMeta,
  type ExtSessionUpdateEnvelope,
} from './kas-extensions/session-update-contract';

/**
 * Strip the `@serverName/` prefix from KAS MCP tool titles.
 * KAS sends titles like "@test-mock/echo"; V2 sends just "echo".
 */
export function stripMcpTitlePrefix(
  title: string | undefined
): string | undefined {
  return parseMcpTitle(title)?.toolName ?? title;
}

export function mcpServerNameFromTitle(
  title: string | undefined
): string | undefined {
  return parseMcpTitle(title)?.serverName;
}

function parseMcpTitle(
  title: string | undefined
): { serverName: string; toolName: string } | undefined {
  const match = title?.match(/^@([^/]+)\/(.+)$/);
  return match ? { serverName: match[1]!, toolName: match[2]! } : undefined;
}

function parseRunningMcpTitle(
  title: string | undefined
): { serverName: string; toolName: string } | undefined {
  const match = title?.match(/^Running:\s+(.+)$/);
  return parseMcpTitle(match?.[1]);
}

const AGENT_REJECTION_REASONS = new Set([
  'cli_only_agent',
  'invalid_config',
  'unreadable',
  'internal_error',
]);

/**
 * Read the rejected-file detail off an agent not-found notification. A backend
 * that doesn't send it, or sends it without the two fields worth showing,
 * yields undefined — which reads as "no file claimed that id".
 */
function parseRejectedAgentConfig(
  value: unknown
): RejectedAgentConfig | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  if (typeof raw.path !== 'string' || typeof raw.error !== 'string') {
    return undefined;
  }
  const reasonCode =
    typeof raw.reasonCode === 'string' &&
    AGENT_REJECTION_REASONS.has(raw.reasonCode)
      ? (raw.reasonCode as RejectedAgentConfig['reasonCode'])
      : undefined;
  return {
    path: raw.path,
    error: raw.error,
    ...(reasonCode ? { reasonCode } : {}),
  };
}

export function normalizeToolCallTitle(
  title: string | undefined,
  canonicalName?: string
): {
  name: string | undefined;
  origin: ToolCallOrigin;
  originalTitle?: string;
} {
  const identitySource = canonicalName || title;
  const name = stripMcpTitlePrefix(identitySource);
  const isQualifiedMcpIdentity = (value: string | undefined): boolean =>
    !!value &&
    (value.startsWith('mcp__') || parseMcpTitle(value) !== undefined);
  const runningMcpTitle = parseRunningMcpTitle(title);
  const runningTitleMatchesCanonical =
    runningMcpTitle !== undefined &&
    (canonicalName === undefined || runningMcpTitle.toolName === canonicalName);
  const isMcp =
    isQualifiedMcpIdentity(identitySource) ||
    isQualifiedMcpIdentity(title) ||
    runningTitleMatchesCanonical;
  const originalTitle =
    title && title !== name
      ? title
      : identitySource && identitySource !== name
        ? identitySource
        : undefined;
  return {
    name,
    origin: isMcp ? 'mcp' : 'builtin',
    ...(originalTitle && { originalTitle }),
  };
}

/**
 * Unwrap KAS MCP output envelope to match V2 format.
 * KAS sends: `{ response: "...", imageBase64Urls: [] }`
 * V2 sends:  `{ content: [{ type: "text", text: "..." }] }`
 */
export function unwrapKasMcpOutput(raw: unknown): unknown {
  if (
    raw &&
    typeof raw === 'object' &&
    'response' in raw &&
    'imageBase64Urls' in raw
  ) {
    const envelope = raw as { response: string; imageBase64Urls: string[] };
    return { content: [{ type: 'text', text: envelope.response }] };
  }
  return raw;
}

export type AcpSessionUpdate = acp.SessionNotification['update'];

/**
 * KAS extends ACP session updates with `_meta.kiro` for pipeline metadata,
 * per-stage event tagging, and command consent context. The base ACP type
 * has no notion of `_meta`, so we overlay it here for type-safe access.
 *
 * Use {@link extractKiroMetaFromUpdate} to read `_meta.kiro` from an
 * `AcpSessionUpdate` without scattering casts across the codebase.
 */
/** Type-safe `_meta.kiro` accessor for ACP session updates. */
export function extractKiroMetaFromUpdate(
  update: unknown
): KiroMeta | undefined {
  return extractKiroMeta(update);
}

function extractToolKiroMetaFromUpdate(
  update: AcpSessionUpdate
): KiroMeta | undefined {
  const kiroMeta = extractKiroMetaFromUpdate(update);
  const title = (update as AcpSessionUpdate & { title?: string }).title;
  const mcpServerName =
    kiroMeta?.mcpServerName ?? mcpServerNameFromTitle(title);
  if (!mcpServerName || kiroMeta?.mcpServerName === mcpServerName) {
    return kiroMeta;
  }
  return { ...(kiroMeta ?? {}), mcpServerName };
}

export function toolTelemetryStartFromEvent(
  event: AgentStreamEvent
): TuiToolCallStart {
  const kiroMeta = 'meta' in event && event.meta ? event.meta.kiro : undefined;
  const name = kiroMeta?.toolName ?? ('name' in event ? event.name : '') ?? '';
  if (kiroMeta?.pipeline) {
    return {
      name,
      toolOrigin: 'builtin',
      builtinToolName: 'use_subagent',
    };
  }
  if (kiroMeta?.mcpServerName) {
    return {
      name,
      toolOrigin: 'mcp',
      mcpServerName: kiroMeta.mcpServerName,
    };
  }
  return { name, toolOrigin: 'builtin', builtinToolName: name || 'unknown' };
}

export const EXT_METHODS = {
  COMMANDS_AVAILABLE: 'kiro.dev/commands/available',
  COMMANDS_EXECUTE: 'kiro.dev/commands/execute',
  COMMANDS_OPTIONS: 'kiro.dev/commands/options',
  METADATA: 'kiro.dev/metadata',
  COMPACTION_STATUS: 'kiro.dev/compaction/status',
  CLEAR_STATUS: 'kiro.dev/clear/status',
  MCP_SERVER_INIT_FAILURE: 'kiro.dev/mcp/server_init_failure',
  MCP_OAUTH_REQUEST: 'kiro.dev/mcp/oauth_request',
  MCP_SERVER_INITIALIZED: 'kiro.dev/mcp/server_initialized',
  MCP_GOVERNANCE_DISABLED: 'kiro.dev/mcp/governance_disabled',
  WEB_TOOLS_GOVERNANCE_DISABLED: 'kiro.dev/webTools/governance_disabled',
  AGENT_NOT_FOUND: 'kiro.dev/agent/not_found',
  AGENT_CONFIG_ERROR: 'kiro.dev/agent/config_error',
  RATE_LIMIT_ERROR: 'kiro.dev/error/rate_limit',
  SUBAGENT_LIST_UPDATE: 'kiro.dev/subagent/list_update',
  SESSION_SPAWN: 'session/spawn',
  SESSION_TERMINATE: 'session/terminate',
  MESSAGE_SEND: 'message/send',
  SESSION_STEER: 'session/steer',
  SESSION_STEER_CLEAR: 'session/steer/clear',
  AGENT_SWITCHED: 'kiro.dev/agent/switched',
  SESSION_UPDATE: 'kiro.dev/session/update',
  GOAL_STATUS: 'kiro.dev/goal/status',
} as const;

/**
 * Steering commands hidden from the TUI slash-command menu. KAS ships
 * built-in steering documents that register inline slash commands to trigger
 * bundled workflows (e.g. `/quick-spec`, `/architecture-selection`,
 * `/bug-fix`). Product does not surface these bundled workflows in the TUI
 * (their picker modes are hidden too — see the agent allowlist in
 * `utils/kas-config-options.ts`), so the inline commands are dropped from
 * autocomplete as well. User/workspace steering documents are unaffected.
 */
const HIDDEN_STEERING_COMMANDS = new Set<string>([
  'quick-spec',
  'architecture-selection',
  'bug-fix',
]);

export type SessionResult = {
  sessionId: string;
  currentModel?: { id: string; name: string };
  currentAgent?: { name: string; welcomeMessage?: string };
  /** Present when the loaded session was forked from another. */
  parentSessionId?: string;
  /** Why the session was forked (e.g. 'tangent', 'rewind', 'subagent'). */
  createdReason?: string;
  /** Session title (tangent name for tangent forks). */
  title?: string;
};

type KasPromptTurnSummary = {
  usage?: unknown;
  unit?: unknown;
  unitPlural?: unknown;
  usedTools?: unknown;
};

type KasTokenUsageMeta = {
  totalTokens?: unknown;
  inputTokens?: unknown;
  uncachedInputTokens?: unknown;
  outputTokens?: unknown;
  cachedTokens?: unknown;
  cacheReadInputTokens?: unknown;
  cacheWriteInputTokens?: unknown;
};

type KasSessionInfoMeta = KasTokenUsageMeta & {
  kind?: string;
  conversationSummary?: string;
  summarization?: {
    status: string;
    summary?: string | { conversationSummary?: string; content?: string };
  };
  contextUsage?: { usagePercentage?: number };
  usagePercentage?: number;
  breakdown?: unknown;
  // Some agents emit the user-facing error for an in-flight tool call here
  // rather than on the tool_call_update payload. Captured into
  // pendingDisplayError so tool_call_update Failed can use it as a fallback
  // when its own error fields are empty.
  displayError?: { message?: string };
  error?: unknown;
  stopReason?: string;
  promptTurnSummaries?: KasPromptTurnSummary[];
  tokenUsage?: unknown;
  usage?: unknown;
  metrics?: unknown;
  elapsedTime?: unknown;
  status?: unknown;
  content?: string;
  // KAS sends one steering_queued per steer (only its own text), unlike Rust
  // which echoes the whole buffer; accumulate by messageId to rebuild it.
  messageId?: string;
  // Present only for workflow-internal notification steering. Keep this raw
  // at the wire boundary and validate it before changing routing behavior.
  notificationSeverity?: unknown;
};

const COMPACT_COMPLETION_FALLBACK_MS = 500;
const KAS_NOTIFICATION_SEVERITIES = new Set([
  'info',
  'success',
  'warning',
  'error',
]);

function isKasNotificationSteering(meta: KasSessionInfoMeta): boolean {
  return (
    typeof meta.notificationSeverity === 'string' &&
    KAS_NOTIFICATION_SEVERITIES.has(meta.notificationSeverity)
  );
}

type KasTurnCompletionTelemetryPayload = {
  sessionId?: string;
  modelId?: string;
  modelInvocationCount?: number;
  meteringUsage: MeteringUsage[];
  turnDurationMs?: number;
  contextUsagePercentage?: number;
  totalTokens?: number;
  uncachedInputTokens?: number;
  outputTokens?: number;
  cacheReadInputTokens?: number;
  cacheWriteInputTokens?: number;
  status?: string;
  usedTools?: string[];
};

export function extractKasSessionInfoMeta(
  update: AcpSessionUpdate
): KasSessionInfoMeta | undefined {
  return (update as { _meta?: { kiro?: KasSessionInfoMeta } })._meta?.kiro;
}

function extractKasSummarizationSummary(
  meta: KasSessionInfoMeta
): string | undefined {
  if (typeof meta.conversationSummary === 'string') {
    return meta.conversationSummary;
  }
  const summary = meta.summarization?.summary;
  if (typeof summary === 'string') return summary;
  return summary?.conversationSummary ?? summary?.content;
}

function extractKasError(meta: KasSessionInfoMeta): string | undefined {
  if (typeof meta.error === 'string') return meta.error;
  if (
    typeof meta.error === 'object' &&
    meta.error !== null &&
    'message' in meta.error &&
    typeof meta.error.message === 'string'
  ) {
    return meta.error.message;
  }
  return meta.displayError?.message;
}

function normalizeKasTurnCompletionStatus(status: unknown): string | undefined {
  return typeof status === 'string' ? status : undefined;
}

function normalizeKasContextUsagePercentage(
  meta: KasSessionInfoMeta
): number | undefined {
  const percentage = meta.usagePercentage ?? meta.contextUsage?.usagePercentage;
  return typeof percentage === 'number' && Number.isFinite(percentage)
    ? percentage
    : undefined;
}

function isKasMetaRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function normalizeKasTokenCount(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) && value > 0
    ? Math.floor(value)
    : undefined;
}

function kasTokenSources(meta: KasSessionInfoMeta): Record<string, unknown>[] {
  const sources: Record<string, unknown>[] = [];
  if (isKasMetaRecord(meta.tokenUsage)) sources.push(meta.tokenUsage);
  if (isKasMetaRecord(meta.usage)) sources.push(meta.usage);
  if (isKasMetaRecord(meta.metrics)) sources.push(meta.metrics);
  sources.push(meta as Record<string, unknown>);
  return sources;
}

function firstKasTokenCount(
  meta: KasSessionInfoMeta,
  names: string[]
): number | undefined {
  for (const source of kasTokenSources(meta)) {
    for (const name of names) {
      const value = normalizeKasTokenCount(source[name]);
      if (value !== undefined) return value;
    }
  }
  return undefined;
}

function normalizeKasTurnTokenCounts(
  meta: KasSessionInfoMeta
): Omit<
  KasTurnCompletionTelemetryPayload,
  | 'sessionId'
  | 'modelId'
  | 'meteringUsage'
  | 'turnDurationMs'
  | 'contextUsagePercentage'
  | 'status'
> {
  const uncachedInputTokens = firstKasTokenCount(meta, [
    'uncachedInputTokens',
    'inputTokens',
  ]);
  const outputTokens = firstKasTokenCount(meta, ['outputTokens']);
  const cacheReadInputTokens = firstKasTokenCount(meta, [
    'cacheReadInputTokens',
    'cachedTokens',
  ]);
  const cacheWriteInputTokens = firstKasTokenCount(meta, [
    'cacheWriteInputTokens',
  ]);
  const derivedTotal =
    uncachedInputTokens !== undefined ||
    outputTokens !== undefined ||
    cacheReadInputTokens !== undefined
      ? (uncachedInputTokens ?? 0) +
        (outputTokens ?? 0) +
        (cacheReadInputTokens ?? 0)
      : undefined;
  const totalTokens = firstKasTokenCount(meta, ['totalTokens']) ?? derivedTotal;

  return {
    ...(totalTokens !== undefined ? { totalTokens } : {}),
    ...(uncachedInputTokens !== undefined ? { uncachedInputTokens } : {}),
    ...(outputTokens !== undefined ? { outputTokens } : {}),
    ...(cacheReadInputTokens !== undefined ? { cacheReadInputTokens } : {}),
    ...(cacheWriteInputTokens !== undefined ? { cacheWriteInputTokens } : {}),
  };
}

function normalizeKasUsedTools(meta: KasSessionInfoMeta): string[] {
  const seen = new Set<string>();
  for (const summary of meta.promptTurnSummaries ?? []) {
    if (!Array.isArray(summary.usedTools)) continue;
    for (const tool of summary.usedTools) {
      if (typeof tool !== 'string') continue;
      const name = tool.trim();
      if (name.length > 0) seen.add(name);
    }
  }
  return Array.from(seen);
}

export function normalizeKasTurnCompletion(
  meta: KasSessionInfoMeta,
  sessionId?: string,
  modelId?: string
): KasTurnCompletionTelemetryPayload | undefined {
  const meteringUsage = (meta.promptTurnSummaries ?? [])
    .filter(
      (entry): entry is { usage: number } & KasPromptTurnSummary =>
        typeof entry.usage === 'number'
    )
    .map((entry) => ({
      value: entry.usage,
      unit: typeof entry.unit === 'string' ? entry.unit : '',
      unitPlural: typeof entry.unitPlural === 'string' ? entry.unitPlural : '',
    }));
  const modelInvocationCount = meta.promptTurnSummaries?.length ?? 0;
  const turnDurationMs =
    typeof meta.elapsedTime === 'number' ? meta.elapsedTime : undefined;
  const contextUsagePercentage = normalizeKasContextUsagePercentage(meta);
  const tokenCounts = normalizeKasTurnTokenCounts(meta);
  const status = normalizeKasTurnCompletionStatus(meta.status);
  const usedTools = normalizeKasUsedTools(meta);
  if (
    modelInvocationCount === 0 &&
    turnDurationMs == null &&
    contextUsagePercentage == null &&
    Object.keys(tokenCounts).length === 0 &&
    !status &&
    usedTools.length === 0
  ) {
    return undefined;
  }

  return {
    ...(sessionId ? { sessionId } : {}),
    ...(modelId ? { modelId } : {}),
    ...(modelInvocationCount > 0 ? { modelInvocationCount } : {}),
    meteringUsage,
    ...(turnDurationMs != null ? { turnDurationMs } : {}),
    ...(contextUsagePercentage != null ? { contextUsagePercentage } : {}),
    ...tokenCounts,
    ...(status ? { status } : {}),
    ...(usedTools.length > 0 ? { usedTools } : {}),
  };
}

// ─── Shared stdio plumbing ───────────────────────────────────────────

/** Build the parsed-message ReadableStream and ndJson WritableStream from a child process. */
export function buildStdioStreams(agentProcess: ChildProcess) {
  const stdin = agentProcess.stdin!;
  const stdout = agentProcess.stdout!;

  stdin.on('error', (err) => logger.error('Agent stdin error:', err));

  const writable = new WritableStream<Uint8Array>({
    async write(chunk) {
      if (stdin.destroyed || stdin.writableEnded) return;
      return new Promise<void>((resolve, reject) => {
        stdin.write(chunk, (err) => (err ? reject(err) : resolve()));
      });
    },
    close() {
      stdin.end();
    },
    abort(reason) {
      stdin.destroy(
        reason instanceof Error ? reason : new Error(String(reason))
      );
    },
  });

  let buffer = '';
  const decoder = new TextDecoder();
  let messageController: ReadableStreamDefaultController<any>;
  const parsedMessages = new ReadableStream<any>({
    start(controller) {
      messageController = controller;
    },
    cancel() {
      stdout.destroy();
    },
  });

  stdout.on('data', (chunk: Buffer) => {
    buffer += decoder.decode(chunk, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed) {
        try {
          messageController.enqueue(JSON.parse(trimmed));
        } catch (err) {
          logger.error('[pipe] Failed to parse JSON:', trimmed, err);
        }
      }
    }
  });
  stdout.on('end', () => {
    if (buffer.trim()) {
      try {
        messageController.enqueue(JSON.parse(buffer.trim()));
      } catch {
        /* ignore */
      }
    }
    messageController.close();
  });
  stdout.on('error', (err) => {
    logger.error('[pipe] stdout error:', err);
    messageController.error(err);
  });

  const dummyReadable = new ReadableStream<Uint8Array>({ start() {} });
  const ndJson = acp.ndJsonStream(writable, dummyReadable);
  return { readable: parsedMessages, writable: ndJson.writable };
}

/**
 * Narrowed view of `ChildProcess` used by `BaseAcpClient` and its
 * subclasses. Declared explicitly so the null shim used in test mode can
 * satisfy it without pretending to implement the entirety of
 * `ChildProcess`. Adding a new method here forces the mock to implement
 * it rather than silently misbehaving at runtime.
 */
export interface AgentProcess {
  readonly stdin: NodeJS.WritableStream | null;
  readonly stdout: NodeJS.ReadableStream | null;
  readonly stderr: NodeJS.ReadableStream | null;
  kill(signal?: NodeJS.Signals | number): boolean;
  /**
   * Registers an exit listener. Returns an unsubscribe function so callers
   * that may register many times (e.g. once per prompt) can detach on
   * completion and never leak listeners in mock mode where exit never
   * fires.
   */
  onExit(listener: (code: number | null) => void): () => void;
}

/** Wrap a real `ChildProcess` so it satisfies `AgentProcess`. */
export function toAgentProcess(proc: ChildProcess): AgentProcess {
  // stdin/stdout/stderr are set synchronously on spawn and never change,
  // so plain property reads are sufficient (no getter indirection).
  //
  // On Unix/macOS the agent is spawned with `detached: true` so it's the
  // leader of its own process group. That lets us signal `-pgid` here to
  // bring down the agent *and every grandchild it spawned* (MCP servers,
  // subagents, etc.) in one shot — without the negative PID, agent crashes
  // leak MCP children as ppid=1 orphans that accumulate across restarts.
  //
  // On Windows, `detached: true` allocates a visible console window for the
  // child (Node.js / libuv maps it to CREATE_NEW_PROCESS_GROUP), and
  // process.kill(-pid) is a no-op (Windows ignores the negative sign). So
  // we skip both the detach and the group kill on Windows. See P460297924.
  const isWindows = process.platform === 'win32';
  return {
    stdin: proc.stdin,
    stdout: proc.stdout,
    stderr: proc.stderr,
    kill: (signal) => {
      if (!isWindows && proc.pid && proc.pid > 0) {
        try {
          process.kill(-proc.pid, signal ?? 'SIGTERM');
          return true;
        } catch {
          // Group may already be dead, or we lost the race with reap. Fall
          // through to the per-process kill so we still tear down the leader
          // if it's somehow still alive.
        }
      }
      return proc.kill(signal);
    },
    onExit(listener) {
      proc.once('exit', listener);
      return () => {
        proc.off('exit', listener);
      };
    },
  };
}

function pipeStderr(agentProcess: AgentProcess) {
  if (!agentProcess.stderr) return;
  let buf = '';
  agentProcess.stderr.on('data', (chunk: Buffer) => {
    buf += chunk.toString();
    const lines = buf.split('\n');
    buf = lines.pop() || '';
    for (const line of lines) {
      if (line.trim()) logger.warn('[agent-stderr]', line);
    }
  });
  agentProcess.stderr.on('end', () => {
    if (buf.trim()) logger.warn('[agent-stderr]', buf);
  });
}

// ─── Prompt types ────────────────────────────────────────────────────
/**
 * V2 wire shape for a prompt entry from `kiro.dev/commands/available`.
 * `serverName` is overloaded by the upstream Rust HashMap key:
 *   - 'local' / 'global' for file-based user prompts
 *   - '<mcp-server-name>' for MCP prompts
 *   - 'skill:<path>' for V2 skill resources
 * The V2 ingest in `BaseAcpClient.handleCommandsAdvertising` partitions
 * this stream into typed `PromptEntry` / `SkillEntry` arrays; consumers
 * never see this shape directly.
 */
type V2WirePrompt = {
  name: string;
  description?: string;
  arguments: Array<{ name: string; description?: string; required?: boolean }>;
  serverName: string;
};

/** Map a V2 wire prompt to a typed PromptSource. */
function v2ServerNameToPromptSource(serverName: string): PromptSource {
  if (serverName === 'local') return { kind: 'workspace' };
  if (serverName === 'global') return { kind: 'global' };
  return { kind: 'mcp', serverName };
}

/**
 * Partition the V2 wire prompts array into typed prompt and skill
 * arrays. Skills are V2 prompts whose `serverName` carries the
 * `skill:` prefix (the upstream agent_config skill resource path).
 * This is the only place the `skill:` prefix is parsed.
 */
function partitionV2Prompts(wire: V2WirePrompt[]): {
  prompts: PromptEntry[];
  skills: SkillEntry[];
} {
  const prompts: PromptEntry[] = [];
  const skills: SkillEntry[] = [];
  for (const w of wire) {
    if (w.serverName.startsWith('skill:')) {
      // V2 wire skills emit `server_name: "skill:config"` (literal label,
      // not a path). The actual file path isn't carried on the wire today.
      // Future: enrich via `_meta.kiro.path` once the agent emits it.
      skills.push({
        name: w.name,
        description: w.description,
        source: { kind: 'agent-config' },
      });
    } else {
      prompts.push({
        name: w.name,
        description: w.description,
        arguments: w.arguments,
        source: v2ServerNameToPromptSource(w.serverName),
      });
    }
  }
  return { prompts, skills };
}

// ─── Base class ──────────────────────────────────────────────────────

export abstract class BaseAcpClient implements SessionClient {
  public sessionId?: string;
  protected agentProcess: AgentProcess;
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private multiSessionHandlers: Set<
    (sessionId: string, event: AgentStreamEvent) => void
  > = new Set();
  private sessionEventHandlers: Set<(event: SessionEvent) => void> = new Set();
  private subagentListHandlers: Set<
    (subagents: any[], pendingStages?: any[]) => void
  > = new Set();
  /** Captured from session_info_update displayError — consumed as fallback by tool_call_update Failed */
  private readonly pendingDisplayErrors = new Map<string, string>();
  private compactCompletionFallbackTimer: ReturnType<typeof setTimeout> | null =
    null;
  private compactCompletionAttemptId = 0;
  private observedCompactCompletionAttemptId = 0;
  private externalCompactInProgress = false;
  // KAS steers accumulated by messageId, to rebuild the full buffer the
  // SteeringQueued handler expects (Rust sends it whole; KAS sends deltas).
  // Reset per-session in wireSessionListeners (a /clear mid-steer ends the
  // session with no injected/cleared event, so it can't reset itself).
  protected readonly kasSteerBuffers = new Map<string, Map<string, string>>();
  private readonly firstVisibleResponse = new TuiFirstVisibleResponseObserver();

  constructor(agentProcess: AgentProcess) {
    this.agentProcess = agentProcess;
    if (!agentProcess.stdout || !agentProcess.stdin) {
      throw new Error('Failed to create agent process stdio streams');
    }
    pipeStderr(agentProcess);
  }

  // ── Abstract methods (differ per engine) ──

  abstract initialize(): Promise<void>;
  abstract newSession(): Promise<SessionResult>;
  abstract loadSession(sessionId: string): Promise<SessionResult>;
  abstract prompt(messages: acp.ContentBlock[]): Promise<void>;
  abstract cancel(): Promise<void>;
  abstract executeCommand(command: TuiCommand): Promise<CommandResult>;
  abstract getCommandOptions(
    commandName: string,
    partial: string
  ): Promise<CommandOptionsResponse>;
  abstract setConfigOption(
    configId: 'mode' | 'model' | 'effortLevel',
    value: string
  ): Promise<void>;
  abstract listSessions(cwd: string): Promise<ListSessionsResponse>;
  abstract listSettings(): Promise<Record<string, unknown>>;
  abstract setSetting(key: string, value: unknown): Promise<void>;
  abstract terminateSession(sessionId: string): Promise<void>;
  abstract spawnSession(
    task: string,
    name?: string
  ): Promise<{ sessionId: string; name: string }>;

  /**
   * Transport for extension methods. Subclasses implement this once (Rust
   * goes through `conn.extMethod(this.ext(method), ...)`, KAS through
   * `kiroClient.sendExtMethod(method, ...)`), and the concrete methods below
   * (`sendMessage`, `steerMessage`, `clearSteering`) share one implementation.
   *
   * `method` is the canonical underscore-prefixed name (e.g.
   * `_session/steer`). Rust's `ext()` helper is a no-op when already prefixed,
   * so both engines receive the same input.
   */
  protected abstract extRequest<T = unknown>(
    method: string,
    params: Record<string, unknown>
  ): Promise<T>;

  // ── Concrete extension-method wrappers (shared between engines) ──

  /**
   * Reply/wake path for persistent (subagent/crew) sessions — routes through
   * `_message/send → wake_session`, starting or resuming a full turn on the
   * target session. For mid-turn steering of the active session, use
   * {@link steerMessage} instead.
   */
  async sendMessage(sessionId: string, content: string): Promise<void> {
    await this.extRequest(`_${EXT_METHODS.MESSAGE_SEND}`, {
      sessionId,
      content,
    });
  }

  /**
   * Queue a mid-turn steering message. The backend holds it until the next
   * drain point (tool boundary or turn end), at which point it's injected
   * into the conversation alongside the pending tool results.
   */
  async steerMessage(sessionId: string, content: string): Promise<void> {
    await this.extRequest(`_${EXT_METHODS.SESSION_STEER}`, {
      sessionId,
      message: content,
    });
  }

  /**
   * Clear the queued steering message without consuming it. Complements
   * {@link steerMessage} for the "removed from queue" UX.
   */
  async clearSteering(sessionId: string): Promise<void> {
    await this.extRequest(`_${EXT_METHODS.SESSION_STEER_CLEAR}`, { sessionId });
  }
  abstract sendProcessHealthMetrics(payload: ProcessHealthSnapshot): void;
  abstract sendModeChanged(payload: ModeChangedNotification): void;
  abstract recordSlashCommandInvocation(command: string): void;
  abstract sendUiModeSessionStart(
    payload: UiModeSessionStartNotification
  ): void;
  abstract sendUiModeChanged(payload: UiModeChangedNotification): void;
  abstract sendUiModeDefaultChanged(
    payload: UiModeDefaultChangedNotification
  ): void;

  // ── Shared methods ──

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    return this.addHandler(this.updateHandlers, handler);
  }

  onMultiSessionUpdate(
    handler: (sessionId: string, event: AgentStreamEvent) => void
  ): () => void {
    return this.addHandler(this.multiSessionHandlers, handler);
  }

  onSubagentListUpdate(
    handler: (subagents: any[], pendingStages?: any[]) => void
  ): () => void {
    return this.addHandler(this.subagentListHandlers, handler);
  }

  onSessionEvent(handler: (event: SessionEvent) => void): () => void {
    return this.addHandler(this.sessionEventHandlers, handler);
  }

  private addHandler<T>(handlers: Set<T>, handler: T): () => void {
    if (this.closed) return () => {};
    handlers.add(handler);
    return () => {
      handlers.delete(handler);
    };
  }

  /**
   * Tear down the agent process group. Idempotent — multiple shutdown paths
   * (SIGINT, SIGTERM, beforeExit, uncaughtException) can all call this and
   * we only signal once.
   *
   * SIGTERM gives the agent a brief window to flush its MCP children
   * cleanly; if it's still alive 800ms later we follow up with SIGKILL so
   * a wedged agent can't hold up shutdown indefinitely.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.updateHandlers.clear();
    this.multiSessionHandlers.clear();
    this.sessionEventHandlers.clear();
    this.subagentListHandlers.clear();
    this.resetCompactCompletionFallback();
    this.agentProcess.kill('SIGTERM');
    // Best-effort SIGKILL escalation after 800ms grace period. Unix only —
    // on Windows SIGTERM already force-kills (there's no graceful/forced
    // distinction), so the escalation is a no-op that kills a dead process.
    if (process.platform !== 'win32') {
      setTimeout(() => {
        try {
          this.agentProcess.kill('SIGKILL');
        } catch {
          // already dead, fine
        }
      }, 800).unref();
    }
  }

  private closed = false;

  private resetCompactCompletionFallback(): void {
    if (!this.compactCompletionFallbackTimer) return;
    clearTimeout(this.compactCompletionFallbackTimer);
    this.compactCompletionFallbackTimer = null;
  }

  protected markCompactCompletionObserved(attemptId: number): void {
    if (attemptId !== this.compactCompletionAttemptId) return;
    this.observedCompactCompletionAttemptId = attemptId;
    this.resetCompactCompletionFallback();
  }

  protected startCompactCompletionAttempt(): number {
    this.resetCompactCompletionFallback();
    this.compactCompletionAttemptId += 1;
    return this.compactCompletionAttemptId;
  }

  protected isCompactCompletionAttemptPending(attemptId: number): boolean {
    return (
      attemptId === this.compactCompletionAttemptId &&
      this.observedCompactCompletionAttemptId < attemptId
    );
  }

  protected scheduleCompactCompletionFallback(attemptId: number): void {
    if (!this.isCompactCompletionAttemptPending(attemptId)) return;
    this.resetCompactCompletionFallback();
    this.compactCompletionFallbackTimer = setTimeout(() => {
      if (!this.isCompactCompletionAttemptPending(attemptId)) return;
      this.compactCompletionFallbackTimer = null;
      this.broadcastStreamEvent({
        type: AgentEventType.CompactionStatus,
        status: 'completed',
        attemptId,
      });
    }, COMPACT_COMPLETION_FALLBACK_MS);
  }

  protected consumeCompactSummaryAttemptId(): number | undefined | null {
    const attemptId = this.compactCompletionAttemptId;
    if (this.isCompactCompletionAttemptPending(attemptId)) {
      // KAS summary notifications do not carry a compact correlation id. The
      // best available source of truth is the currently pending local compact;
      // stale summaries after a newer compact starts are indistinguishable from
      // that newer compact's report until KAS provides a backend id.
      this.markCompactCompletionObserved(attemptId);
      return attemptId;
    }
    if (this.externalCompactInProgress) {
      this.externalCompactInProgress = false;
      return undefined;
    }
    return null;
  }

  protected broadcastStreamEvent(event: AgentStreamEvent): void {
    this.firstVisibleResponse.observe(event);
    this.updateHandlers.forEach((handler) => handler(event));
  }

  protected startFirstVisibleResponse(args: {
    mode: string;
    version: string;
    engine: Engine;
  }): void {
    this.firstVisibleResponse.start(args);
  }

  protected cancelFirstVisibleResponse(): void {
    this.firstVisibleResponse.cancel();
  }

  protected broadcastSynthesizedFailedToolCall(event: AgentStreamEvent): void {
    const eventSessionId = 'sessionId' in event ? event.sessionId : undefined;
    // Subagent tools are excluded because aggregate tool metrics have no session dimension.
    if (!eventSessionId || eventSessionId === this.sessionId) {
      this.observeTurnTelemetry(event);
    }
    this.broadcastStreamEvent(event);
  }

  protected broadcastMultiSession(
    sessionId: string,
    event: AgentStreamEvent
  ): void {
    this.multiSessionHandlers.forEach((h) => h(sessionId, event));
  }

  protected broadcastSessionEvent(event: SessionEvent): void {
    this.sessionEventHandlers.forEach((h) => h(event));
  }

  protected clearSessionConversionState(sessionId?: string): void {
    if (sessionId !== undefined) {
      this.pendingDisplayErrors.delete(sessionId);
      this.kasSteerBuffers.delete(sessionId);
      return;
    }
    this.pendingDisplayErrors.clear();
    this.kasSteerBuffers.clear();
    this.resetCompactCompletionFallback();
    this.compactCompletionAttemptId = 0;
    this.observedCompactCompletionAttemptId = 0;
    this.externalCompactInProgress = false;
  }

  protected broadcastSubagentList(
    subagents: any[],
    pendingStages?: any[]
  ): void {
    this.subagentListHandlers.forEach((h) => h(subagents, pendingStages));
  }

  // ── Shared ext notification handlers ──

  protected extNotificationHandlers: Record<
    string,
    (params: Record<string, unknown>) => void
  > = {
    [EXT_METHODS.COMMANDS_AVAILABLE]: (p) => this.handleCommandsAdvertising(p),
    [EXT_METHODS.METADATA]: (p) => this.handleMetadataUpdate(p),
    [EXT_METHODS.COMPACTION_STATUS]: (p) => this.handleCompactionStatus(p),
    [EXT_METHODS.CLEAR_STATUS]: () => this.handleClearStatus(),
    [EXT_METHODS.MCP_SERVER_INIT_FAILURE]: (p) =>
      this.handleMcpServerInitFailure(p),
    [EXT_METHODS.MCP_OAUTH_REQUEST]: (p) => this.handleMcpOauthRequest(p),
    [EXT_METHODS.MCP_SERVER_INITIALIZED]: (p) =>
      this.handleMcpServerInitialized(p),
    [EXT_METHODS.MCP_GOVERNANCE_DISABLED]: (p) =>
      this.handleMcpGovernanceDisabled(p),
    [EXT_METHODS.WEB_TOOLS_GOVERNANCE_DISABLED]: (p) =>
      this.handleWebToolsGovernanceDisabled(p),
    [EXT_METHODS.AGENT_NOT_FOUND]: (p) => this.handleAgentNotFound(p),
    [EXT_METHODS.AGENT_CONFIG_ERROR]: (p) => this.handleAgentConfigError(p),
    [EXT_METHODS.RATE_LIMIT_ERROR]: (p) => this.handleRateLimitError(p),
    [EXT_METHODS.SUBAGENT_LIST_UPDATE]: (p) => this.handleSubagentListUpdate(p),
    [EXT_METHODS.AGENT_SWITCHED]: (p) => this.handleAgentSwitched(p),
    [EXT_METHODS.SESSION_UPDATE]: (p) => this.handleExtSessionUpdate(p),
    [EXT_METHODS.GOAL_STATUS]: (p) => this.handleGoalStatus(p),
  };

  private handleCommandsAdvertising(params: Record<string, unknown>) {
    const commands =
      (params.commands as Array<{
        name: string;
        description: string;
        meta?: Record<string, unknown>;
      }>) || [];
    const wirePrompts = (params.prompts as V2WirePrompt[]) || [];
    const { prompts, skills } = partitionV2Prompts(wirePrompts);
    const tools =
      (params.tools as Array<{
        name: string;
        description: string;
        source: string;
      }>) || [];
    const mcpServers =
      (params.mcpServers as Array<{
        name: string;
        status: string;
        toolCount: number;
      }>) || [];

    this.broadcastStreamEvent({
      type: AgentEventType.CommandsUpdate,
      commands: commands.map((cmd) => {
        let description = cmd.description;
        if (cmd.name === 'tools' && tools.length > 0) {
          description = `${cmd.description} (${tools.length} available)`;
        } else if (cmd.name === 'mcp' && mcpServers.length > 0) {
          const running = mcpServers.filter(
            (s) => s.status === 'running'
          ).length;
          description = `${cmd.description} (${running}/${mcpServers.length} running)`;
        }
        return { name: cmd.name, description, meta: cmd.meta };
      }),
      mcpServers: mcpServers.length > 0 ? mcpServers : undefined,
    });
    this.broadcastStreamEvent({ type: AgentEventType.PromptsUpdate, prompts });
    this.broadcastStreamEvent({ type: AgentEventType.SkillsUpdate, skills });
    if (mcpServers.length > 0) {
      this.broadcastStreamEvent({
        type: AgentEventType.McpServersUpdate,
        servers: mcpServers,
      });
    }
    // V2 has no steering concept; consumers keep state.steering at [].
  }

  private handleMetadataUpdate(params: Record<string, unknown>) {
    const sessionId = params.sessionId as string | undefined;
    if (sessionId && sessionId !== this.sessionId) return;
    const percent = params.contextUsagePercentage as number | undefined;
    if (params.contextUsageInvalidated === true) {
      this.broadcastStreamEvent({
        type: AgentEventType.ContextUsage,
        percent: null,
      });
    } else if (percent != null) {
      this.broadcastStreamEvent({ type: AgentEventType.ContextUsage, percent });
    }
    const metering = params.meteringUsage as MeteringUsage[] | undefined;
    const durationMs = params.turnDurationMs as number | undefined;
    if (metering && metering.length > 0) {
      this.broadcastStreamEvent({
        type: AgentEventType.TurnSummary,
        meteringUsage: metering,
        turnDurationMs: durationMs,
      });
    }
    // Only emitted when the notification actually carries effort; a refusal-only
    // notification omits it and must not clear the effort chip.
    if ('effort' in params) {
      const effort = (params.effort as string | undefined) ?? null;
      this.broadcastStreamEvent({ type: AgentEventType.EffortUpdate, effort });
    }

    const refusal = params.refusal as
      | { category?: string; explanation?: string; recommendedModel?: string }
      | undefined;
    const stopReason = params.stopReason as string | undefined;
    if (refusal || stopReason === 'CONTENT_FILTERED') {
      logger.debug('[acp] model refusal', { stopReason, refusal });
      this.broadcastStreamEvent({
        type: AgentEventType.ModelRefusal,
        stopReason,
        category: refusal?.category,
        explanation: refusal?.explanation,
        recommendedModel: refusal?.recommendedModel,
      });
    }
  }

  private handleClearStatus() {
    logger.debug('Clear status received');
  }

  private handleCompactionStatus(params: Record<string, unknown>) {
    const status = params.status as { type: string; error?: string };
    const summary = params.summary as string | undefined;
    if (status) {
      this.broadcastStreamEvent({
        type: AgentEventType.CompactionStatus,
        status: status.type as 'started' | 'completed' | 'failed',
        error: status.error,
        summary,
      });
    }
  }

  private handleMcpServerInitFailure(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerInitFailure,
      serverName: (params.serverName as string) ?? '',
      error: (params.error as string) ?? '',
    });
  }

  private handleMcpOauthRequest(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpOauthRequest,
      serverName: (params.serverName as string) ?? '',
      oauthUrl: (params.oauthUrl as string) ?? '',
    });
  }

  private handleMcpServerInitialized(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.McpServerInitialized,
      serverName: (params.serverName as string) ?? '',
    });
  }

  protected handleMcpGovernanceDisabled(params: Record<string, unknown>) {
    const apiFailure = (params.apiFailure as boolean) ?? false;
    logger.warn('MCP governance disabled:', { apiFailure });
    this.broadcastStreamEvent({
      type: AgentEventType.McpGovernanceDisabled,
      apiFailure,
    });
  }

  protected handleWebToolsGovernanceDisabled(params: Record<string, unknown>) {
    const apiFailure = (params.apiFailure as boolean) ?? false;
    logger.warn('Web tools governance disabled:', { apiFailure });
    this.broadcastStreamEvent({
      type: AgentEventType.WebToolsGovernanceDisabled,
      apiFailure,
    });
  }

  protected handleRateLimitError(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.RateLimitError,
      message: (params.message as string) ?? '',
    });
  }

  protected handleAgentNotFound(params: Record<string, unknown>) {
    const skipped = parseRejectedAgentConfig(params.skipped);
    this.broadcastStreamEvent({
      type: AgentEventType.AgentNotFound,
      requestedAgent: (params.requestedAgent as string) ?? '',
      fallbackAgent: (params.fallbackAgent as string) ?? '',
      ...(skipped ? { skipped } : {}),
    });
  }

  protected handleAgentConfigError(params: Record<string, unknown>) {
    this.broadcastStreamEvent({
      type: AgentEventType.AgentConfigError,
      path: params.path as string | undefined,
      error: (params.error as string) ?? '',
    });
  }

  private handleSubagentListUpdate(params: Record<string, unknown>) {
    this.broadcastSubagentList(
      (params as any)?.subagents ?? [],
      (params as any)?.pendingStages ?? []
    );
  }

  protected handleAgentSwitched(params: Record<string, unknown>) {
    const p = params as {
      agentName: string;
      previousAgentName?: string;
      welcomeMessage?: string;
      model?: string;
    };
    this.broadcastStreamEvent({
      type: AgentEventType.AgentSwitched,
      agentName: p.agentName,
      previousAgentName: p.previousAgentName,
      welcomeMessage: p.welcomeMessage,
      model: p.model,
    });
  }

  protected handleExtSessionUpdate(params: Record<string, unknown>) {
    const decoded = decodeExtSessionUpdate(params);
    if (decoded) this.handleDecodedExtSessionUpdate(decoded);
  }

  protected handleDecodedExtSessionUpdate(
    envelope: ExtSessionUpdateEnvelope
  ): void {
    const { sessionId, update } = envelope;
    if (update.sessionUpdate === 'tool_call_chunk') {
      const isSubagentEvent = sessionId && sessionId !== this.sessionId;
      const kiroMeta = update.kiroMeta;
      const identity = normalizeToolCallTitle(update.title, kiroMeta?.toolName);
      const event: AgentStreamEvent = {
        type: AgentEventType.ToolCall,
        id: update.toolCallId,
        name: identity.name || update.title,
        origin: identity.origin,
        originalTitle: identity.originalTitle,
        kind: update.kind,
        args: {},
        sessionId: isSubagentEvent ? sessionId : undefined,
        ...(kiroMeta && { meta: { kiro: kiroMeta } }),
      };
      if (isSubagentEvent) this.broadcastMultiSession(sessionId, event);
      this.broadcastStreamEvent(event);
      return;
    }

    if (update.sessionUpdate === 'retry_warning') {
      logger.warn('Retry warning received:', update);
      this.broadcastStreamEvent({
        type: AgentEventType.RetryWarning,
        attempt: update.attempt,
        maxAttempts: update.maxAttempts,
        delaySecs: update.delaySecs,
        message: update.message,
      });
      return;
    }

    // Steering events (Rust engine only). The Rust engine emits the
    // `AgentExecution*` PascalCase discriminators on the
    // `_kiro.dev/session/update` ext channel, with `{ messageId, content }` on
    // queued/injected and `{ messageIds }` on cleared. Schema reference:
    //   Rust: crates/chat-cli-v2/src/agent/acp/extensions.rs::ExtSessionUpdate
    //
    // The KAS engine no longer uses this channel: it now emits the same
    // steering lifecycle as `session_info_update` kinds
    // (`steering_queued` / `steering_injected` / `steering_cleared`) handled in
    // `convertAcpUpdateToEvent`. Both paths map onto the same internal events.
    // Keep in sync with the kas-acp-client test fixture
    // (packages/tui/src/__tests__/kas-acp-client.test.ts).
    if (update.sessionUpdate === 'AgentExecutionUserMessageQueued') {
      this.broadcastStreamEvent({
        type: AgentEventType.SteeringQueued,
        message: update.content,
      });
      return;
    }

    if (update.sessionUpdate === 'AgentExecutionSteeringInjected') {
      this.broadcastStreamEvent({
        type: AgentEventType.SteeringConsumed,
        content: update.content,
      });
      return;
    }

    if (update.sessionUpdate === 'AgentExecutionUserMessageCleared') {
      this.broadcastStreamEvent({ type: AgentEventType.SteeringCleared });
    }
  }

  private handleGoalStatus(params: Record<string, unknown>) {
    const { state, iteration, maxIterations, message, elapsedSecs } =
      params as {
        state: string;
        iteration: number;
        maxIterations: number;
        message?: string;
        elapsedSecs?: number;
      };
    this.broadcastStreamEvent({
      type: AgentEventType.GoalStatus,
      state,
      iteration,
      maxIterations,
      message,
      elapsedSecs,
    });
  }

  // ── Shared session update → event conversion ──

  protected convertAcpUpdateToEvent(
    update: AcpSessionUpdate,
    notifSessionId?: string,
    sideEffectSink?: (event: AgentStreamEvent) => void
  ): AgentStreamEvent | null {
    const sessionStateKey = notifSessionId ?? this.sessionId ?? '';
    const isPrimarySession =
      notifSessionId === undefined || notifSessionId === this.sessionId;
    const emitSideEffect =
      sideEffectSink ?? ((event) => this.broadcastStreamEvent(event));
    const emitSynthesized =
      sideEffectSink ??
      ((event) => this.broadcastSynthesizedFailedToolCall(event));

    switch (update.sessionUpdate) {
      case 'user_message_chunk': {
        const kiroMeta = extractKiroMetaFromUpdate(update);
        return update.content.type === 'text'
          ? {
              type: AgentEventType.UserMessage,
              id: kiroMeta?.messageId ?? crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
              ...(kiroMeta && { meta: { kiro: kiroMeta } }),
            }
          : null;
      }

      case 'agent_message_chunk':
        switch (update.content.type) {
          case 'text': {
            const kiroMeta = extractKiroMetaFromUpdate(update);
            return {
              type: AgentEventType.Content,
              id: crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
              ...(kiroMeta && { meta: { kiro: kiroMeta } }),
            };
          }
          case 'image':
            return {
              type: AgentEventType.Content,
              id: crypto.randomUUID(),
              content: { type: ContentType.Image, image: update.content },
            };
          default:
            return null;
        }

      case 'agent_thought_chunk': {
        if (update.content.type === 'text') {
          // Carry _meta.kiro so a KAS subagent's reasoning is routed to its
          // subtask session by routeKasSubtaskEvent instead of bleeding into the
          // main agent's bufferedThinking. Mirrors the agent_message_chunk case
          // above — without it the subtask discriminator (agentSubtaskId) is lost.
          const kiroMeta = extractKiroMetaFromUpdate(update);
          return {
            type: AgentEventType.Thought,
            id: crypto.randomUUID(),
            content: { type: ContentType.Text, text: update.content.text },
            ...(kiroMeta && { meta: { kiro: kiroMeta } }),
          };
        }
        return null;
      }

      case 'tool_call': {
        const toolContent = (update.content ?? [])
          .filter((c) => c.type === 'diff')
          .map((c) => ({
            type: 'diff' as const,
            path: c.path,
            newText: c.newText ?? '',
            oldText: c.oldText ?? undefined,
          }));
        const locations = (update.locations ?? []).map((loc) => ({
          path: loc.path,
          line: loc.line ?? undefined,
        }));
        const kiroMeta = extractToolKiroMetaFromUpdate(update);
        const identity = normalizeToolCallTitle(
          update.title,
          kiroMeta?.toolName
        );
        return {
          type: AgentEventType.ToolCall,
          id: update.toolCallId,
          name: kiroMeta?.pipeline
            ? 'orchestrate_subagent'
            : identity.name || 'unknown',
          origin: kiroMeta?.pipeline ? 'builtin' : identity.origin,
          originalTitle: kiroMeta?.pipeline
            ? undefined
            : identity.originalTitle,
          kind: update.kind ?? undefined,
          args: (update.rawInput as Record<string, unknown>) ?? {},
          toolContent: toolContent.length > 0 ? toolContent : undefined,
          locations: locations.length > 0 ? locations : undefined,
          ...(kiroMeta && { meta: { kiro: kiroMeta } }),
        };
      }

      case 'tool_call_update': {
        const kiroMetaUpdate = extractToolKiroMetaFromUpdate(update);
        if (update.status === ToolCallStatus.Completed) {
          const diffContent = (update.content ?? [])
            .filter((c) => c.type === 'diff')
            .map((c) => ({
              type: 'diff' as const,
              path: c.path,
              newText: c.newText ?? '',
              oldText: c.oldText ?? undefined,
            }));
          // KAS emits the subagent's final output as a Completed-only
          // tool_call_update (no preceding `tool_call`) with the text in
          // rawInput.response and a null rawOutput. Without a `tool_call` the
          // store never creates a message carrying that response, so it renders
          // nowhere. Synthesize the missing ToolCall from rawInput so the
          // response harvest (parseSummaryTool reads msg.content) sees it.
          const rawInput = update.rawInput as
            | Record<string, unknown>
            | undefined;
          if (rawInput && typeof rawInput.response === 'string') {
            const identity = normalizeToolCallTitle(
              update.title ?? undefined,
              kiroMetaUpdate?.toolName
            );
            const synthesized: AgentStreamEvent = {
              type: AgentEventType.ToolCall,
              id: update.toolCallId,
              name: identity.name || 'unknown',
              origin: identity.origin,
              originalTitle: identity.originalTitle,
              kind: update.kind ?? undefined,
              args: rawInput,
              synthesized: true,
              ...(kiroMetaUpdate && { meta: { kiro: kiroMetaUpdate } }),
            };
            if (notifSessionId && notifSessionId !== this.sessionId) {
              synthesized.sessionId = notifSessionId;
            }
            emitSynthesized(synthesized);
          }
          return {
            type: AgentEventType.ToolCallFinished,
            id: update.toolCallId,
            result: {
              status: 'success',
              output: unwrapKasMcpOutput(update.rawOutput),
            },
            toolContent: diffContent.length > 0 ? diffContent : undefined,
            ...(kiroMetaUpdate && { meta: { kiro: kiroMetaUpdate } }),
          };
        }
        if (update.status === ToolCallStatus.Failed) {
          // If the backend rejected the tool before execution, no `tool_call`
          // notification was sent. Synthesize one from rawInput so the TUI
          // can render the tool name and attempted arguments.
          //
          // A genuine rejected-before-exec tool always carries a `title`
          // (V2 acp_agent get_tool_title; KAS model tools emit PendingAction
          // first). A title-LESS failed update on the MAIN session is KAS's
          // orchestrate-subagent re-reading a finished subagent's referenced
          // files at the parent executionId — synthesizing it here attributes
          // it to the main agent and leaks it into the main transcript (the
          // subagent tool "bleed"). Only failures leak, since the success
          // branch synthesizes solely on rawInput.response. Skip synthesis for
          // that case; a subagent-stamped update (notifSessionId differs) still
          // synthesizes so it routes to the subagent surface.
          const isUnattributableOrphanRead =
            !update.title &&
            (!notifSessionId || notifSessionId === this.sessionId);
          if (update.rawInput !== undefined && !isUnattributableOrphanRead) {
            const identity = normalizeToolCallTitle(
              update.title ?? undefined,
              kiroMetaUpdate?.toolName
            );
            const synthesized: AgentStreamEvent = {
              type: AgentEventType.ToolCall,
              id: update.toolCallId,
              name: identity.name || 'unknown',
              origin: identity.origin,
              originalTitle: identity.originalTitle,
              kind: update.kind ?? undefined,
              args: (update.rawInput as Record<string, unknown>) ?? {},
              ...(kiroMetaUpdate && { meta: { kiro: kiroMetaUpdate } }),
              synthesized: true,
            };
            // Stamp the originating subagent session so the store resolves the
            // stage's agentName instead of falling back to the MAIN agent.
            // When a Failed tool_call_update is the FIRST event the store sees
            // for this toolCallId (parse error / permission-denied / hook-
            // rejected — the only paths that carry rawInput here, since the
            // backend never sent an initial `tool_call`), an unstamped synthesized
            // event resolves to the main agent. lite's isInnerSubagentTool then
            // can't hide it, leaking the rejected stage tool into the main
            // scrollback / static flush. Mirrors handleSessionUpdate's guard at
            // ~1722 so a genuine main-agent rejected-before-exec tool is
            // unaffected (notifSessionId absent or === this.sessionId → no stamp).
            if (notifSessionId && notifSessionId !== this.sessionId) {
              synthesized.sessionId = notifSessionId;
            }
            emitSynthesized(synthesized);
          }
          // Prefer a descriptive error from the content block; fall back to
          // rawOutput, then a generic message.
          let errorText: string | undefined;
          const failedContent = update.content;
          if (Array.isArray(failedContent)) {
            const textItem = failedContent.find(
              (item) => item.type === 'content' && item.content.type === 'text'
            );
            if (
              textItem &&
              textItem.type === 'content' &&
              textItem.content.type === 'text'
            ) {
              errorText = textItem.content.text;
            }
          }
          if (!errorText && typeof update.rawOutput === 'string') {
            errorText = update.rawOutput;
          }
          // Some agents send the user-facing error in session_info_update's
          // displayError meta rather than on the tool_call_update payload
          // itself. Captured upstream into pendingDisplayError; consumed
          // here as a final fallback so the failure surfaces with text
          // instead of a bare `✗ failed` chip. Always cleared after read
          // so a stale value can't bleed into the next tool failure.
          const pendingDisplayError =
            this.pendingDisplayErrors.get(sessionStateKey);
          if (!errorText && pendingDisplayError) {
            errorText = pendingDisplayError;
          }
          this.pendingDisplayErrors.delete(sessionStateKey);
          // Recover user-cancellation from the canonical reason string the
          // V2 Rust side tunnels through the failure content (acp_agent.rs
          // ToolCallFinished arm for ToolCallResult::Cancelled). ACP's
          // ToolCallStatus only has Completed/Failed, so without this
          // detection a tool the user interrupted lands as a generic FAILED
          // chip — including the parent agent_crew tool when the user hits
          // Esc mid-pipeline. `isUserCancelledReason` localizes the V2 string
          // coupling (no-op for KAS); mirrors `isUserDeniedReason` in the
          // app-store.ts ToolCallFinished handler.
          if (isUserCancelledReason(errorText)) {
            return {
              type: AgentEventType.ToolCallFinished,
              id: update.toolCallId,
              result: { status: 'cancelled' },
            };
          }
          return {
            type: AgentEventType.ToolCallFinished,
            id: update.toolCallId,
            result: {
              status: 'error',
              error: errorText || 'Tool execution failed',
            },
            ...(kiroMetaUpdate && { meta: { kiro: kiroMetaUpdate } }),
          };
        }

        // content is a Vec<ToolCallContent> — a tagged enum where the Content
        // variant wraps a ContentBlock: { type: "content", content: { type: "text", text: "..." } }
        const contentArray = update.content;
        let firstText = '';
        if (Array.isArray(contentArray)) {
          const textItem = contentArray.find(
            (item) => item.type === 'content' && item.content.type === 'text'
          );
          if (
            textItem &&
            textItem.type === 'content' &&
            textItem.content.type === 'text'
          ) {
            firstText = textItem.content.text ?? '';
          }
        }

        return {
          type: AgentEventType.ToolCallUpdate,
          id: update.toolCallId,
          content: { type: ContentType.Text, text: firstText },
          ...(kiroMetaUpdate && { meta: { kiro: kiroMetaUpdate } }),
        };
      }

      case 'available_commands_update': {
        const cu = update as any;
        // KAS sends prompts/skills/steering as commands tagged with
        // _meta.kiro.type. `_meta.kiro.scope` (and `serverName` when
        // scope is `mcp`) carry the source classification; default
        // is `workspace` when omitted. Partition into typed slices
        // and broadcast all three update events; remaining commands
        // flow through CommandsUpdate.
        const allCommands = (cu.availableCommands || []) as Array<{
          name: string;
          description?: string;
          _meta?: {
            kiro?: {
              type?: string;
              scope?: string;
              serverName?: string;
              path?: string;
              telemetryId?: string;
            };
            arguments?: Array<{
              name: string;
              description?: string;
              required?: boolean;
            }>;
          };
        }>;
        const prompts: PromptEntry[] = [];
        const skills: SkillEntry[] = [];
        const steering: SteeringEntry[] = [];
        const otherCommands: typeof allCommands = [];
        for (const cmd of allCommands) {
          const kiroMeta = cmd._meta?.kiro;
          const kind = kiroMeta?.type;
          const scope = kiroMeta?.scope;
          const path = kiroMeta?.path;
          switch (kind) {
            case 'prompt': {
              const source: PromptSource =
                scope === 'mcp' && kiroMeta?.serverName
                  ? { kind: 'mcp', serverName: kiroMeta.serverName }
                  : scope === 'global'
                    ? { kind: 'global', ...(path ? { path } : {}) }
                    : { kind: 'workspace', ...(path ? { path } : {}) };
              prompts.push({
                name: cmd.name,
                description: cmd.description,
                ...(kiroMeta?.telemetryId
                  ? { telemetryId: kiroMeta.telemetryId }
                  : {}),
                arguments: cmd._meta?.arguments ?? [],
                source,
              });
              break;
            }
            case 'skill': {
              const source: SkillSource =
                scope === 'global'
                  ? { kind: 'global', ...(path ? { path } : {}) }
                  : { kind: 'workspace', ...(path ? { path } : {}) };
              skills.push({
                name: cmd.name,
                description: cmd.description,
                ...(kiroMeta?.telemetryId
                  ? { telemetryId: kiroMeta.telemetryId }
                  : {}),
                source,
              });
              break;
            }
            case 'steering': {
              if (HIDDEN_STEERING_COMMANDS.has(cmd.name)) {
                break;
              }
              const source: SteeringSource =
                scope === 'global'
                  ? { kind: 'global', ...(path ? { path } : {}) }
                  : { kind: 'workspace', ...(path ? { path } : {}) };
              steering.push({
                name: cmd.name,
                description: cmd.description,
                ...(kiroMeta?.telemetryId
                  ? { telemetryId: kiroMeta.telemetryId }
                  : {}),
                source,
              });
              break;
            }
            case 'agent':
            case 'mode':
            case 'custom-agent':
              // Agents/modes are handled via the modes cache (session/new
              // response), not as slash commands. `custom-agent` entries
              // (e.g. KAS's `context-gatherer`, `general-task-execution`,
              // and user/workspace agent profiles) are delegate-a-task
              // subagents — not top-level commands, and not switchable modes
              // in the cache the Layer-2 override cross-references. Drop them
              // all here so they don't clutter the autocomplete menu.
              break;
            default:
              otherCommands.push(cmd);
          }
        }
        emitSideEffect({
          type: AgentEventType.PromptsUpdate,
          prompts,
        });
        emitSideEffect({
          type: AgentEventType.SkillsUpdate,
          skills,
        });
        emitSideEffect({
          type: AgentEventType.SteeringUpdate,
          steering,
        });
        return {
          type: AgentEventType.CommandsUpdate,
          commands: otherCommands.map((cmd) => {
            const telemetryId = cmd._meta?.kiro?.telemetryId;
            return {
              name: cmd.name,
              description: cmd.description ?? '',
              meta: {
                ...cmd._meta,
                ...(telemetryId ? { telemetryId } : {}),
              },
            };
          }),
        };
      }

      // KAS-specific update types.  `current_mode_update` and
      // `config_option_update` are intercepted in KasAcpClient.wireSessionListeners
      // (they update local caches before this switch runs).  The rest are
      // not yet mapped to TUI events.
      // (`agent_thought_chunk` is handled above — see ThinkingDisplay pipeline.)
      case 'session_info_update': {
        const meta = extractKasSessionInfoMeta(update);
        // Some agents emit the user-facing error for an in-flight tool call
        // here rather than on the tool_call_update payload. Captured into
        // pendingDisplayError so tool_call_update Failed can fall back to it
        // when its own error fields are empty.
        if (meta?.displayError?.message) {
          this.pendingDisplayErrors.set(
            sessionStateKey,
            meta.displayError.message
          );
        }
        // The session's bound-repo set, pushed when the sandbox attaches or
        // detaches repos mid-session (KAS relay of the sandbox agent's
        // notification; pending fleet rollout). Checked on the raw meta key
        // rather than a `kind` so it rides along with whichever variant KAS
        // stamps it on. Side-effect broadcast, like `context_usage`.
        if ('repositories' in ((meta ?? {}) as Record<string, unknown>)) {
          this.broadcastStreamEvent({
            type: AgentEventType.SessionRepositoriesUpdate,
            // The key is present, so this is an explicit report: a malformed
            // value degrades to "zero repos", not "not reported".
            repositories:
              parseSessionRepositories(
                (meta as Record<string, unknown>).repositories
              ) ?? [],
          });
        }
        if (meta?.kind === 'turn_completion') {
          const completion = normalizeKasTurnCompletion(meta);
          if (!completion) {
            // Nothing to show. Returning null keeps the chip cleared.
            return null;
          }
          if (completion.contextUsagePercentage != null) {
            emitSideEffect({
              type: AgentEventType.ContextUsage,
              percent: completion.contextUsagePercentage,
            });
          }
          if (
            completion.meteringUsage.length === 0 &&
            completion.turnDurationMs == null
          ) {
            return null;
          }
          return {
            type: AgentEventType.TurnSummary,
            meteringUsage: completion.meteringUsage,
            turnDurationMs: completion.turnDurationMs,
          };
        }
        if (meta?.kind === 'summarization_completed') {
          if (!isPrimarySession) {
            return {
              type: AgentEventType.CompactionStatus,
              status: 'completed',
              summary: extractKasSummarizationSummary(meta),
            };
          }
          const attemptId = this.consumeCompactSummaryAttemptId();
          if (attemptId === null) return null;
          return {
            type: AgentEventType.CompactionStatus,
            status: 'completed' as const,
            attemptId,
            summary: extractKasSummarizationSummary(meta),
          };
        }
        if (meta?.kind === 'summarization_started') {
          if (!isPrimarySession) {
            return {
              type: AgentEventType.CompactionStatus,
              status: 'started',
            };
          }
          if (
            this.isCompactCompletionAttemptPending(
              this.compactCompletionAttemptId
            )
          ) {
            return null;
          }
          this.externalCompactInProgress = true;
          return {
            type: AgentEventType.CompactionStatus,
            status: 'started' as const,
          };
        }
        if (meta?.kind === 'summarization_failed') {
          if (!isPrimarySession) {
            return {
              type: AgentEventType.CompactionStatus,
              status: 'failed',
              error: extractKasError(meta),
            };
          }
          const attemptId = this.consumeCompactSummaryAttemptId();
          if (attemptId === null) return null;
          return {
            type: AgentEventType.CompactionStatus,
            status: 'failed' as const,
            attemptId,
            error: extractKasError(meta),
          };
        }
        if (meta?.kind === 'context_usage' || meta?.contextUsage) {
          const percent = normalizeKasContextUsagePercentage(meta);
          if (typeof percent === 'number') {
            emitSideEffect({
              type: AgentEventType.ContextUsage,
              percent,
            });
          }
          if (meta?.breakdown) {
            emitSideEffect({
              type: AgentEventType.ContextBreakdownUpdate,
              breakdown: meta.breakdown as ContextBreakdownData,
            });
          }
        }
        if (
          meta?.kind === 'user_message_id_assigned' &&
          typeof (meta as any)?.userMessageId === 'string'
        ) {
          emitSideEffect({
            type: AgentEventType.KasMessageIdAssigned,
            kasMessageId: (meta as any).userMessageId,
          });
        }
        // Mid-turn steering queue lifecycle (KAS engine). KAS rides the
        // standard `session_info_update` channel via the typed
        // `KiroSessionInfoUpdate` union (snake_case `kind`), unlike the Rust
        // engine which uses PascalCase discriminators on the
        // `_kiro.dev/session/update` ext channel (see `handleExtSessionUpdate`).
        // These map onto the same internal steering events. They are
        // side-effect broadcasts (like `context_usage`), so broadcast and
        // return null rather than returning the event.
        if (meta?.kind === 'steering_queued') {
          // Workflow notifications are model-facing wakeups, not user turns.
          // KAS supplies notificationSeverity specifically so clients can
          // keep them out of the steering tray and conversation scrollback.
          if (isKasNotificationSteering(meta)) return null;
          const steerBuffer =
            this.kasSteerBuffers.get(sessionStateKey) ??
            new Map<string, string>();
          steerBuffer.set(meta.messageId ?? '', meta.content ?? '');
          this.kasSteerBuffers.set(sessionStateKey, steerBuffer);
          emitSideEffect({
            type: AgentEventType.SteeringQueued,
            message: [...steerBuffer.values()].join('\n\n'),
          });
          return null;
        }
        if (meta?.kind === 'steering_injected') {
          if (isKasNotificationSteering(meta)) return null;
          this.kasSteerBuffers.delete(sessionStateKey);
          emitSideEffect({
            type: AgentEventType.SteeringConsumed,
            content: meta.content ?? '',
          });
          return null;
        }
        if (meta?.kind === 'steering_cleared') {
          this.kasSteerBuffers.delete(sessionStateKey);
          emitSideEffect({ type: AgentEventType.SteeringCleared });
          return null;
        }
        // Turn boundaries must reach clients that did not submit the turn.
        if (meta?.kind === 'turn_start') {
          this.broadcastStreamEvent({ type: AgentEventType.TurnStart });
          return null;
        }
        if (meta?.kind === 'turn_end') {
          this.broadcastStreamEvent({
            type: AgentEventType.TurnEnd,
            ...(typeof meta.stopReason === 'string'
              ? { stopReason: meta.stopReason }
              : {}),
          });
          return null;
        }
        logger.debug(
          'KAS session update (not yet mapped):',
          update.sessionUpdate
        );
        return null;
      }
      case 'config_option_update':
      case 'plan':
      case 'usage_update':
        logger.debug(
          'KAS session update (not yet mapped):',
          update.sessionUpdate
        );
        return null;

      case 'current_mode_update': {
        // Handled by KasAcpClient.wireSessionListeners with dedup logic.
        return null;
      }

      default:
        // Steering events arrive either as `session_info_update` kinds
        // (KAS engine — handled in the `session_info_update` case above) or as
        // `AgentExecution*` discriminators on the `_kiro.dev/session/update`
        // ext channel (Rust engine — see `handleExtSessionUpdate`). If we reach
        // this default branch we've received something neither ACP nor KAS has
        // taught us to render yet — log and drop.
        logger.debug(
          'Unhandled session update type:',
          (update as any).sessionUpdate
        );
        return null;
    }
  }

  // ── Shared permission handling ──

  protected handlePermissionRequest(
    params: acp.RequestPermissionRequest,
    eventSink: (event: ApprovalRequestEvent) => void = (event) =>
      this.broadcastStreamEvent(event)
  ): Promise<acp.RequestPermissionResponse> {
    return new Promise<acp.RequestPermissionResponse>((resolve) => {
      const meta = params._meta as any;
      const rawToolCall = (params.toolCall ?? {}) as {
        toolCallId?: string;
        title?: string;
        rawInput?: unknown;
        name?: string;
        kind?: string;
        origin?: ToolCallOrigin;
      };
      const canonicalName =
        rawToolCall.name ?? meta?.kiro?.toolName ?? meta?.kiro?.toolId;
      const identity = normalizeToolCallTitle(rawToolCall.title, canonicalName);
      const event: ApprovalRequestEvent = {
        type: AgentEventType.ApprovalRequest,
        value: {
          sessionId: (params as any).sessionId as string | undefined,
          originSessionId: (params as any).originSessionId as
            | string
            | undefined,
          toolCall: {
            toolCallId: rawToolCall.toolCallId || '',
            title: rawToolCall.title,
            rawInput: rawToolCall.rawInput,
            name: rawToolCall.name ?? identity.name,
            kind: rawToolCall.kind,
            origin: rawToolCall.origin ?? identity.origin,
          },
          permissionOptions: (params.options || []).map((opt) => ({
            kind: opt.kind as ApprovalOptionId,
            name: opt.name,
            optionId: opt.optionId,
          })),
          trustOptions: meta?.trustOptions,
          consentContext: meta?.kiro?.consent,
          toolId: meta?.kiro?.toolId,
          resolve: (userResponse: {
            outcome: string;
            optionId?: string;
            _meta?: unknown;
          }) => {
            resolve(
              userResponse.outcome === 'selected'
                ? {
                    _meta: userResponse._meta as
                      | Record<string, unknown>
                      | undefined,
                    outcome: {
                      outcome: 'selected' as const,
                      optionId: userResponse.optionId,
                      // V2 Rust SDK reads _meta from the outcome variant (selected.meta)
                      _meta: userResponse._meta,
                    } as acp.RequestPermissionResponse['outcome'],
                  }
                : { outcome: { outcome: 'cancelled' as const } }
            );
          },
        },
      };
      eventSink(event);
    });
  }

  // ── Shared session update routing ──

  protected handleSessionUpdate(params: acp.SessionNotification): void {
    const { update } = params;
    if (!update) return;
    const notifSessionId = (params as any).sessionId as string | undefined;
    const isSubagentEvent = notifSessionId && notifSessionId !== this.sessionId;
    const event = this.convertAcpUpdateToEvent(update, notifSessionId);
    if (!event) return;

    // Per-engine telemetry observation off the single shared conversion. The
    // base is a no-op; RustAcpClient (V2) overrides it to drive its tool-call
    // observer. KAS does not route through this method (it wires its own
    // per-session listeners), so this only fires on the V2 path. We restrict
    // it to MAIN-session tool calls (sub-agent tool calls are host-internal and
    // have no is_subagent dimension on kiro_cli_tool_call_total to filter them out).
    if (!isSubagentEvent) {
      this.observeTurnTelemetry(event);
    }

    if (isSubagentEvent) {
      this.broadcastMultiSession(notifSessionId, event);
      const isToolEvent =
        event.type === AgentEventType.ToolCall ||
        event.type === AgentEventType.ToolCallUpdate ||
        event.type === AgentEventType.ToolCallFinished;
      if (isToolEvent) {
        // Stamp the originating session on the standard `tool_call` event so
        // the main store's ToolCall handler resolves the stage's agentName
        // instead of falling back to the main agent. `convertAcpUpdateToEvent`
        // can't see the notification's sessionId, so the standard `tool_call`
        // path (unlike `tool_call_chunk`) would otherwise broadcast it here
        // with sessionId undefined — the store then stamps the stage tool
        // incorrectly
        // as a MAIN-agent tool, leaking it into the lite chat log / static
        // flush (scrollback wedges) and dropping the parent subagent out of
        // the live region's active-tool set (spinner stalls). ToolCallUpdate /
        // ToolCallFinished are matched by id in the store, so they don't need
        // it.
        if (event.type === AgentEventType.ToolCall) {
          event.sessionId = notifSessionId;
        }
        this.broadcastStreamEvent(event);
      }
    } else {
      this.broadcastStreamEvent(event);
    }
  }

  /**
   * Telemetry observation hook off {@link handleSessionUpdate}'s single shared
   * event conversion. No-op in the base; RustAcpClient overrides it to feed its
   * V2 tool-call observer. Kept as a hook (rather than re-converting
   * in the subclass) because the converter has broadcast side effects for
   * failed-before-exec tools — re-running it would double-emit those.
   */
  protected observeTurnTelemetry(_event: AgentStreamEvent): void {
    // no-op (V2 overrides; KAS uses its own listener flow)
  }
}
