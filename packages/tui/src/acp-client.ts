import * as acp from '@agentclientprotocol/sdk';
import { KiroClient } from '@kiro/client';
import type { Stream } from '@kiro/client';
// Spec workflow types are sourced from the shared ACP type covenant so the
// TUI, KAS, and any other ACP client speak the same contract for the
// `_kiro/spec/*` extension methods.
import type {
  SpecInvokeRequest,
  SpecInvokeResponse,
  SpecResolveSessionRequest,
  SpecResolveSessionResponse,
} from '@kiro/acp-type-covenant';
import { logger } from './utils/logger';
import {
  getTelemetryIdentity,
  isTelemetryEnabled,
} from './utils/telemetry-identity';
import { buildKasSettings } from './utils/kas-settings';
import { webToolsGovernanceFromState } from './utils/governance-state';
import { readCliSettings, updateCliSetting } from './utils/cli-settings';
import { maybeWrapStreamWithRecorder } from './acp-recorder';
import { createGetAccessTokenCapability } from './auth/acp-auth-callback';
import { createOpenExternalUrlCapability } from './capabilities/open-external-url';
import { createSecretStorageCapabilities } from './capabilities/secret-storage';
import { spawn, type ChildProcess } from 'node:child_process';
import type {
  ChatSlashCommandTelemetryPayload,
  SessionClient,
} from './types/session-client';
import type { ProcessHealthSnapshot } from './utils/process-health-collector';
import type { ModeChangedNotification } from './types/generated/chat-cli';
import {
  AgentEventType,
  ContentType,
  ApprovalOptionId,
  ToolCallStatus,
  type AgentStreamEvent,
  type KiroMeta,
  type MeteringUsage,
} from './types/agent-events';
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
} from './types/commands';
import type { ListSessionsResponse } from './types/session-client';
import type {
  HookInfo,
  McpServerInfo,
  ContextBreakdownData,
} from './stores/app-store';
import type {
  KasContextShowResponse,
  KasContextMutationResponse,
} from './types/session-client';

import packageJson from '../package.json';
import { KAS_COMMANDS } from './kas-commands';
import { resolveAgentEngine } from './agent-engine';
import { readClipboardImage } from './utils/clipboard-image';
import { formatEffort } from './utils/string';
import { getAgentDisplayName } from './utils/agentColors';
import { emitKasTelemetry } from './utils/kas-telemetry-cli';

const TUI_VERSION: string = packageJson.version;

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
 * Strip the `@serverName/` prefix from KAS MCP tool titles.
 * KAS sends titles like "@test-mock/echo"; V2 sends just "echo".
 */
export function stripMcpTitlePrefix(
  title: string | undefined
): string | undefined {
  if (!title) return title;
  const match = title.match(/^@[^/]+\/(.+)$/);
  return match ? match[1] : title;
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
export type KasAcpSessionUpdate = AcpSessionUpdate & {
  _meta?: { kiro?: KiroMeta } | null;
};

/** Type-safe `_meta.kiro` accessor for ACP session updates. */
function extractKiroMetaFromUpdate(
  update: AcpSessionUpdate
): KiroMeta | undefined {
  return (update as KasAcpSessionUpdate)._meta?.kiro;
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

const EXT_METHODS = {
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
  SESSION_ACTIVITY: 'kiro.dev/session/activity',
  SESSION_LIST_UPDATE: 'kiro.dev/session/list_update',
  INBOX_NOTIFICATION: 'kiro.dev/session/inbox_notification',
  SESSION_LIST: 'session/list',
  SESSION_SPAWN: 'session/spawn',
  SESSION_TERMINATE: 'session/terminate',
  SESSION_ATTACH: 'session/attach',
  MESSAGE_SEND: 'message/send',
  SESSION_STEER: 'session/steer',
  SESSION_STEER_CLEAR: 'session/steer/clear',
  AGENT_SWITCHED: 'kiro.dev/agent/switched',
  SESSION_UPDATE: 'kiro.dev/session/update',
  GOAL_STATUS: 'kiro.dev/goal/status',
} as const;

/** Subset of ACP's SessionModeState that we cache client-side.  Used for the
 *  /agent command, which is composed from the modes advertised on
 *  session/new and session/load responses (and kept in sync via
 *  current_mode_update notifications) rather than a custom extension
 *  method. */
type CachedModesState = {
  availableModes: Array<{
    id: string;
    name: string;
    description?: string | null;
    _meta?: Record<string, unknown> | null;
  }>;
  currentModeId?: string;
};

/**
 * Agents (KAS modes) that should never surface in the `/agent` menu or any
 * agent listing, keyed by their KAS mode id.
 *
 * Each entry MUST carry a comment explaining why it is being removed so that
 * future maintainers know whether the exclusion is still warranted.
 *
 * Note: ids are compared after `fromKasModeId` normalization, i.e. the same
 * form stored in `modesState.availableModes`. Denial is scoped to
 * bundled agents (see `isAgentDenied`) so a user/workspace config that
 * deliberately defines an agent under one of these ids is preserved.
 */
const AGENT_DENYLIST: Record<string, string> = {
  // The semantic reviewer is an internal review-only subagent invoked
  // programmatically (e.g. by the autonomous planner's review loop). It is
  // not a general-purpose conversational agent, so exposing it in the
  // user-facing `/agent` picker is confusing and lets users switch into a
  // mode that isn't meant to drive an interactive session. KAS advertises it
  // under the wire mode id `semantic_reviewer`.
  semantic_reviewer:
    'Internal review-only subagent; not a user-selectable conversational agent.',

  // The autonomous agent is a bundled KAS mode that drives long-running,
  // self-directed execution rather than an interactive conversational
  // session. Surfacing it in the user-facing `/agent` picker lets users
  // switch into a mode that isn't meant for normal chat, so we hide it.
  // KAS advertises it under the wire mode id `autonomous` (passed through
  // unchanged by `fromKasModeId`).
  autonomous:
    'Bundled self-directed execution mode; not a user-selectable conversational agent.',
};

/**
 * Builtin KAS modes hidden from the TUI agent picker and all derived
 * listings. Unlike AGENT_DENYLIST (which only targets *bundled* agents and
 * preserves user/workspace overrides), these are reserved builtin mode ids a
 * user cannot redefine, so they are filtered unconditionally.
 *
 * `quick-spec`: the heavyweight spec-generation workflow (formerly
 * `quick-plan`). The TUI surfaces interactive planning via the `plan` mode
 * (reached through /plan and Shift+Tab); quick-spec is not offered as a
 * directly selectable agent.
 */
const HIDDEN_BUILTIN_MODES = new Set<string>(['quick-spec']);

/**
 * Steering commands hidden from the TUI slash-command menu. KAS ships a
 * builtin `quick-spec` steering document that registers `/quick-spec` to
 * trigger the spec-generation workflow inline. Product does not want
 * quick-spec exposed in the TUI (the picker mode is hidden too), so the
 * inline command is dropped from autocomplete as well.
 */
const HIDDEN_STEERING_COMMANDS = new Set<string>(['quick-spec']);

/**
 * Whether the given mode should be hidden from agent listings.
 *
 * The denylist targets KAS's *bundled* agents only. A user- or
 * workspace-defined agent that happens to share a denylisted id (e.g. a
 * workspace `semantic_reviewer` that overrides the bundled one) is
 * intentionally left visible — the user opted into defining it, so we must
 * not silently drop it. Modes with no source metadata are treated as
 * non-bundled and therefore never denied.
 */
function isAgentDenied(mode: {
  id: string;
  _meta?: Record<string, unknown> | null;
}): boolean {
  if (!Object.prototype.hasOwnProperty.call(AGENT_DENYLIST, mode.id)) {
    return false;
  }
  return getModeSource(mode._meta) === 'bundled';
}

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

type SessionResult = {
  sessionId: string;
  currentModel?: { id: string; name: string };
  currentAgent?: { name: string; welcomeMessage?: string };
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
    summary?: { conversationSummary?: string };
  };
  contextUsage?: { usagePercentage?: number };
  usagePercentage?: number;
  breakdown?: unknown;
  promptTurnSummaries?: KasPromptTurnSummary[];
  tokenUsage?: unknown;
  usage?: unknown;
  metrics?: unknown;
  elapsedTime?: unknown;
  status?: unknown;
};

type KasTurnCompletionTelemetryPayload = {
  sessionId?: string;
  modelId?: string;
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

function extractKasSessionInfoMeta(
  update: AcpSessionUpdate
): KasSessionInfoMeta | undefined {
  return (update as { _meta?: { kiro?: KasSessionInfoMeta } })._meta?.kiro;
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

function normalizeKasTurnCompletion(
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
  const turnDurationMs =
    typeof meta.elapsedTime === 'number' ? meta.elapsedTime : undefined;
  const contextUsagePercentage = normalizeKasContextUsagePercentage(meta);
  const tokenCounts = normalizeKasTurnTokenCounts(meta);
  const status = normalizeKasTurnCompletionStatus(meta.status);
  const usedTools = normalizeKasUsedTools(meta);
  if (
    meteringUsage.length === 0 &&
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
function buildStdioStreams(agentProcess: ChildProcess) {
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
function toAgentProcess(proc: ChildProcess): AgentProcess {
  // stdin/stdout/stderr are set synchronously on spawn and never change,
  // so plain property reads are sufficient (no getter indirection).
  return {
    stdin: proc.stdin,
    stdout: proc.stdout,
    stderr: proc.stderr,
    kill: (signal) => proc.kill(signal),
    onExit(listener) {
      proc.once('exit', listener);
      return () => {
        proc.off('exit', listener);
      };
    },
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

// ─── KAS model config extraction (ACP Session Config Options) ────────
//
// KAS exposes model selection through ACP's standard Session Config
// Options API, not through the (Rust-backend-specific) `models` field.
// The model option appears as:
//   { type: 'select', id: 'model', category: 'model',
//     currentValue: <id>, options: [{value, name, description?}, ...] }
//
// The shape of a flat SessionConfigSelectOption — matches ACP SDK 0.19.2.
interface ModelOption {
  value: string;
  name: string;
  description?: string;
}

/** Find the `category: 'model'` entry in a KAS configOptions array. */
function findModelConfigOption(
  configOptions: unknown
): { currentValue?: string; options: ModelOption[] } | undefined {
  if (!Array.isArray(configOptions)) return undefined;
  for (const opt of configOptions as Array<Record<string, unknown>>) {
    if (opt.category !== 'model' || opt.type !== 'select') continue;
    // `options` may be flat (SessionConfigSelectOption[]) or grouped
    // (SessionConfigSelectGroup[]). KAS currently emits flat; we only
    // support flat here. Grouped options simply yield an empty list,
    // which surfaces as "No options available" in the TUI.
    const raw = Array.isArray(opt.options) ? opt.options : [];
    const options = raw
      .filter((o: any): o is Record<string, unknown> => {
        return (
          typeof o === 'object' &&
          o !== null &&
          typeof (o as any).value === 'string' &&
          typeof (o as any).name === 'string'
        );
      })
      .map((o: any) => ({
        value: o.value as string,
        name: o.name as string,
        description:
          typeof o.description === 'string' ? o.description : undefined,
      }));
    return {
      currentValue:
        typeof opt.currentValue === 'string' ? opt.currentValue : undefined,
      options,
    };
  }
  return undefined;
}

/** Extract the currently selected model as `{id, name}` from configOptions. */
function extractModelFromConfigOptions(
  configOptions: unknown
): { id: string; name: string } | undefined {
  const modelOpt = findModelConfigOption(configOptions);
  if (!modelOpt?.currentValue) return undefined;
  const match = modelOpt.options.find((o) => o.value === modelOpt.currentValue);
  return match ? { id: match.value, name: match.name } : undefined;
}

/**
 * Extract the current effort level (e.g. "low", "medium", "high", "xhigh")
 * from a KAS configOptions array. KAS exposes this as a `select` with
 * `id: 'effortLevel'` (category: 'thought_level') only when the active
 * model declares an effortLevels schema. Returns null when the option is
 * absent or when its currentValue is not a string — both signal "no
 * effort chip" to the TUI.
 *
 * V2 surfaces the same value through `_kiro.dev/metadata.effort` per turn;
 * KAS surfaces it as session-level state attached to model config. We
 * normalize both paths into the existing `EffortUpdate` event so the
 * prompt-bar chip renders uniformly.
 */
function extractEffortFromConfigOptions(configOptions: unknown): string | null {
  const effortOpt = findEffortConfigOption(configOptions);
  return effortOpt?.currentValue ?? null;
}

/** A flat effort-level option as advertised by KAS (`{ value, name }`). */
interface EffortOption {
  value: string;
  name: string;
}

/**
 * Find the `id: 'effortLevel'` select entry in a KAS configOptions array,
 * returning its `currentValue` and the (flat) list of available levels.
 *
 * Mirrors `findModelConfigOption`: it keys off `id` (not `category`) since
 * the effort option is identified by `id: 'effortLevel'`. Grouped options
 * are not emitted by KAS today and yield an empty list. Returns undefined
 * when no effortLevel entry is present (e.g. the active model declares no
 * thought-level schema) so callers can clear cached state.
 */
function findEffortConfigOption(
  configOptions: unknown
): { currentValue?: string; options: EffortOption[] } | undefined {
  if (!Array.isArray(configOptions)) return undefined;
  for (const opt of configOptions as Array<Record<string, unknown>>) {
    if (opt.id !== 'effortLevel' || opt.type !== 'select') continue;
    const raw = Array.isArray(opt.options) ? opt.options : [];
    const options = raw
      .filter((o: any): o is Record<string, unknown> => {
        return (
          typeof o === 'object' &&
          o !== null &&
          typeof (o as any).value === 'string' &&
          typeof (o as any).name === 'string'
        );
      })
      .map((o: any) => ({ value: o.value as string, name: o.name as string }));
    return {
      currentValue:
        typeof opt.currentValue === 'string' ? opt.currentValue : undefined,
      options,
    };
  }
  return undefined;
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

abstract class BaseAcpClient implements SessionClient {
  public sessionId?: string;
  protected agentProcess: AgentProcess;
  private updateHandlers: Set<(event: AgentStreamEvent) => void> = new Set();
  private multiSessionHandlers: Set<
    (sessionId: string, event: AgentStreamEvent) => void
  > = new Set();
  private inboxHandlers: Set<(notification: any) => void> = new Set();
  private sessionEventHandlers: Set<(event: any) => void> = new Set();
  private subagentListHandlers: Set<
    (subagents: any[], pendingStages?: any[]) => void
  > = new Set();
  protected cachedBreakdown: unknown = null;

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
  abstract setMode(modeId: string): Promise<void>;
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
  abstract sendChatSlashCommandTelemetry(
    payload: ChatSlashCommandTelemetryPayload
  ): void;

  // ── Shared methods ──

  onUpdate(handler: (event: AgentStreamEvent) => void): () => void {
    this.updateHandlers.add(handler);
    return () => this.updateHandlers.delete(handler);
  }

  onMultiSessionUpdate(
    handler: (sessionId: string, event: AgentStreamEvent) => void
  ): () => void {
    this.multiSessionHandlers.add(handler);
    return () => this.multiSessionHandlers.delete(handler);
  }

  onSubagentListUpdate(
    handler: (subagents: any[], pendingStages?: any[]) => void
  ): () => void {
    this.subagentListHandlers.add(handler);
    return () => this.subagentListHandlers.delete(handler);
  }

  onSessionEvent(handler: (event: any) => void): () => void {
    this.sessionEventHandlers.add(handler);
    return () => this.sessionEventHandlers.delete(handler);
  }

  onInboxNotification(handler: (notification: any) => void): () => void {
    this.inboxHandlers.add(handler);
    return () => this.inboxHandlers.delete(handler);
  }

  close(): void {
    this.agentProcess.kill('SIGTERM');
  }

  protected broadcastStreamEvent(event: AgentStreamEvent): void {
    this.updateHandlers.forEach((handler) => handler(event));
  }

  protected broadcastMultiSession(
    sessionId: string,
    event: AgentStreamEvent
  ): void {
    this.multiSessionHandlers.forEach((h) => h(sessionId, event));
  }

  protected broadcastSubagentList(
    subagents: any[],
    pendingStages?: any[]
  ): void {
    this.subagentListHandlers.forEach((h) => h(subagents, pendingStages));
  }

  protected broadcastInbox(notification: any): void {
    this.inboxHandlers.forEach((h) => h(notification));
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
    [EXT_METHODS.SESSION_ACTIVITY]: (p) => this.handleSessionActivity(p),
    [EXT_METHODS.SESSION_LIST_UPDATE]: (p) => this.handleSessionListUpdate(p),
    [EXT_METHODS.INBOX_NOTIFICATION]: (p) => this.handleInboxNotification(p),
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
    });
    this.broadcastStreamEvent({ type: AgentEventType.PromptsUpdate, prompts });
    this.broadcastStreamEvent({ type: AgentEventType.SkillsUpdate, skills });
    // V2 has no steering concept; consumers keep state.steering at [].
  }

  private handleMetadataUpdate(params: Record<string, unknown>) {
    const sessionId = params.sessionId as string | undefined;
    if (sessionId && sessionId !== this.sessionId) return;
    const percent =
      (params.contextUsagePercentage as number | undefined) ?? null;
    if (percent !== null) {
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
    const effort = (params.effort as string | undefined) ?? null;
    this.broadcastStreamEvent({ type: AgentEventType.EffortUpdate, effort });
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
    this.broadcastStreamEvent({
      type: AgentEventType.AgentNotFound,
      requestedAgent: (params.requestedAgent as string) ?? '',
      fallbackAgent: (params.fallbackAgent as string) ?? '',
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

  private handleSessionActivity(params: Record<string, unknown>) {
    const sessionId = (params as any)?.sessionId as string;
    const event = (params as any)?.event as AgentStreamEvent;
    if (sessionId && event) this.broadcastMultiSession(sessionId, event);
  }

  private handleSessionListUpdate(params: Record<string, unknown>) {
    this.broadcastSubagentList((params as any)?.sessions ?? []);
  }

  private handleInboxNotification(params: Record<string, unknown>) {
    this.broadcastInbox(params);
  }

  private handleAgentSwitched(params: Record<string, unknown>) {
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
    const update = params.update as Record<string, unknown> | undefined;
    if (!update) return;
    const sessionUpdate = update.sessionUpdate;

    if (sessionUpdate === 'tool_call_chunk') {
      const chunk = update as {
        toolCallId: string;
        title: string;
        kind: string;
      };
      const sessionId = params.sessionId as string | undefined;
      const isSubagentEvent = sessionId && sessionId !== this.sessionId;
      const event: AgentStreamEvent = {
        type: AgentEventType.ToolCall,
        id: chunk.toolCallId,
        name: stripMcpTitlePrefix(chunk.title) || chunk.title,
        kind: chunk.kind,
        args: {},
        sessionId: isSubagentEvent ? sessionId : undefined,
      };
      if (isSubagentEvent) this.broadcastMultiSession(sessionId, event);
      this.broadcastStreamEvent(event);
      return;
    }

    if (sessionUpdate === 'retry_warning') {
      const warning = update as {
        attempt: number;
        maxAttempts: number;
        delaySecs: number;
        message: string;
      };
      logger.warn('Retry warning received:', warning);
      this.broadcastStreamEvent({
        type: AgentEventType.RetryWarning,
        attempt: warning.attempt,
        maxAttempts: warning.maxAttempts,
        delaySecs: warning.delaySecs,
        message: warning.message,
      });
      return;
    }

    // Steering events — both engines emit the same payload shape, but KAS
    // uses `AgentExecutionXxx` discriminators where Rust uses `steering_xxx`.
    // Normalize both to the same TUI event. Schema references:
    //   Rust:  crates/chat-cli-v2/src/agent/acp/extensions.rs::ExtSessionUpdate
    //   KAS:   packages/@kiro/agent/src/acp/session-updates.ts
    // Keep in sync with the kas-acp-client test fixture
    // (packages/tui/src/__tests__/kas-acp-client.test.ts).
    if (
      sessionUpdate === 'steering_queued' ||
      sessionUpdate === 'AgentExecutionUserMessageQueued'
    ) {
      this.broadcastStreamEvent({
        type: AgentEventType.SteeringQueued,
        message: (update as { message?: string }).message ?? '',
      });
      return;
    }

    if (
      sessionUpdate === 'steering_consumed' ||
      sessionUpdate === 'AgentExecutionSteeringInjected'
    ) {
      this.broadcastStreamEvent({
        type: AgentEventType.SteeringConsumed,
        content: (update as { content?: string }).content ?? '',
      });
      return;
    }

    if (
      sessionUpdate === 'steering_cleared' ||
      sessionUpdate === 'AgentExecutionUserMessageCleared'
    ) {
      this.broadcastStreamEvent({ type: AgentEventType.SteeringCleared });
      return;
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
    update: AcpSessionUpdate
  ): AgentStreamEvent | null {
    switch (update.sessionUpdate) {
      case 'user_message_chunk':
        return update.content.type === 'text'
          ? {
              type: AgentEventType.UserMessage,
              id: crypto.randomUUID(),
              content: { type: ContentType.Text, text: update.content.text },
            }
          : null;

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
          return {
            type: AgentEventType.Thought,
            id: crypto.randomUUID(),
            content: { type: ContentType.Text, text: update.content.text },
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
        const kiroMeta = extractKiroMetaFromUpdate(update);
        return {
          type: AgentEventType.ToolCall,
          id: update.toolCallId,
          name: kiroMeta?.pipeline
            ? 'orchestrate_subagent'
            : stripMcpTitlePrefix(update.title) || 'unknown',
          kind: update.kind ?? undefined,
          args: (update.rawInput as Record<string, unknown>) ?? {},
          toolContent: toolContent.length > 0 ? toolContent : undefined,
          locations: locations.length > 0 ? locations : undefined,
          ...(kiroMeta && { meta: { kiro: kiroMeta } }),
        };
      }

      case 'tool_call_update': {
        const kiroMetaUpdate = extractKiroMetaFromUpdate(update);
        if (update.status === ToolCallStatus.Completed) {
          const diffContent = (update.content ?? [])
            .filter((c) => c.type === 'diff')
            .map((c) => ({
              type: 'diff' as const,
              path: c.path,
              newText: c.newText ?? '',
              oldText: c.oldText ?? undefined,
            }));
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
          if (update.rawInput !== undefined) {
            this.broadcastStreamEvent({
              type: AgentEventType.ToolCall,
              id: update.toolCallId,
              name: stripMcpTitlePrefix(update.title ?? undefined) || 'unknown',
              kind: update.kind ?? undefined,
              args: (update.rawInput as Record<string, unknown>) ?? {},
            });
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
        this.broadcastStreamEvent({
          type: AgentEventType.PromptsUpdate,
          prompts,
        });
        this.broadcastStreamEvent({
          type: AgentEventType.SkillsUpdate,
          skills,
        });
        this.broadcastStreamEvent({
          type: AgentEventType.SteeringUpdate,
          steering,
        });
        return {
          type: AgentEventType.CommandsUpdate,
          commands: otherCommands.map((cmd: any) => ({
            name: cmd.name,
            description: cmd.description,
            meta: cmd._meta,
          })),
        };
      }

      // KAS-specific update types.  `current_mode_update` and
      // `config_option_update` are intercepted in KasAcpClient.wireSessionListeners
      // (they update local caches before this switch runs).  The rest are
      // not yet mapped to TUI events.
      // (`agent_thought_chunk` is handled above — see ThinkingDisplay pipeline.)
      case 'session_info_update': {
        const meta = extractKasSessionInfoMeta(update);
        if (meta?.kind === 'turn_completion') {
          const completion = normalizeKasTurnCompletion(meta);
          if (!completion) {
            // Nothing to show. Returning null keeps the chip cleared.
            return null;
          }
          if (completion.contextUsagePercentage != null) {
            this.broadcastStreamEvent({
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
          return {
            type: AgentEventType.CompactionStatus,
            status: 'completed' as const,
            summary:
              meta.conversationSummary ??
              meta.summarization?.summary?.conversationSummary,
          };
        }
        if (meta?.kind === 'summarization_started') {
          return {
            type: AgentEventType.CompactionStatus,
            status: 'started' as const,
          };
        }
        if (meta?.kind === 'summarization_failed') {
          return {
            type: AgentEventType.CompactionStatus,
            status: 'failed' as const,
          };
        }
        if (meta?.kind === 'context_usage' || meta?.contextUsage) {
          const percent = normalizeKasContextUsagePercentage(meta);
          if (typeof percent === 'number') {
            this.broadcastStreamEvent({
              type: AgentEventType.ContextUsage,
              percent,
            });
          }
          if (meta?.breakdown) {
            this.cachedBreakdown = meta.breakdown;
          }
        }
        if (
          meta?.kind === 'user_message_id_assigned' &&
          typeof (meta as any)?.userMessageId === 'string'
        ) {
          this.broadcastStreamEvent({
            type: AgentEventType.KasMessageIdAssigned,
            kasMessageId: (meta as any).userMessageId,
          });
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
        // Steering events arrive through the `_kiro.dev/session/update`
        // extension channel (see `handleExtSessionUpdate`), not here. If
        // we reach this default branch we've received something neither
        // ACP nor KAS has taught us to render yet — log and drop.
        logger.debug(
          'Unhandled session update type:',
          (update as any).sessionUpdate
        );
        return null;
    }
  }

  // ── Shared permission handling ──

  protected handlePermissionRequest(
    params: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    return new Promise<acp.RequestPermissionResponse>((resolve) => {
      const meta = params._meta as any;
      const event: AgentStreamEvent = {
        type: AgentEventType.ApprovalRequest,
        value: {
          sessionId: (params as any).sessionId as string | undefined,
          toolCall: {
            toolCallId: params.toolCall?.toolCallId || '',
            title: params.toolCall?.title ?? undefined,
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
      this.broadcastStreamEvent(event);
    });
  }

  // ── Shared session update routing ──

  protected handleSessionUpdate(params: acp.SessionNotification): void {
    const { update } = params;
    if (!update) return;
    const notifSessionId = (params as any).sessionId as string | undefined;
    const isSubagentEvent = notifSessionId && notifSessionId !== this.sessionId;
    const event = this.convertAcpUpdateToEvent(update);
    if (!event) return;

    if (isSubagentEvent) {
      this.broadcastMultiSession(notifSessionId, event);
      const isToolEvent =
        event.type === AgentEventType.ToolCall ||
        event.type === AgentEventType.ToolCallUpdate ||
        event.type === AgentEventType.ToolCallFinished;
      if (isToolEvent) this.broadcastStreamEvent(event);
    } else {
      this.broadcastStreamEvent(event);
    }
  }
}

// ─── Rust ACP client ─────────────────────────────────────────────────

export class RustAcpClient extends BaseAcpClient implements acp.Client {
  private connection: acp.ClientSideConnection;

  constructor(agentPath: string, extraAcpArgs: string[] = []) {
    const proc = spawn(agentPath, ['acp', ...extraAcpArgs], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
    });
    super(toAgentProcess(proc));
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
      clientInfo: { name: 'kiro-tui', version: TUI_VERSION },
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
    return {
      sessionId: r.sessionId,
      currentModel: extractModel(r.models),
      currentAgent: extractCurrentAgent(r.modes),
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
    return {
      sessionId,
      currentModel: extractModel(r.models),
      currentAgent: extractCurrentAgent(r.modes),
    };
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

    await Promise.race([
      this.connection.prompt({ prompt: messages, sessionId: this.sessionId }),
      connectionClosed,
    ]);
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

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionId) return;
    await this.connection.setSessionMode({ sessionId: this.sessionId, modeId });
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

// ─── KAS ACP client ──────────────────────────────────────────────────

/** Map TUI-facing mode names to KAS wire names. */
function toKasModeId(tuiModeId: string): string {
  // The TUI surfaces the planner under the internal name `kiro_planner`; the
  // agent's read-only planner builtin mode is wire id `plan`.
  if (tuiModeId === 'kiro_planner') return 'plan';
  if (tuiModeId === 'kiro_default') return 'vibe';
  return tuiModeId;
}

/** Map KAS wire mode names back to TUI-facing names. */
function fromKasModeId(kasModeId: string): string {
  if (kasModeId === 'plan') return 'kiro_planner';
  if (kasModeId === 'vibe') return 'kiro_default';
  return kasModeId;
}

export class KasAcpClient extends BaseAcpClient {
  private kiroClient: KiroClient;
  private mcpServerCache: McpServerInfo[] = [];
  private mcpRegistryCache: McpServerInfo[] = [];
  private pendingOAuthServerNames: Set<string> = new Set();
  private chatSessionStartedSessions = new Set<string>();

  /**
   * Initial agent name (KAS "mode") to apply on the next `newSession`.
   * Sourced from the TUI's `--agent` CLI flag.  Mirrors V2's
   * `set_next_agent_name` semantics: only applied to brand-new sessions;
   * `loadSession` keeps the persisted agent.
   */
  private readonly initialAgent?: string;

  /**
   * Default model to apply on the next `newSession`, sourced from
   * `chat.defaultModel` in cli.json.  Only used when `--model` was not
   * passed on the CLI (so an explicit flag always takes precedence).
   */
  private readonly initialModel?: string;

  /**
   * Construct a KAS ACP client.
   *
   * Default (no options): spawn the KAS subprocess and wire its stdio as
   * the ACP `Stream`. This is the production path.
   *
   * With `options.stream`: skip the subprocess spawn and use the provided
   * stream (for `acp_integ_tests/`'s mock transport). The client reports
   * `kill()`/`onExit()` through a no-op null agent process internally so
   * `BaseAcpClient` has a uniform interface to work against.
   *
   * With `options.initialAgent`: apply the given agent name as the KAS
   * `mode` config option on the first `newSession`.  Takes precedence
   * over the legacy `KIRO_MODE` env var (which is the propagation
   * channel for the KAS-only `--mode=vibe|spec` Rust flag).
   *
   * `agentProcess` is intentionally not a public option - mock callers
   * never need to inject a different one, and accepting it without a
   * stream would silently ignore it.
   */
  constructor(options?: {
    stream?: Stream;
    initialAgent?: string;
    initialModel?: string;
  }) {
    if (options?.stream) {
      super(createNullAgentProcess());
      this.initialAgent = options.initialAgent;
      this.initialModel = options.initialModel;
      const finalStream = maybeWrapStreamWithRecorder(options.stream);
      this.kiroClient = new KiroClient({
        stream: finalStream,
        clientInfo: { name: 'kiro-cli', version: TUI_VERSION },
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

    const proc = spawn(
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
          KIRO_CUSTOM_USER_AGENT: `KiroCLI/${TUI_VERSION} KAS/${getKasVersion(kasServerPath)} os/${process.platform} md/appVersion-${TUI_VERSION} app/AmazonQ-For-CLI`,
        },
      }
    );
    super(toAgentProcess(proc));
    this.initialAgent = options?.initialAgent;
    this.initialModel = options?.initialModel;
    const stream = buildStdioStreams(proc);
    const finalStream = maybeWrapStreamWithRecorder(stream);
    const kasSettings = buildKasSettings();
    this.kiroClient = new KiroClient({
      stream: finalStream,
      clientInfo: { name: 'kiro-cli', version: TUI_VERSION },
      capabilities: [
        createGetAccessTokenCapability(),
        createOpenExternalUrlCapability(),
        ...createSecretStorageCapabilities(),
      ],
      clientMeta: {
        telemetryEnabled: isTelemetryEnabled(),
        ...(isTelemetryEnabled() && { telemetry: getTelemetryIdentity() }),
        knowledge: true,
        hooks: { enabled: true, v2: true },
        ...(kasSettings && { settings: kasSettings }),
      },
    });
  }

  // Pipeline support: maps toolCallId → agentSubtaskId for permission routing
  private toolCallToSubtask: Map<string, string> = new Map();

  private sessionDisposables: Array<{ dispose: () => void }> = [];

  /** Cache of session modes received on session/new and session/load.  This
   *  is the source of truth for the /agent selection menu and stays in sync
   *  via current_mode_update session notifications. */
  private modesState: CachedModesState = {
    availableModes: [],
    currentModeId: undefined,
  };

  /** Capture availableModes / currentModeId from a session/new or
   *  session/load response.  Responses lacking a `modes` field leave the
   *  cache untouched so we don't accidentally blank out a known-good
   *  snapshot. */
  private captureModes(
    response: { modes?: CachedModesState | null | undefined } | undefined
  ): void {
    const modes = response?.modes;
    if (!modes) return;
    this.modesState = {
      availableModes: (modes.availableModes ?? [])
        .map((m) => {
          const id = fromKasModeId(m.id);
          return {
            ...m,
            id,
            name: id === 'kiro_default' ? 'Kiro' : m.name,
          };
        })
        // Drop denylisted bundled agents (e.g. the bundled semantic
        // reviewer) so they never appear in the /agent menu or any derived
        // listing. User/workspace-defined agents are preserved — see
        // AGENT_DENYLIST and isAgentDenied for the rationale.
        // Also drop reserved builtin modes hidden from the TUI (quick-spec).
        .filter((m) => !isAgentDenied(m) && !HIDDEN_BUILTIN_MODES.has(m.id)),
      currentModeId: modes.currentModeId
        ? fromKasModeId(modes.currentModeId)
        : modes.currentModeId,
    };
  }

  /**
   * Cached model options from the most recent session/new, session/load,
   * or session/set_config_option response. Populated from the `model`
   * category entry in the ACP Session Config Options list.
   *
   * Used by:
   *   - getCommandOptions('/model') — powers the selection menu
   *   - executeCommand('model') — validates the switch + resolves display name
   *
   * Empty when the KAS agent has no ModelConfigProvider registered.
   */
  private modelOptions: ModelOption[] = [];
  /** ID of the currently selected model, or undefined if no model config. */
  private currentModelId?: string;
  /**
   * Cached effort-level options from the most recent session/new,
   * session/load, or session/set_config_option response. Populated from
   * the `id: 'effortLevel'` entry in the ACP Session Config Options list.
   *
   * Used by:
   *   - getCommandOptions('/effort') — powers the selection menu
   *   - executeCommand('effort') — validates the level + resolves the label
   *
   * Empty when the active model declares no effortLevels schema.
   */
  private effortOptions: EffortOption[] = [];
  /** Currently selected effort level, or undefined when none is advertised. */
  private currentEffortLevel?: string;
  /** Cached hooks from the agent's registry, updated via _kiro/hooks/didChange. */
  private cachedHooks: HookInfo[] = [];
  /** Disposable for the hooks notification subscription. */
  private hooksNotificationDisposable: { dispose: () => void } | null = null;

  private wireSessionListeners(sessionId: string): void {
    this.sessionDisposables.forEach((d) => d.dispose());
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
        if (update.sessionUpdate === 'current_mode_update') {
          const newModeId = fromKasModeId(
            (update as { currentModeId: string }).currentModeId
          );
          const previousModeId = this.modesState.currentModeId;
          this.modesState = { ...this.modesState, currentModeId: newModeId };
          // Broadcast only on an actual change so we don't emit a
          // spurious "switched to X" welcome message when the agent
          // re-asserts its current mode at session start.
          if (newModeId && newModeId !== previousModeId) {
            const mode = this.modesState.availableModes.find(
              (m) => m.id === newModeId
            );
            const welcomeMessage = mode?._meta?.welcomeMessage as
              | string
              | undefined;
            this.broadcastStreamEvent({
              type: AgentEventType.AgentSwitched,
              agentName: newModeId,
              previousAgentName: previousModeId,
              welcomeMessage,
            });
          }
        }
        // Intercept config_option_update (KAS-specific) to keep the
        // local model cache fresh without a round-trip. The agent may
        // push these notifications when it autonomously changes a
        // config option (e.g. fallback to a different model after
        // rate limits), resolves the model list late (e.g. once auth
        // completes after launch), or mirrors a client-initiated change.
        //
        // Model propagation to the app store uses a dedicated, model-only
        // `ModelUpdate` event (broadcast below) rather than the
        // `AgentSwitched` channel — so updating the model chip never
        // clobbers `currentAgent`. User-initiated `/model` switches also
        // propagate synchronously via the effect handler `updateModel`;
        // the extra ModelUpdate here is an idempotent no-op for those.
        if (
          (update as { sessionUpdate?: string }).sessionUpdate ===
          'config_option_update'
        ) {
          const configOptions = (update as { configOptions?: unknown })
            .configOptions;
          this.refreshModelCache(configOptions);
          // Keep the /effort menu cache fresh too: an autonomous model
          // switch can change (or remove) the advertised effort levels, so
          // the next time the user opens /effort it reflects the new model.
          this.refreshEffortCache(configOptions);
          // Effort, unlike model, propagates to the app store directly from
          // here. KAS exposes `effortLevel` as a session config option, and
          // the only path back to the UI for autonomous changes (e.g. the
          // user switches model from a high-effort model to a low-effort one
          // via the /model command) is through this notification. The store
          // setter is idempotent — re-broadcasting an unchanged value is a
          // harmless no-op.
          this.broadcastEffortFromConfigOptions(configOptions);
          // Propagate the current model so the model chip self-heals when KAS
          // resolves the model list late (e.g. after auth completes post-
          // launch) or changes it autonomously. We use a dedicated
          // ModelUpdate event (model-only) rather than AgentSwitched so this
          // never clobbers currentAgent. extractModelFromConfigOptions
          // returns undefined when no model category/currentValue is present,
          // in which case we leave the chip unchanged.
          const model = extractModelFromConfigOptions(configOptions);
          if (model) {
            this.broadcastStreamEvent({
              type: AgentEventType.ModelUpdate,
              model,
            });
          }
        }
        this.forwardKasTurnCompletionTelemetry(sessionId, update);
        const event = this.convertAcpUpdateToEvent(update);
        if (!event) return;

        // Intercept pipeline metadata → emit subagent list update
        const meta = extractKiroMetaFromEvent(event);
        if (meta?.pipeline) {
          this.handlePipelineStateUpdate(meta.pipeline);
        }

        // Intercept per-stage events → route to multi-session handlers
        if (meta?.agentSubtaskId) {
          const subtaskId = meta.agentSubtaskId;
          if (event.type === AgentEventType.ToolCall) {
            event.sessionId = subtaskId;
            this.toolCallToSubtask.set(event.id, subtaskId);
          }
          this.broadcastMultiSession(subtaskId, event);
          // Any event tagged with agentSubtaskId belongs to a sub-agent
          // (per-stage content, tool calls, or the invoke_sub_agent
          // wrapper). It must not also reach the main conversation,
          // otherwise sub-agent narration leaks into the main view as
          // duplicates of what shows in SUBAGENT OUTPUT.
          return;
        }

        this.broadcastStreamEvent(event);
      }),
      this.kiroClient.onPermissionRequest(sessionId, async (request) => {
        return this.handleKasPermissionRequest(request);
      }),
      // Mid-turn steering events arrive as `_kiro/steering/session_update` ext
      // notifications rather than standard `session/update` because the ACP
      // SDK validates `session/update` params against a fixed schema that
      // rejects vendor-specific discriminators. Delegate to the same
      // handler used by the Rust engine.
      this.kiroClient.onExtNotification(
        '_kiro/steering/session_update',
        (params) => this.handleExtSessionUpdate(params)
      ),
    ];
  }

  /** Override to filter out commands that match cached modes (agents).
   *  Typed agent/mode/custom-agent entries are already dropped upstream in
   *  the partition switch (see convertAcpUpdateToEvent in the base class).
   *  This is the fallback for the *untyped* case: KAS may send a switchable
   *  agent in available_commands_update without a recognized _meta.kiro.type,
   *  so we cross-reference the modes cache by name to catch it. (Untyped
   *  custom-agent subagents can't be caught here — they're not modes — so
   *  the upstream type-based filter is the source of truth for those.)
   *  The cache holds TUI-translated ids (e.g. `kiro_default`), but KAS
   *  emits commands using the wire ids (e.g. `vibe`), so the filter set
   *  has to include both. */
  protected override convertAcpUpdateToEvent(
    update: AcpSessionUpdate
  ): AgentStreamEvent | null {
    const event = super.convertAcpUpdateToEvent(update);
    if (
      event?.type === AgentEventType.CommandsUpdate &&
      this.modesState.availableModes.length > 0
    ) {
      const modeIds = new Set<string>();
      for (const m of this.modesState.availableModes) {
        modeIds.add(m.id);
        modeIds.add(toKasModeId(m.id));
      }
      event.commands = event.commands.filter((cmd) => !modeIds.has(cmd.name));
    }
    return event;
  }

  private handlePipelineStateUpdate(pipeline: {
    groupId: string;
    stages: Array<{
      name: string;
      role: string;
      status: string;
      dependsOn: string[];
      agentSubtaskId: string | null;
    }>;
  }): void {
    const statusMap: Record<string, { type: string }> = {
      running: { type: 'working' },
      completed: { type: 'terminated' },
      failed: { type: 'terminated' },
    };

    const subagents = pipeline.stages
      .filter((s) => s.agentSubtaskId != null)
      .map((s) => ({
        sessionId: s.agentSubtaskId!,
        sessionName: s.name,
        agentName: s.role,
        status: statusMap[s.status] || { type: 'idle' },
        group: pipeline.groupId,
        role: s.role,
        dependsOn: s.dependsOn,
      }));

    const pendingStages = pipeline.stages
      .filter((s) => s.status === 'pending')
      .map((s) => ({
        name: s.name,
        role: s.role,
        agentName: s.role,
        group: pipeline.groupId,
        dependsOn: s.dependsOn,
      }));

    this.broadcastSubagentList(subagents, pendingStages);
  }

  private handleKasPermissionRequest(
    request: any
  ): Promise<acp.RequestPermissionResponse> {
    // KAS sends toolCallId at top level; normalize to ACP format and enrich with stage correlation
    const toolCallId = request.toolCallId || request.toolCall?.toolCallId || '';
    const subtaskId = this.toolCallToSubtask.get(toolCallId);
    const enriched = {
      ...request,
      toolCall: request.toolCall || { toolCallId },
      ...(subtaskId && { sessionId: subtaskId }),
    };
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
    await this.kiroClient.initialize();

    // Subscribe to hooks registry changes. The agent pushes this
    // notification whenever hooks are loaded, reloaded, or the file
    // watcher detects a change. We cache the list and broadcast a
    // HooksUpdate event so the TUI can refresh the panel if open.
    this.hooksNotificationDisposable = this.kiroClient.onExtNotification(
      '_kiro/hooks/didChange',
      (params: Record<string, unknown>) => {
        const rawHooks = Array.isArray(params.hooks) ? params.hooks : [];
        this.cachedHooks = this.projectHooks(rawHooks);
        this.broadcastStreamEvent({
          type: AgentEventType.HooksUpdate,
          hooks: this.cachedHooks,
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
        this.handleAgentNotFound(params);
        // KAS doesn't send current_mode_update after fallback, so update the cached mode here
        const fallback = params.fallbackAgent as string | undefined;
        if (fallback) {
          const previousModeId = this.modesState.currentModeId;
          const newModeId = fromKasModeId(fallback);
          this.modesState = {
            ...this.modesState,
            currentModeId: newModeId,
          };
          // Also broadcast AgentSwitched so the store's currentAgent updates
          // and the prompt bar shows the correct fallback agent name.
          if (newModeId !== previousModeId) {
            this.broadcastStreamEvent({
              type: AgentEventType.AgentSwitched,
              agentName: newModeId,
              previousAgentName: previousModeId,
            });
          }
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

    const commands = KAS_COMMANDS.map((cmd) => ({
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

  override close(): void {
    this.hooksNotificationDisposable?.dispose();
    this.hooksNotificationDisposable = null;
    this.sessionDisposables.forEach((d) => d.dispose());
    this.sessionDisposables = [];
    super.close();
  }

  async newSession(): Promise<SessionResult> {
    const r = await this.kiroClient.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });
    const sid = r.sessionId;
    this.sessionId = sid;
    logger.debug('KAS session created', { sessionId: sid });

    // Snapshot modes before any further async work so /agent has data even
    // if setSessionConfigOption below fails.
    this.captureModes(r as { modes?: CachedModesState | null | undefined });

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

    // Initial agent (KAS "mode") resolution.  CLI `--agent` flag takes
    // precedence over the legacy `KIRO_MODE` env var so explicit user
    // input always wins; the env var remains a propagation channel for
    // the KAS-only `--mode=vibe|spec` Rust flag.
    const initialMode = this.initialAgent ?? process.env.KIRO_MODE;
    if (initialMode) {
      try {
        const modeResp = await this.kiroClient.setSessionConfigOption({
          sessionId: sid,
          configId: 'mode',
          value: toKasModeId(initialMode),
        });
        // Read the actual mode from the response (may differ if KAS fell back)
        this.refreshModeFromConfigOptions(
          (modeResp as { configOptions?: unknown }).configOptions,
          initialMode
        );
      } catch (e) {
        logger.debug('Failed to set mode:', e);
      }
    }

    if (this.initialModel) {
      try {
        const modelResp = await this.kiroClient.setSessionConfigOption({
          sessionId: sid,
          configId: 'model',
          value: this.initialModel,
        });
        this.refreshModelCache(
          (modelResp as { configOptions?: unknown }).configOptions
        );
        this.refreshEffortCache(
          (modelResp as { configOptions?: unknown }).configOptions
        );
        this.broadcastEffortFromConfigOptions(
          (modelResp as { configOptions?: unknown }).configOptions
        );
      } catch (e) {
        logger.debug('Failed to set default model:', e);
      }
    }

    if (!this.initialModel) {
      this.refreshModelCache((r as { configOptions?: unknown }).configOptions);
      this.refreshEffortCache((r as { configOptions?: unknown }).configOptions);
      this.broadcastEffortFromConfigOptions(
        (r as { configOptions?: unknown }).configOptions
      );
    }

    const currentModelEntry = this.currentModelId
      ? this.modelOptions.find((m) => m.value === this.currentModelId)
      : undefined;
    // A selected-but-unavailable model (e.g. a `chat.defaultModel` this user
    // can't access) shows no chip — matching V2's `extractModel` — rather than
    // a misleading "Auto" fallback. The `r` fallback only applies when no
    // model was selected at all.
    const currentModel = currentModelEntry
      ? { id: currentModelEntry.value, name: currentModelEntry.name }
      : this.currentModelId
        ? undefined
        : (extractModelFromConfigOptions(
            (r as { configOptions?: unknown }).configOptions
          ) ?? extractModel(r.models));
    if (!this.currentModelId && currentModel) {
      this.currentModelId = currentModel.id;
    }

    return {
      sessionId: sid,
      currentModel,
      // TODO: Remove cast once @kiro/client adds `modes` to NewSessionResponse
      currentAgent: extractCurrentAgent(this.modesState),
    };
  }

  async loadSession(sessionId: string): Promise<SessionResult> {
    const previousSessionId = this.sessionId;
    this.sessionId = sessionId;

    // Register BEFORE loadSession to capture history replay events
    this.wireSessionListeners(sessionId);

    const r = await this.kiroClient
      .loadSession({ sessionId, cwd: process.cwd(), mcpServers: [] })
      .catch((err) => {
        this.sessionId = previousSessionId;
        throw err;
      });
    logger.debug(
      '[acp-client] KAS loadSession completed for session:',
      sessionId
    );

    this.captureModes(r as { modes?: CachedModesState | null | undefined });
    const configModel = extractModelFromConfigOptions(
      (r as { configOptions?: unknown }).configOptions
    );
    const legacyModel = extractModel(r.models);
    this.refreshModelCache((r as { configOptions?: unknown }).configOptions);
    if (!configModel && legacyModel) {
      this.currentModelId = legacyModel.id;
    }
    this.refreshEffortCache((r as { configOptions?: unknown }).configOptions);
    this.broadcastEffortFromConfigOptions(
      (r as { configOptions?: unknown }).configOptions
    );

    return {
      sessionId,
      currentModel: configModel ?? legacyModel,
      // TODO: Remove cast once @kiro/client adds `modes` to LoadSessionResponse
      currentAgent: extractCurrentAgent(
        (r as { modes?: Parameters<typeof extractCurrentAgent>[0] }).modes
      ),
    };
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
      case 'agent': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const parsed = parseAgentSubcommand(args);
        switch (parsed.kind) {
          case 'list':
            return this.executeAgentList();
          case 'swap':
            if (!parsed.name) {
              return { success: false, message: 'Usage: /agent swap <name>' };
            }
            return this.executeAgentSwap(parsed.name);
          case 'create':
            // TODO: route through a KAS extension method so config writes
            // stay the source of truth on the agent side.  Until then,
            // surface a clear not-yet-implemented error.
            return {
              success: false,
              message: '/agent create is not yet implemented in KAS mode',
            };
          case 'edit':
            // TODO: route through a KAS extension method (see create).
            return {
              success: false,
              message: '/agent edit is not yet implemented in KAS mode',
            };
          default: {
            // Exhaustiveness check — also satisfies eslint no-fallthrough.
            const _exhaustive: never = parsed;
            throw new Error(
              `Unhandled /agent subcommand: ${JSON.stringify(_exhaustive)}`
            );
          }
        }
      }
      case 'model': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const modelId = args?.value ?? '';
        if (!modelId) {
          // Bare `/model` invocation. The dispatcher normally fetches
          // options via getCommandOptions for selection-type commands,
          // so this branch fires only when the user typed `/model`
          // directly as a command with no arg (unlikely) — surface a
          // helpful hint rather than a generic failure.
          return {
            success: false,
            message:
              this.modelOptions.length === 0
                ? 'No models available'
                : 'Usage: /model <model-id>',
          };
        }
        if (modelId === 'set-current-as-default') {
          if (!this.currentModelId) {
            return { success: false, message: 'No model is currently active' };
          }
          await updateCliSetting('chat.defaultModel', this.currentModelId);
          const name =
            this.modelOptions.find((m) => m.value === this.currentModelId)
              ?.name ?? this.currentModelId;
          return {
            success: true,
            message: `Saved ${name} as default model`,
          };
        }
        return this.executeModelSwap(modelId);
      }
      case 'effort': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const level = args?.value ?? '';
        if (!level) {
          // Bare `/effort` reaches here only when getCommandOptions
          // returned no options (model has no effortLevels schema) and the
          // dispatcher fell through to execute. Surface the V2-style
          // descriptive guidance rather than a generic failure.
          return {
            success: false,
            message:
              this.effortOptions.length === 0
                ? 'Effort is not available on the current model. Select a model that supports effort levels.'
                : 'Usage: /effort <level>',
          };
        }
        return this.executeEffortChange(level);
      }
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
        this.broadcastStreamEvent({
          type: AgentEventType.CompactionStatus,
          status: 'started',
        });
        // KAS reports the compact outcome only as `{ success: boolean }` (see
        // the `_kiro/session/compact` covenant type) and resolves the RPC after
        // the operation finishes. It never emits a `summarization_started`, so
        // the eager 'started' above is what shows the spinner; we terminate it
        // from the RPC result, deriving purely from `success` — we never infer
        // a reason the agent didn't give us:
        //   • success → 'completed'. A real compaction also emits a
        //     `summarization_completed` session update carrying the summary;
        //     this terminator is an idempotent duplicate (no summary, so it
        //     won't double-append and processQueue() no-ops when idle).
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
              this.broadcastStreamEvent({
                type: AgentEventType.CompactionStatus,
                status: 'failed',
                error: result.message || undefined,
              });
            } else {
              this.broadcastStreamEvent({
                type: AgentEventType.CompactionStatus,
                status: 'completed',
              });
            }
          })
          // Defensive: callExtMethod swallows errors today, but guard against a
          // future change so a rejected promise can never strand the spinner.
          .catch((e) => {
            this.broadcastStreamEvent({
              type: AgentEventType.CompactionStatus,
              status: 'failed',
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
      case 'mcp': {
        const args = (command as Record<string, unknown>).args as
          | Record<string, string>
          | undefined;
        const value = args?.value?.trim() ?? '';

        if (value === 'list') {
          return {
            success: true,
            message: `${this.mcpServerCache.length} configured, ${this.mcpRegistryCache.length} registry servers`,
            data: {
              servers: this.mcpServerCache,
              registryServers: this.mcpRegistryCache,
              mode: 'list',
            },
          };
        }

        return {
          success: true,
          message: `${this.mcpServerCache.length} configured server${this.mcpServerCache.length === 1 ? '' : 's'}`,
          data: { servers: this.mcpServerCache },
        };
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

  /** /agent (no args) — fallback reached only when getCommandOptions returns
   *  no options.  We derive from cached ACP modes (see getCommandOptions for
   *  the primary code path). */
  private async executeAgentList(): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    const modes = this.modesState.availableModes;
    const current = this.modesState.currentModeId ?? '';
    const agents = modes.map((m) => ({
      name: m.id,
      description: m.description ?? '',
    }));
    return {
      success: true,
      message: `${agents.length} agents available`,
      data: { agents, current },
    };
  }

  /** /agent swap — uses session/setMode (standard ACP) since KAS doesn't have _kiro/agent/swap */
  private async executeAgentSwap(agentName: string): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'mode',
        value: toKasModeId(agentName),
      });
      return {
        success: true,
        message: `Switched to ${agentName}`,
        data: { agent: { name: agentName } },
      };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Failed to switch agent',
      };
    }
  }

  /**
   * /model switch — uses the ACP-standard `session/set_config_option`
   * with `configId: 'model'`. KAS returns the full configOptions state
   * in the response, which we use to refresh the local model cache and
   * resolve the new model's display name.
   *
   * Session state (history, tools, MCP servers, agent profile) is
   * preserved across model switches — KAS simply updates the
   * `modelId` on its in-memory session and picks it up on the next
   * prompt turn.
   */
  private async executeModelSwap(modelId: string): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    try {
      const response = await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'model',
        value: modelId,
      });
      const configOptions = (response as { configOptions?: unknown })
        .configOptions;
      this.refreshModelCache(configOptions);
      this.refreshEffortCache(configOptions);
      this.broadcastEffortFromConfigOptions(configOptions);
      const model = extractModelFromConfigOptions(configOptions);
      // Validate the switch landed on the requested id. If KAS rejected
      // the value but still returned a configOptions state, surface a
      // clear error rather than silently reporting success.
      if (!model || model.id !== modelId) {
        return {
          success: false,
          message: `Model '${modelId}' not available`,
        };
      }
      return {
        success: true,
        message: `Switched to ${model.name}`,
        data: { model: { id: model.id, name: model.name } },
      };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Failed to switch model',
      };
    }
  }

  /**
   * /effort — set the reasoning effort level via the ACP-standard
   * `session/set_config_option` with `configId: 'effortLevel'`. Mirrors
   * `executeModelSwap`: KAS returns the full configOptions state in the
   * response, which we use to refresh the local cache and validate the
   * write landed.
   *
   * The success message is locked to `"Effort set to {Level}"` (display-
   * cased via `formatEffort`, e.g. `xhigh → xHigh`). Unlike V2's
   * `/effort`, we deliberately omit the `" (saved for {model})"` suffix:
   * that suffix exists in V2 only because V2 persists a per-model default
   * to the client-side `ChatModelDefaults` setting. KAS persists
   * `effortLevel` to its own session metadata server-side and this feature
   * does no client-side persistence, so there is nothing "saved" from the
   * TUI's perspective.
   */
  private async executeEffortChange(level: string): Promise<CommandResult> {
    if (!this.sessionId)
      return { success: false, message: 'No active session' };
    if (this.effortOptions.length === 0) {
      return {
        success: false,
        message:
          'Effort is not available on the current model. Select a model that supports effort levels.',
      };
    }
    try {
      const response = await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'effortLevel',
        value: level,
      });
      const configOptions = (response as { configOptions?: unknown })
        .configOptions;
      this.refreshEffortCache(configOptions);
      // Note: unlike the autonomous `config_option_update` path, we do NOT
      // call `broadcastEffortFromConfigOptions` here. Propagation of a
      // user-initiated `/effort` to the store happens through the
      // `updateEffort` effect handler (reads `data.effort` from this
      // result) — matching how `/model` propagates via `updateModel`. A
      // broadcast here would double-fire `setCurrentEffort` for the same
      // action; keeping a single path makes the data flow unambiguous.
      // Validate the write landed on the requested level. KAS silently
      // ignores invalid values, leaving currentValue unchanged — surface a
      // clear error rather than reporting a false success.
      if (this.currentEffortLevel !== level) {
        return {
          success: false,
          message: `Effort '${level}' not available`,
        };
      }
      return {
        success: true,
        message: `Effort set to ${formatEffort(level)}`,
        data: { effort: level },
      };
    } catch (e) {
      return {
        success: false,
        message: e instanceof Error ? e.message : 'Failed to set effort',
      };
    }
  }

  /**
   * /hooks — lists configured hooks from the agent's registry.
   *
   * Uses the cached hook list (populated by _kiro/hooks/didChange
   * notifications) when available. Otherwise calls the agent's
   * _kiro/hooks/list extension method.
   */
  private async executeHooks(): Promise<CommandResult> {
    // Use cached hooks if we have them (populated by didChange notification)
    if (this.cachedHooks.length > 0) {
      const message = `${this.cachedHooks.length} hook${this.cachedHooks.length === 1 ? '' : 's'} configured`;
      return {
        success: true,
        message,
        data: { hooks: this.cachedHooks, message },
      };
    }

    // Fetch from agent
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
        this.cachedHooks = hooks;
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

  /** Project raw hook data from the agent into the HookInfo shape. */
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
  ): HookInfo[] {
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
  private formatHooksResult(hooks: HookInfo[]): CommandResult {
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
    return { entries: data?.entries ?? [], message: data?.message };
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

  /**
   * Latest context-usage breakdown pushed via session_info_update, or
   * null if none has arrived yet. The /context handler uses this to
   * open the panel from cache without a round-trip.
   */
  getCachedContextBreakdown(): ContextBreakdownData | null {
    return (this.cachedBreakdown as ContextBreakdownData | null) ?? null;
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
   * Refresh the cached model options + current model id from a KAS
   * configOptions array (returned by session/new, session/load, and
   * session/set_config_option).
   *
   * If the array contains no `category: 'model'` entry (e.g. KAS has
   * no ModelConfigProvider registered), the cache is cleared so that
   * `/model` surfaces "No options available" rather than stale data.
   */
  private refreshModelCache(configOptions: unknown): void {
    const modelOpt = findModelConfigOption(configOptions);
    if (!modelOpt) {
      // Visibility into the "why is /model empty?" case — logs the
      // ids/categories present so developers can see that KAS returned
      // e.g. only [mode, autopilot, contentCollection] with no model
      // entry (typical for a standalone KAS without a
      // ModelConfigProvider registered by the host IDE).
      const present = Array.isArray(configOptions)
        ? (configOptions as Array<Record<string, unknown>>).map((o) => ({
            id: o.id,
            category: o.category,
          }))
        : configOptions;
      logger.debug(
        '[kas] refreshModelCache: no `category: "model"` entry. configOptions:',
        present
      );
      this.modelOptions = [];
      this.currentModelId = undefined;
      return;
    }
    logger.debug(
      '[kas] refreshModelCache: cached',
      modelOpt.options.length,
      'models, current:',
      modelOpt.currentValue
    );
    this.modelOptions = modelOpt.options;
    this.currentModelId = modelOpt.currentValue;
  }

  /**
   * Refresh the cached effort-level options + current level from a KAS
   * configOptions array (returned by session/new, session/load,
   * session/set_config_option, or pushed via `config_option_update`).
   *
   * Mirrors `refreshModelCache`. When the array contains no
   * `id: 'effortLevel'` entry (e.g. the active model declares no
   * thought-level schema), the cache is cleared so `/effort` surfaces a
   * descriptive "not available" error rather than stale levels.
   */
  private refreshEffortCache(configOptions: unknown): void {
    const effortOpt = findEffortConfigOption(configOptions);
    if (!effortOpt) {
      this.effortOptions = [];
      this.currentEffortLevel = undefined;
      return;
    }
    this.effortOptions = effortOpt.options;
    this.currentEffortLevel = effortOpt.currentValue;
  }

  /** Update cached currentModeId from a setSessionConfigOption response.
   *  Falls back to `requestedMode` if the response doesn't contain mode info. */
  private refreshModeFromConfigOptions(
    configOptions: unknown,
    requestedMode: string
  ): void {
    if (!Array.isArray(configOptions)) {
      this.modesState = { ...this.modesState, currentModeId: requestedMode };
      return;
    }
    const modeOpt = (configOptions as Array<Record<string, unknown>>).find(
      (o) => o.id === 'mode'
    );
    const actual = (modeOpt as { currentValue?: string } | undefined)
      ?.currentValue;
    this.modesState = {
      ...this.modesState,
      currentModeId: fromKasModeId(actual ?? requestedMode),
    };
  }

  /**
   * Broadcast the current effort level extracted from a KAS configOptions
   * array (returned by session/new, session/load, set_config_option, or
   * pushed via a `config_option_update` session notification) as an
   * `EffortUpdate` stream event.
   *
   * The KAS path's effort signal is per-session config, not per-turn
   * metadata as on V2 — but the TUI's `currentEffort` store slot is the
   * same in both cases, so we funnel into the same event channel.
   * Broadcasting `null` when no effortLevel option is present clears
   * the chip cleanly when the active model has no thought-level schema.
   */
  private broadcastEffortFromConfigOptions(configOptions: unknown): void {
    const effort = extractEffortFromConfigOptions(configOptions);
    this.broadcastStreamEvent({ type: AgentEventType.EffortUpdate, effort });
  }

  /**
   * Handle `_kiro/mcp/status` notification from KAS.
   * Transforms the notification data into McpServerInfo[] and caches it.
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

    if (!servers) {
      this.mcpServerCache = [];
    } else {
      this.mcpServerCache = servers.map((server) => {
        let status: McpServerInfo['status'];
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

    // Cache registry servers separately
    const registryServers =
      (params.registryServers as Array<{
        name: string;
        version?: string;
        description?: string;
        enabled?: boolean;
      }>) ?? [];
    this.mcpRegistryCache = registryServers.map((s) => ({
      name: s.name,
      status: 'disabled' as const,
      toolCount: 0,
      version: s.version,
      description: s.description,
      enabled: s.enabled,
    }));

    logger.debug(
      '[kas] handleMcpStatusNotification: cached',
      this.mcpServerCache.length,
      'configured,',
      this.mcpRegistryCache.length,
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
    const result = await this.executeAgentSwap('kiro_planner');
    if (!result.success) return result;
    return {
      success: true,
      message: result.message,
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
      case 'agent': {
        // Derive options from cached session modes (ACP primitive) rather
        // than a custom ext method.  Modes are grouped by `_meta.kiro.source`
        // (e.g. "bundled", "user", "workspace") so the menu reflects where
        // each agent came from.  See the review discussion at
        // https://github.com/kiro-team/kiro-agent/pull/568#discussion_r3192594213
        const { availableModes, currentModeId } = this.modesState;
        return {
          options: availableModes.map((m) => {
            const source = getModeSource(m._meta);
            const isActive = m.id === currentModeId;
            const descBase = m.description ?? '';
            return {
              value: m.id,
              label: getAgentDisplayName(m.id, m.name),
              description: isActive
                ? `[active]${descBase ? ` ${descBase}` : ''}`
                : descBase,
              ...(source ? { group: capitalize(source) } : {}),
            };
          }),
        };
      }
      case 'model': {
        // Served from the local cache populated by session/new,
        // session/load, and session/set_config_option responses.
        // KAS returns the full configOptions state on every
        // mutation, so the cache stays in sync without extra
        // round-trips.
        if (this.modelOptions.length === 0) return { options: [] };
        return {
          options: this.modelOptions.map((m) => {
            const isActive = m.value === this.currentModelId;
            const desc = m.description ?? '';
            return {
              value: m.value,
              label: m.name,
              description: isActive
                ? desc
                  ? `[active] ${desc}`
                  : '[active]'
                : desc,
            };
          }),
        };
      }
      case 'effort': {
        // Served from the local cache populated by session/new,
        // session/load, session/set_config_option, and config_option_update
        // responses — same flow as `model` above. Empty when the active
        // model declares no effortLevels schema.
        if (this.effortOptions.length === 0) return { options: [] };
        return {
          options: this.effortOptions.map((o) => {
            const isActive = o.value === this.currentEffortLevel;
            return {
              value: o.value,
              label: o.name,
              description: isActive ? '[active]' : '',
            };
          }),
        };
      }
      // /prompts options are owned by the `handlePrompts` kas-handler,
      // which reads directly from the typed AppState slices (prompts /
      // skills / steering) and never round-trips through here.
      default:
        return { options: [] };
    }
  }

  async setMode(modeId: string): Promise<void> {
    if (!this.sessionId) return;
    try {
      await this.kiroClient.setSessionConfigOption({
        sessionId: this.sessionId,
        configId: 'mode',
        value: toKasModeId(modeId),
      });
    } catch (e) {
      logger.debug('Failed to set mode:', e);
    }
  }

  async listSessions(_cwd: string): Promise<ListSessionsResponse> {
    try {
      const r = await this.kiroClient.listSessions();
      logger.debug('[kas] listSessions raw:', JSON.stringify(r));
      return {
        sessions: r.sessions.map((s: any) => ({
          sessionId: s.sessionId,
          cwd: s.cwd,
          title: s.title,
          updatedAt: s.updatedAt ?? s._meta?.createdAt,
        })),
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

  sendProcessHealthMetrics(payload: ProcessHealthSnapshot): void {
    void emitKasTelemetry('kas-process-health', {
      ...payload,
      agentKind: 'kas',
    })
      .then((result) => {
        if (!result.ok) {
          logger.debug('[kas] telemetry bridge failed:', result.message);
        }
      })
      .catch((err) => logger.debug('[kas] telemetry bridge failed:', err));
  }

  sendModeChanged(payload: ModeChangedNotification): void {
    void emitKasTelemetry('kas-mode-changed', payload)
      .then((result) => {
        if (!result.ok) {
          logger.debug('[kas] telemetry bridge failed:', result.message);
        }
      })
      .catch((err) => logger.debug('[kas] telemetry bridge failed:', err));
  }

  sendChatSlashCommandTelemetry(
    payload: ChatSlashCommandTelemetryPayload
  ): void {
    void emitKasTelemetry('kas-chat-slash-command', {
      ...payload,
      sessionId: this.sessionId,
    })
      .then((result) => {
        if (!result.ok) {
          logger.debug('[kas] telemetry bridge failed:', result.message);
        }
      })
      .catch((err) => logger.debug('[kas] telemetry bridge failed:', err));
  }

  private emitChatSessionStartedOnce(sessionId: string): void {
    if (this.chatSessionStartedSessions.has(sessionId)) return;
    this.chatSessionStartedSessions.add(sessionId);
    void emitKasTelemetry('kas-chat-session-started', {
      sessionId,
      ...(this.modesState.currentModeId
        ? { mode: this.modesState.currentModeId }
        : {}),
    })
      .then((result) => {
        if (!result.ok) {
          logger.debug('[kas] telemetry bridge failed:', result.message);
        }
      })
      .catch((err) => logger.debug('[kas] telemetry bridge failed:', err));
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
      this.currentModelId
    );
    if (!payload) return;
    void emitKasTelemetry('kas-turn-completion', payload)
      .then((result) => {
        if (!result.ok) {
          logger.debug('[kas] telemetry bridge failed:', result.message);
        }
      })
      .catch((err) => logger.debug('[kas] telemetry bridge failed:', err));
  }
}

/** Extract the Kiro agent source (bundled / user / workspace) from a
 *  SessionMode's `_meta` field.  Returns undefined when the agent hasn't
 *  attached any source metadata, which is fine — those modes fall into the
 *  default (ungrouped) bucket in the /agent menu. */
function getModeSource(
  meta?: Record<string, unknown> | null
): string | undefined {
  const kiroMeta = meta?.kiro as Record<string, unknown> | undefined;
  const source = kiroMeta?.source;
  return typeof source === 'string' ? source : undefined;
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0]!.toUpperCase() + s.slice(1) : s;
}

/** Tagged union describing what `/agent …` should do.  Produced by
 *  parseAgentSubcommand and consumed by KasAcpClient.executeCommand. */
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

/** Best-effort WSL detection via the Linux kernel osrelease string. */
function detectWsl(): boolean {
  if (process.platform !== 'linux') return false;
  try {
    const { readFileSync } = require('fs');
    const release = readFileSync(
      '/proc/sys/kernel/osrelease',
      'utf8'
    ).toLowerCase();
    return release.includes('microsoft') || release.includes('wsl');
  } catch {
    return false;
  }
}

/**
 * Build the argv to open a URL in the default browser, per platform. Windows
 * uses rundll32's URL handler (not cmd `start`, which mangles `&` and treats
 * the URL as a window title); WSL uses wslview to reach the Windows browser.
 * URL stays its own argv element — no shell. Pure for testability.
 */
export function browserOpenCommand(
  platform: NodeJS.Platform,
  url: string,
  isWsl = false
): { file: string; args: string[] } {
  if (platform === 'darwin') return { file: 'open', args: [url] };
  if (platform === 'win32')
    return { file: 'rundll32', args: ['url.dll,FileProtocolHandler', url] };
  if (isWsl) return { file: 'wslview', args: [url] };
  return { file: 'xdg-open', args: [url] };
}

function kasFeedback(args?: Record<string, string>): CommandResult {
  const kind = args?.value || 'general';
  // KIRO_INTERNAL=1 = signed in via Amazon-internal SSO (set by the launcher).
  const isInternal = process.env.KIRO_INTERNAL === '1';
  const url = resolveFeedbackUrl(kind, isInternal);
  try {
    const { execFileSync } = require('child_process');
    const { file, args: openArgs } = browserOpenCommand(
      process.platform,
      url,
      detectWsl()
    );
    execFileSync(file, openArgs, { stdio: 'ignore' });
    return { success: true, message: 'Opening in browser...' };
  } catch {
    return {
      success: false,
      message: `Could not open browser. Copy the URL: ${url}`,
      data: { url },
    };
  }
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

// ─── Factory ─────────────────────────────────────────────────────────

export function createAcpClient(
  agentPath: string,
  extraAcpArgs: string[] = [],
  kasOptions?: { initialAgent?: string; initialModel?: string }
): SessionClient {
  if (resolveAgentEngine() === 'kas') {
    // Test-only: inject an in-process mock transport when the harness set
    // the socket path env var. Production boots skip this branch entirely.
    // The require() path stays in this one call site so the rest of
    // `KasAcpClient` never references test-utils.
    const mockSocketPath = process.env.KIRO_ACP_MOCK_SOCKET;
    if (mockSocketPath) {
      logger.info(
        `[acp-client] KAS mock transport (test-only), socket=${mockSocketPath}`
      );
      const {
        connectMockTransport,
      } = require('./test-utils/acp-mock/MockAcpTransport');
      const stream = connectMockTransport(mockSocketPath);
      return new KasAcpClient({ stream, ...(kasOptions ?? {}) });
    }
    return new KasAcpClient(kasOptions);
  }
  return new RustAcpClient(agentPath, extraAcpArgs);
}

/** @deprecated Use createAcpClient() instead */
export const AcpClient = RustAcpClient;
