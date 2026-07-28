import { createStore, useStore, type StoreApi } from 'zustand';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import type { Kiro } from '../kiro';
import { chalk } from '../utils/color.js';
import type { TerminalColor } from '../types/themeTypes';
import { kiroSafe } from '../theme/kiroSafe';
import { createContext, useContext } from 'react';
import { getKasCommands, type KasCommand } from '../kas-commands';
import { loadExistingSession } from '../commands/kas-handlers/chat';
import { noteCloudScrollbackRepaint } from '../commands/cloud-scrollback-reconcile';
import { emitCloudDetachNoticeOnce } from '../utils/cloud-detach-notice';
import {
  mergeRosterDelta,
  deriveActiveSessionStatus,
  type RosterEntry,
} from '../utils/session-roster';
import type { SessionRepositoryEntry } from '../utils/session-repositories';
import { features } from '../features';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import type { SessionPickerRow } from '../components/ui/SessionPickerPanel';
import {
  formatRepoChangeInstruction,
  reconcileRepoSelection,
  resolveSourceProviderConnection,
} from '../utils/repo-attach';
import { dedupeRepoResources } from '../utils/repo-multiselect';
import { recordTuiCloudRepoAttach } from '../utils/tui-telemetry-observer';
import { type AgentEngine, resolveAgentEngine } from '../agent-engine';
import type {
  AgentScope,
  MigrationWarning,
} from '../utils/agent-migration/index.js';
import { selectVisibleSlashCommands } from './visible-slash-commands';
import { normalizeAtPrompt } from '../utils/normalize-at-prompt';
import { synthesizeToolUseContent } from './tool-use-synthesis';
import {
  isHistoryOnlyAssistantMessage,
  isHistoryOnlyAssistantMessagePrefix,
} from '../utils/history-only-assistant-messages.js';
import {
  isKasShellCapability,
  KAS_WHOLE_CAPABILITY_RESOURCE,
} from '../utils/shell-trust-options.js';
import {
  AgentEventType,
  ApprovalOptionId,
  TASK_TOOL_NAMES,
  SESSION_TOOL_NAMES,
  isParentSubagentTool,
  deriveToolDiff,
  type AgentStreamEvent,
  type ApprovalRequestInfo,
  type QuestionRequestInfo,
  type ToolDiff,
  type ToolKind,
  type KasModelConfigUpdateEvent,
} from '../types/agent-events';
import {
  resolveEffortToApply,
  shouldApplyEffortDefault,
} from '../utils/kas-config-options';
import { readSavedEffortDefault } from '../utils/effort-defaults';
import type {
  InputBufferState,
  InputBufferActions,
  MoveCursorDir,
} from '../types/input-buffer';
import type {
  AvailableCommand,
  CommandOption,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from '../types/commands';
import type {
  AgentEntry,
  EffortEntry,
  ModelEntry,
} from '../utils/kas-config-options';
import type { StatusType } from '../types/componentTypes';
import type { SubagentInfo, SubagentStatus } from '../types/subagent.js';
import type { AgentSession } from '../types/multi-session.js';
import type {
  SessionActivityStatus,
  SessionsChangedNotification,
  ProvisioningFailureCode,
} from '../types/session-client';
import type { TaskItem, RawTask } from '../types/tasks';
import type { ContextBreakdownData } from '../types/context';
import type { UiMode } from '../types/ui-mode.js';
import {
  createInitialKasSubagentRoutingState,
  createKasSubagentRoutingActions,
  type KasSubagentRoutingState,
  type KasSubagentRoutingStore,
} from './kas-subagent-routing';

export type { ContextBreakdownData } from '../types/context';
export type { KasSubagentRoutingStore } from './kas-subagent-routing';

/** A selectable turn in the `/rewind` Explorer. Shape is defined by the
 *  backend `/rewind` execute handler in `CommandResult.data.turns`. */
/** The armed `/spec new` description-collection step. */
export interface PendingSpecDescription {
  featureName: string;
}

export interface RewindTurn {
  logIndex: number;
  label: string;
  group: string;
  responseSnippet: string;
}

/** One agent row in the /upgrade-agent diagnostics list + drill-in detail. */
export interface UpgradeAnalysisRow {
  name: string;
  scope: AgentScope;
  warnings: MigrationWarning[];
}

export interface UsageBreakdownItem {
  displayName: string;
  used: number;
  limit: number;
  percentage: number;
  currentOverages: number;
  overageRate: number;
  overageCharges: number;
  currency: string;
  /** Whether this dimension has a real usage limit. When false (no-limit sentinel), the
   * progress bar is hidden and only consumption is shown. */
  hasLimit: boolean;
}

export interface BonusCredit {
  name: string;
  used: number;
  total: number;
  daysUntilExpiry: number;
}

/** A purchased prepaid add-on credit pack ("Additional credits"). */
export interface AddOnCreditPack {
  used: number;
  total: number;
  /** Pre-formatted expiry date (e.g. "Jun 18, 2027"), or null if omitted. */
  expiresAt: string | null;
  /** Derived: the single pack currently being consumed (earliest-expiry with remaining, FIFO). */
  isActive: boolean;
}

export interface UsageData {
  planName: string;
  billingCycleReset: string;
  overagesEnabled: boolean;
  isEnterprise: boolean;
  usageBreakdowns: UsageBreakdownItem[];
  bonusCredits: BonusCredit[];
  /** Purchased prepaid add-on credit packs (individual prepaid-overages model). */
  addOnCredits: AddOnCreditPack[];
  /** Whether the user can use/purchase add-on credits (overage_capability == OVERAGE_CAPABLE). */
  overageCapable: boolean;
}

export interface McpServerInfo {
  name: string;
  status: 'running' | 'loading' | 'failed' | 'disabled' | 'auth-required';
  toolCount: number;
  /**
   * True while a forced (re-)authentication is in progress for this server. The
   * original server keeps running during the flow (a hidden shadow server runs
   * the OAuth handshake), so this is surfaced as an `auth-required` overlay on
   * the master row rather than listing the shadow as a separate server.
   */
  authenticating?: boolean;
  // Registry fields (present in /mcp list response)
  version?: string;
  description?: string;
  enabled?: boolean;
}

export type ToolStatus = 'allowed' | 'requires-approval' | 'denied';

export interface ToolInfo {
  name: string;
  source: string;
  description: string;
  /**
   * Permission status. Present for the Rust (V2) engine, which exposes
   * per-tool trust. Absent for KAS, whose `_kiro/tools/didChange` listing is a
   * tag-based capability view with no per-tool status — the panel hides the
   * Status column when every row omits it.
   */
  status?: ToolStatus;
}

export interface RequestStat {
  request_id: string | null;
  timestamp: string;
  duration_ms: number | null;
  ttfc_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  status_code: number | null;
  had_tool_use: boolean;
  error: string | null;
}

export interface StatsSummary {
  avg_ms: number;
  p90_ms: number;
  max_ms: number;
  errors: number;
}

export interface HookInfo {
  name?: string;
  trigger: string;
  command: string;
  matcher?: string;
}

export interface KnowledgeEntry {
  name: string;
  id: string;
  description: string;
  item_count: number;
  path: string | null;
  items_display?: string;
  indexing?: boolean;
}

export interface CodeLspInfo {
  name: string;
  languages: string[];
  status: string;
  isAvailable: boolean;
  initDurationMs: number | null;
  workspaceFolders: string[];
}

export interface CodePanelData {
  status: string;
  rootPath: string;
  detectedLanguages: string[];
  projectMarkers: string[];
  lsps: CodeLspInfo[];
  configPath: string;
  docUrl?: string;
  warning?: string;
  // logs subcommand
  entries?: Array<{ timestamp: string; level: string; message: string }>;
  level?: string;
  // message from backend
  message?: string;
}

// ── Spec artifact view ───────────────────────────────────────
//
// Generation-phase tracker state and the open artifact-view panel.
// See `utils/spec-artifact-loader.ts` and
// `utils/spec-artifact-parser/` for the underlying types.

/**
 * Generation-phase entry for the live artifact-generation card.
 *
 * The store holds at most one entry at a time (see `artifactGenerating`
 * below) — when the agent moves on to a new artifact, the prior entry
 * is replaced. The `absolutePath` field doubles as the dedup key the
 * lifecycle wiring in `index.tsx` uses to correlate idle timers and
 * `ToolCallFinished` events with the active card.
 */
export interface ArtifactGenerationEntry {
  /** Absolute path on disk. Used as the dedup key for idle timers. */
  absolutePath: string;
  featureName: string;
  artifact: ArtifactKind;
  /** Last successful summary parse, or null while we wait for the first read. */
  summary: ArtifactSummary | null;
  /** Wall-clock ms of the most recent fs_write notification. */
  lastWriteTs: number;
  /** Marked true after 2 s of idleness or when the panel is opened directly. */
  complete: boolean;
  /** Set when a parse failed mid-stream so the card can show a non-blocking indicator. */
  parseError: string | null;
}

export interface OpenArtifactView {
  featureName: string;
  artifact: ArtifactKind;
  summary: ArtifactSummary;
  mode: 'summary' | 'detail';
  /** Index into the displayed item list. */
  cursor: number;
  /** Tasks-only expansion state, keyed by item index. */
  expanded: Record<number, boolean>;
  /** Non-fatal load error to surface inline; null on success. */
  error: { message: string } | null;
  /**
   * Workflow config for this feature, loaded from
   * `.kiro/specs/<feature>/.config.kiro` when the panel opens. Drives
   * the stage-bar order in the panel header. Falls back to defaults
   * when the file is missing or malformed (see `loadSpecConfig`).
   */
  workflow: SpecConfig;
}

// ── End spec artifact view ───────────────────────────────────

import {
  executeCommand,
  executeCommandWithArg,
  isKnownSlashCommandToken,
  type CommandContext,
} from '../commands/index.js';
import {
  loadArtifactSummary,
  type ArtifactKind,
  type ArtifactSummary,
  type LoadError,
} from '../utils/spec-artifact-loader.js';
import { loadSpecConfig, type SpecConfig } from '../utils/spec-config.js';
export type {
  ArtifactKind,
  ArtifactSummary,
} from '../utils/spec-artifact-loader.js';
export type { SpecConfig } from '../utils/spec-config.js';
import { formatImageLabel } from '../utils/image-label.js';
import { spliceSteerLine, removeSteerLine } from '../utils/queue-navigation.js';
import { expandFileReferences, readFileContent } from '../utils/file-search.js';
import { collectCloudAttachments } from '../utils/cloud-attach.js';
import { logger } from '../utils/logger.js';
import {
  setTerminalProgressWarning,
  setTerminalProgressIndeterminate,
  setTerminalProgressError,
  clearTerminalProgress,
} from '../utils/terminal-capabilities.js';
import { syncCmuxStatus, type CmuxAgentStatus } from '../utils/cmux.js';
import {
  getAuthErrorGuidance,
  getSessionErrorGuidance,
  getErrorGuidance,
  simplifyErrorMessage,
  detectErrorCategory,
} from '../utils/error-guidance.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { composeSpecKickoffPrompt } from '../utils/spec-workspace.js';
import { CommandHistory } from '../utils/command-history.js';
import { Settings } from '../constants/settings.js';
import { isUserDeniedReason } from '../constants/tool-failure-reasons.js';
import {
  InterruptMode,
  DEFAULT_INTERRUPT_MODE,
  parseInterruptMode,
} from '../constants/interrupt-mode.js';
import { readBoolSetting, readStringSetting } from '../utils/cli-settings.js';
import {
  resolveNotificationMethod,
  playNotification,
} from '../utils/notification.js';
import {
  DEFAULT_TURN_THRESHOLD,
  loadSurveyState,
  resolveEligibility,
  shouldShowSurvey,
  markSurveyShown,
  markSurveyCompleted,
  markSurveyDismissed,
  type SurveyState,
} from '../utils/survey-state.js';
import { submitFormToAperture } from '../utils/survey-submit.js';
import {
  SESSION_FEEDBACK_SURVEY,
  PLAN_QUALITY_SURVEY,
  IMPLEMENT_PLAN_SURVEY,
  type SurveyDefinition,
} from '../constants/survey.js';

export enum MessageRole {
  User = 'user',
  Model = 'model',
  ToolUse = 'tool_use',
  System = 'system',
}

// Helper to generate unique message IDs
const generateMessageId = () => crypto.randomUUID();

// ── Spec artifact view helpers ─────────────────────────────
//
// Kept local because they only exist to support the artifactView
// slice and aren't part of the public store API.

/** Count items in a summary for cursor / wrap-around math. */
function countArtifactItems(summary: ArtifactSummary): number {
  switch (summary.kind) {
    case 'requirements':
      return summary.items.length;
    case 'design':
      return summary.sections.length;
    case 'tasks':
      return summary.items.length;
  }
}

/** Produce a human-readable message for a `LoadError`. */
function describeLoadError(err: LoadError): string {
  switch (err.kind) {
    case 'FeatureNotFound':
      return `No spec found at .kiro/specs/${err.featureName}/`;
    case 'ArtifactNotFound':
      return `No ${err.artifact}.md in spec "${err.featureName}".`;
    case 'TooLarge':
      return `Artifact at ${err.path} is too large (${err.sizeBytes} bytes).`;
    case 'ReadFailed':
      switch (err.category) {
        case 'NotFound':
          return `File not found: ${err.path}`;
        case 'PermissionDenied':
          return `Permission denied reading ${err.path}`;
        case 'Io':
          return `Failed to read ${err.path}: ${err.message}`;
      }
  }
}

/** Empty-summary placeholder used when opening the panel in error mode. */
function emptySummaryFor(artifact: ArtifactKind): ArtifactSummary {
  switch (artifact) {
    case 'requirements':
      return { kind: 'requirements', items: [] };
    case 'design':
      return {
        kind: 'design',
        overview: '',
        overviewTruncated: false,
        sections: [],
      };
    case 'tasks':
      return { kind: 'tasks', items: [] };
  }
}

// ── End spec artifact view helpers ─────────────────────────

/**
 * Tools that are known to be broken or unavailable in the current environment.
 * Tool calls matching these names are immediately marked as finished with an
 * error result so they don't block the flush state machine with a stuck spinner.
 */
export const NOT_READY_TOOLS: Set<string> = new Set([]);

export enum ToolUseStatus {
  Pending = 'pending',
  Approved = 'approved',
  Rejected = 'rejected',
}

export type ToolResult =
  | { status: 'success'; output: unknown }
  | { status: 'error'; error: string; output?: unknown }
  | { status: 'cancelled'; output?: unknown };

export type MessageType =
  | {
      id: string;
      role: MessageRole.User;
      content: string;
      agentName?: string;
      contextPercent?: number;
      kasMessageId?: string;
      questionToolCallId?: string;
      /**
       * True when this user bubble was injected mid-turn via steering
       * (consumed from the steer queue) rather than sent as a standalone
       * prompt. Multiple steers are concatenated into a single agent
       * continuation, so an injected bubble legitimately has no AI response
       * of its own — the shared response attaches to the final bubble in the
       * group. Renderers use this to avoid mislabeling such turns as
       * "Cancelled".
       */
      steered?: boolean;
    }
  | {
      id: string;
      role: MessageRole.Model;
      content: string;
      thinking?: string;
      /** Wall-clock duration spent reasoning, in ms (set once reasoning ends). */
      thinkingMs?: number;
      agentName?: string;
      shellOutput?: boolean;
      standalone?: boolean;
    }
  | {
      id: string;
      role: MessageRole.ToolUse;
      name: string;
      isQuestion?: boolean;
      sessionId?: string;
      pipelineGroupId?: string;
      kind?: ToolKind;
      content: string;
      /**
       * Model-supplied `__tool_use_purpose` (the per-tool "why"), captured at
       * the ACP boundary before `content` is rebuilt for edit-kind tools (the
       * rebuild would otherwise drop it). Surfaced by lite's reasoning slot.
       */
      purpose?: string;
      /**
       * Structured diff produced by an edit-kind tool, when known.
       * Carried separately from `content` so renderers don't have to
       * parse a JSON-encoded blob to find the diff text.
       */
      diff?: ToolDiff;
      isFinished?: boolean;
      status?: ToolUseStatus;
      result?: ToolResult;
      locations?: Array<{ path: string; line?: number }>;
      agentName?: string;
      liveOutput?: string[];
      /** True when this tool call originated from a subagent session (event.sessionId set). */
      isSubagentTool?: boolean;
      startTime?: number;
      finishTime?: number;
    }
  | {
      id: string;
      role: MessageRole.System;
      content: string;
      success: boolean;
      /** True for status rows emitted while a user turn is in flight. */
      turnOwned?: boolean;
    };

/**
 * A conversation "turn" groups a user message with all of the AI-side messages
 * (model responses, tool uses, system notes) that followed it before the next
 * user message. Shared between `ConversationView` and `SessionOutput`.
 */
export interface ConversationTurn {
  userMessage: MessageType;
  aiMessages: MessageType[];
  isActive: boolean;
}

function isApprovalOptionKind(optionId: string): optionId is ApprovalOptionId {
  return Object.values(ApprovalOptionId).includes(optionId as ApprovalOptionId);
}

function resolveApprovalOptionKind(
  approval: ApprovalRequestInfo,
  optionId: string
): ApprovalOptionId | undefined {
  const resolvedKind = approval.permissionOptions.find(
    (o) => o.optionId === optionId
  )?.kind;
  return (
    resolvedKind ?? (isApprovalOptionKind(optionId) ? optionId : undefined)
  );
}

function resolveApprovalOptionIdByKind(
  approval: ApprovalRequestInfo,
  kind: ApprovalOptionId
): string {
  return (
    approval.permissionOptions.find((o) => o.kind === kind)?.optionId ?? kind
  );
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0
    ? value
    : undefined;
}

function hasKasResourceTrustMeta(
  meta: Record<string, unknown> | undefined
): boolean {
  return typeof meta?.kasResource === 'string' && meta.kasResource.length > 0;
}

function hasKasToolConsentTarget(approval: ApprovalRequestInfo): boolean {
  return (
    !!approval.toolId ||
    !!approval.consentContext ||
    (approval.trustOptions?.length ?? 0) > 0
  );
}

function usesKasWholeCapabilityResource(
  resolvedKind: ApprovalOptionId | undefined,
  meta: Record<string, unknown> | undefined
): boolean {
  // Any capability, not just shell: pressing "trust whole tool" on a write (or
  // any non-shell) approval must also persist the '*' wildcard, else KAS scopes
  // the trust to the one path and re-asks every other path.
  return (
    resolvedKind === ApprovalOptionId.AllowAlways &&
    meta?.kasWholeCapability === true
  );
}

function buildKasConsentMeta(
  approval: ApprovalRequestInfo,
  resolvedKind: ApprovalOptionId | undefined,
  meta: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (meta?.trustOption) return meta;
  if (!hasKasToolConsentTarget(approval)) return meta;

  const kasScope =
    (meta?.kasScope as string) ??
    (resolvedKind === ApprovalOptionId.AllowAlways ? 'session' : 'invocation');
  const explicitKasResource =
    typeof meta?.kasResource === 'string' && meta.kasResource.length > 0
      ? meta.kasResource
      : undefined;
  const capability = nonEmptyString(approval.consentContext?.capability);
  const kasResource =
    explicitKasResource !== undefined
      ? explicitKasResource
      : usesKasWholeCapabilityResource(resolvedKind, meta)
        ? KAS_WHOLE_CAPABILITY_RESOURCE
        : undefined;

  return {
    kiro: {
      consent: {
        ...(capability ? { capability } : {}),
        scope: kasScope,
        ...(kasResource ? { resource: kasResource } : {}),
        ...(approval.consentContext?.workspaceRoot
          ? { workspaceRoot: approval.consentContext.workspaceRoot }
          : {}),
      },
    },
  };
}

function toolUseMessageName(
  messages: readonly MessageType[],
  toolCallId: string
): string | undefined {
  const toolMsg = messages.find(
    (m) => m.role === MessageRole.ToolUse && m.id === toolCallId
  );
  return toolMsg?.role === MessageRole.ToolUse ? toolMsg.name : undefined;
}

function approvalToolIdentity(
  approval: ApprovalRequestInfo,
  messages: readonly MessageType[]
): string | undefined {
  const capability = nonEmptyString(approval.consentContext?.capability);
  if (capability) return capability;
  return (
    nonEmptyString(approval.toolId) ??
    nonEmptyString(
      toolUseMessageName(messages, approval.toolCall.toolCallId)
    ) ??
    nonEmptyString(approval.toolCall.title)
  );
}

function approvalTrustIdentity(
  approval: ApprovalRequestInfo,
  messages: readonly MessageType[],
  agentEngine: AgentEngine
): string | undefined {
  const identity = approvalToolIdentity(approval, messages);
  if (!identity || agentEngine !== 'kas') return identity;

  const capability = nonEmptyString(approval.consentContext?.capability);
  if (!capability) return identity;

  return [
    isKasShellCapability(capability) ? 'shell' : capability,
    nonEmptyString(approval.consentContext?.workspaceRoot) ?? '',
    nonEmptyString(approval.originSessionId) ??
      nonEmptyString(approval.sessionId) ??
      '',
  ].join('\0');
}

export interface SlashCommand extends AvailableCommand {
  source: 'local' | 'backend';
}

export interface ActiveCommand {
  command: AvailableCommand;
  options: CommandOption[];
  /**
   * Cursor row to highlight when the menu opens. Defaults to 0.
   * Used by submenu navigation (e.g. /verbose) so that re-entering the top
   * menu via ESC lands the cursor on the row the user descended from.
   * Clamped to `0..options.length-1` by the menu component.
   */
  initialIndex?: number;
  /**
   * When set, CommandMenu renders a live preview pane below the menu.
   * The /verbosity submenus use this to show synthetic scrollback that
   * reflects the in-progress config. Values match VerbosityPreviewKey in
   * lite/render.ts plus the editor-mode keys `truncation:args:edit` and
   * `truncation:output:edit`, which swap the menu for the numeric editor.
   */
  previewKey?: string;
  /**
   * When true, selecting an option always executes it via
   * `executeCommandWithArg` — even when the options happen to mirror the
   * command's `meta.subcommands` and would otherwise take the Tab-subcommand
   * prefill path. Used by handlers whose picker rows ARE the final argument
   * (e.g. /autonomous on|off).
   */
  executeOnSelect?: boolean;
}

export interface TransientAlert {
  message: string;
  status: StatusType;
  autoHideMs?: number;
  /** Optional keyboard shortcut action shown in the alert */
  action?: { label: string; key: string; onAction: () => void };
}

/**
 * Inline retry status shown alongside the thinking spinner while the HTTP client
 * is backing off between attempts. Replaces the previous transient-alert UX so the
 * user sees the retry context right next to the "Thinking..." indicator.
 */
export interface RetryStatus {
  attempt: number;
  maxAttempts: number;
  delaySecs: number;
  /** Human-readable message from the backend (e.g. "Retrying in 5s (attempt 2/6)"). */
  message: string;
}

export type InitError =
  | { type: 'mcp_failure'; serverName: string; error: string }
  | { type: 'agent_not_found'; requestedAgent: string; fallbackAgent: string }
  | { type: 'agent_config_error'; path?: string; error: string }
  | { type: 'mcp_governance_disabled'; apiFailure: boolean }
  | { type: 'web_tools_governance_disabled'; apiFailure: boolean };

export interface LastTurnTokens {
  input: number;
  output: number;
  cached: number;
}

/** Extract just the filename from a path. */
function basename(p: string): string {
  const i = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
  return i >= 0 ? p.slice(i + 1) : p;
}

/**
 * Build a `cancelled`-status tool result that carries any partial output the
 * tool streamed before the user interrupted. Shaped as the canonical ACP
 * `{items: [{Text}]}` envelope so {@link unwrapToolOutput} in the renderer
 * surfaces it without special-casing — same code path as a normal completed
 * tool, just with the yellow `✗ cancelled` chip stamped on the header.
 *
 * `chunks` is the value of `state.liveOutputs.get(toolCallId)` — an array of
 * arrays of lines accumulated by `flushToolOutputs`. Returns a bare
 * `{ status: 'cancelled' }` when there's no buffered output so callers
 * preserve the prior behavior for tools that hadn't streamed anything.
 */
function buildCancelledResult(chunks: string[][] | undefined): {
  status: 'cancelled';
  output?: { items: Array<{ Text: string }> };
} {
  if (!chunks || chunks.length === 0) return { status: 'cancelled' };
  const lines: string[] = [];
  for (const chunk of chunks) {
    for (const line of chunk) lines.push(line);
  }
  if (lines.length === 0) return { status: 'cancelled' };
  return {
    status: 'cancelled',
    output: { items: [{ Text: lines.join('\n') }] },
  };
}

// ── Tool-call id disambiguation ──
//
// Some serving paths emit tool-call ids that are only unique within a single
// model request (e.g. GPT's call_0, call_1, … resetting every turn), so a
// multi-turn session reuses ids. Tool rows are keyed by id, so without
// disambiguation a reused id silently rewrites the previous turn's finished
// row (already flushed to static scrollback) instead of creating a new one —
// the new tool renders nowhere and its approval prompt shows stale data.
// Both are derived statelessly from the current message list, so there is no
// registry to reset on turn or session boundaries.

/** Newest tool row whose id is `wireId` or a `wireId#N` generation of it. */
function latestToolRowFor(
  messages: ReadonlyArray<MessageType>,
  wireId: string
): (MessageType & { role: MessageRole.ToolUse }) | undefined {
  const prefix = `${wireId}#`;
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (
      m?.role === MessageRole.ToolUse &&
      (m.id === wireId || m.id.startsWith(prefix))
    ) {
      return m;
    }
  }
  return undefined;
}

/**
 * Internal id for an event that may open a tool row. A wire id whose newest
 * matching row is still active belongs to that call (streaming chunk → full
 * tool_call, or a question on a running tool) and merges into it; a wire id
 * whose newest matching row already finished is a NEW call reusing the id and
 * gets the next `wireId#N` generation. Unused ids pass through, so serving
 * paths with globally-unique ids are unaffected.
 */
export function allocateToolCallId(
  messages: ReadonlyArray<MessageType>,
  wireId: string
): string {
  const latest = latestToolRowFor(messages, wireId);
  if (!latest) return wireId;
  if (!latest.isFinished) return latest.id;
  const gen =
    latest.id === wireId ? 0 : Number(latest.id.slice(wireId.length + 1)) || 0;
  return `${wireId}#${gen + 1}`;
}

/**
 * Internal id for a follow-up event (output update, finish, approval
 * request): the newest row for the wire id, or the wire id itself when no
 * row exists yet (synthesized/out-of-order flows).
 */
export function resolveToolCallId(
  messages: ReadonlyArray<MessageType>,
  wireId: string
): string {
  return latestToolRowFor(messages, wireId)?.id ?? wireId;
}

/** Compute a summary message from accumulated init errors. */
export function summarizeInitErrors(errors: InitError[]): string | null {
  if (errors.length === 0) return null;

  const mcpFailures = errors.filter((e) => e.type === 'mcp_failure');
  const agentNotFound = errors.filter((e) => e.type === 'agent_not_found');
  const configErrors = errors.filter((e) => e.type === 'agent_config_error');
  const mcpGovernance = errors.filter(
    (e) => e.type === 'mcp_governance_disabled'
  );
  const webToolsGovernance = errors.filter(
    (e) => e.type === 'web_tools_governance_disabled'
  );
  const parts: string[] = [];

  // MCP governance disabled (show first — important admin notice)
  // When both MCP and web tools fail due to the same GetProfile API failure,
  // coalesce into one message instead of two redundant warnings.
  const mcpGov = mcpGovernance[0];
  const webGov = webToolsGovernance[0];
  if (mcpGov?.apiFailure && webGov?.apiFailure) {
    parts.push(
      'failed to retrieve governance settings — MCP and web tools disabled'
    );
  } else {
    if (mcpGov) {
      parts.push(
        mcpGov.apiFailure
          ? 'failed to retrieve MCP settings — MCP disabled'
          : 'MCP disabled by your administrator'
      );
    }
    if (webGov) {
      parts.push(
        webGov.apiFailure
          ? 'failed to retrieve web tools settings — web tools disabled'
          : 'web tools disabled by your administrator'
      );
    }
  }

  // Agent not found
  if (agentNotFound.length > 0) {
    const e = agentNotFound[0]!;
    parts.push(
      `agent "${e.requestedAgent}" not found, using "${e.fallbackAgent}"`
    );
  }

  // Agent config errors — show up to 3 filenames
  if (configErrors.length > 0) {
    const names = configErrors
      .slice(0, 3)
      .map((e) => (e.path ? basename(e.path) : 'unknown'))
      .join(', ');
    const extra =
      configErrors.length > 3 ? ` +${configErrors.length - 3} more` : '';
    parts.push(`invalid agent config: ${names}${extra}`);
  }

  // MCP failures
  if (mcpFailures.length > 0) {
    parts.push(
      `${mcpFailures.length} MCP failure${mcpFailures.length > 1 ? 's' : ''} — see /mcp`
    );
  }

  return parts.join('; ');
}

/**
 * Pick alert severity from init errors. Governance-disabled is a benign admin
 * notice (warning); everything else is a hard error (red).
 */
export function severityForInitErrors(
  errors: InitError[]
): 'warning' | 'error' {
  const hasHardError = errors.some(
    (e) =>
      e.type !== 'mcp_governance_disabled' &&
      e.type !== 'web_tools_governance_disabled'
  );
  return hasHardError ? 'error' : 'warning';
}

const initialInputBufferState = (): InputBufferState => ({
  lines: [''],
  cursorRow: 0,
  cursorCol: 0,
  preferredCursorCol: 0,
  undoStack: [],
  redoStack: [],
  viewportWidth: 0,
  viewportHeight: 0,
  visibleLines: [],
  logicalToVisibleMap: [],
  visibleToLogicalMap: [],
});

interface AppStoreProps {
  kiro: Kiro;
  agentEngine?: AgentEngine;
  noInteractive?: boolean;
  initialInput?: string;
  trustAllTools?: boolean;
  uiMode?: UiMode;
}

/**
 * Stream event handler with lifecycle controls.
 *
 * Call as a function (`handler(event)`) to dispatch a stream event.
 * `flush()` commits any buffered content to the store — call it on the
 * happy path when a turn completes normally.
 * `dispose()` abandons the handler and drops buffered content — call it
 * from cancel/error paths so stale chunks from the cancelled stream
 * don't leak into the next turn via the handler's batched-flush timers
 * or the ACP SDK's deferred-unsubscribe window.
 */
export interface StreamEventHandler {
  (event: AgentStreamEvent): void;
  /** Commit any buffered assistant content to the store (happy path). */
  flush: () => void;
  /** Abandon the handler and drop buffered content (cancel/error path). */
  dispose: () => void;
  /** Switch tool timing between persisted replay and live delivery. */
  setHistoryReplay: (value: boolean) => void;
  /** Clear per-turn state without retiring the session-lifetime handler. */
  reset: () => void;
  /** Prepare the persistent handler to replay a different session. */
  resetSession: () => void;
}

export type AppActions = BaseAppActions & InputBufferActions;

interface BaseAppActions {
  // Kiro actions
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>,
    displayContent?: string
  ) => Promise<void>;
  createStreamEventHandler: (options?: {
    fromHistory?: boolean;
    cloudReplay?: boolean;
  }) => StreamEventHandler;
  kasSubagentRouting: KasSubagentRoutingStore;
  processMessageStream: (
    stream: AsyncGenerator<AgentStreamEvent>
  ) => Promise<void>;
  cancelMessage: () => Promise<void>;
  setProcessing: (processing: boolean) => void;
  setAgentError: (error: string | null, guidance?: string | null) => void;
  respondToApproval: (
    optionId: string,
    target?: ApprovalRequestInfo,
    _meta?: Record<string, unknown>
  ) => void;
  cancelApproval: () => void;
  respondToQuestion: (
    answer: string,
    target: QuestionRequestInfo,
    answerForAgent?: string
  ) => boolean;
  cancelQuestion: () => void;
  /** Arm (or clear) the `/spec new` description-collection step. */
  setPendingSpecDescription: (pending: PendingSpecDescription | null) => void;
  /**
   * Cancel the description-collection step: drop the pending state (the
   * live intro block disappears with it). Mode is untouched — the user
   * stays in spec and leaves it explicitly, like any other mode.
   */
  cancelPendingSpecDescription: () => void;
  setApprovalMode: (mode: 'dropdown' | 'drill-in') => void;
  setAutoApproveCrewTools: (value: boolean) => void;
  setCurrentModel: (model: { id: string; name: string } | null) => void;
  setCurrentEffort: (effort: string | null) => void;
  /**
   * Begin tracking a KAS session: record whether it is new or resumed and reset
   * the model-change baseline. Called by the session-start paths (boot, `/chat`
   * new/resume, rewind) before the session RPC runs. Returns a restore function
   * that reverts the prior tracking state if that RPC fails.
   */
  beginKasSession: (origin: NonNullable<SessionOrigin>) => () => void;
  /**
   * Handle an incoming `KasModelConfigUpdate` from KAS: refresh the model/effort
   * slices, track the active model, and push the model's saved per-model effort
   * default when the update warrants it.
   */
  handleKasModelConfigEvent: (event: KasModelConfigUpdateEvent) => void;
  setGoalStatus: (
    status: {
      state: string;
      iteration: number;
      maxIterations: number;
      message?: string;
      elapsedSecs?: number;
      startedAt?: number;
    } | null
  ) => void;
  setCurrentAgent: (
    agent: { name: string; welcomeMessage?: string } | null,
    options?: { suppressWelcome?: boolean }
  ) => void;
  setPreviousAgentName: (name: string | null) => void;
  handleCompactionEvent: (event: AgentStreamEvent) => Promise<void>;
  handleTurnSummaryEvent: (event: AgentStreamEvent) => void;

  // Chat actions
  clearMessages: () => void;
  resetMessages: () => void;
  /**
   * Queue a message for later. In QUEUE interrupt mode (and pre-init) the
   * content is appended to the local queue / pending-steer buffer; in STEER
   * mode it is sent to the backend mid-turn via ACP. Duplicates are allowed
   * (re-queuing the same slash command is a valid action). No-op on
   * empty/whitespace-only input.
   */
  queueMessage: (content: string) => void;
  /**
   * Clear staged steer content. With no argument, clears the WHOLE buffer
   * (legacy single-steer behavior). With `targetLine`, removes only that one
   * line from a `\n\n`-joined multi-steer buffer, preserving its siblings —
   * so deleting one staged steer row no longer discards the others.
   */
  clearSteerMessage: (targetLine?: string) => void;
  /**
   * Edit-in-place for the backend steer buffer ("clear-and-resteer"): re-stage
   * the edited steer. Used when the user edits a steer-origin entry in the lite
   * preview list. With `targetLine`, replaces only that one line of a
   * `\n\n`-joined multi-steer buffer (preserving siblings); without it, replaces
   * the whole buffer (single-steer case). Steer content stays the single source
   * of truth in `pendingSteerContent` (never copied into `queuedMessages`) so
   * processQueue can't double-send it. Empty `content` delegates to
   * `clearSteerMessage` (discard that line). No-op when nothing is staged.
   */
  replaceSteerMessage: (content: string, targetLine?: string) => void;
  processQueue: () => Promise<void>;
  clearQueue: () => void;
  removeQueuedMessage: (index: number) => void;
  replaceQueuedMessage: (index: number, content: string) => void;
  startEditingQueue: (index: number) => void;
  cancelEditingQueue: () => void;
  /**
   * Lite mode uses its own input segments rather than commandInputValue, so
   * it tracks the editing index without piping the message text through the
   * store. This is just a flag setter — no side effects. Setting a queue index
   * clears any steer-line editing index (the two are mutually exclusive — the
   * input edits one entry at a time).
   */
  setEditingQueueIndex: (index: number | null) => void;
  /**
   * Flag setter for which steer line the lite input is editing (parallel to
   * `setEditingQueueIndex`). Setting a non-null steer index clears
   * `editingQueueIndex`. No side effects.
   */
  setEditingSteerLineIndex: (index: number | null) => void;
  /**
   * Apply the pending `queuedInputRestore` snapshot back into
   * `commandInputValue` and `input`, then clear the snapshot. No-op when
   * `queuedInputRestore` is null. See the field's doc comment for the
   * full lifecycle. Called from a `useLayoutEffect` in `LiteLayout` on
   * `activeCommand` transitions non-null → null.
   */
  applyQueuedInputRestore: () => void;
  setSlashCommands: (commands: SlashCommand[]) => void;
  setKasCommands: (commands: KasCommand[]) => void;
  setPrompts: (prompts: PromptEntry[]) => void;
  setSkills: (skills: SkillEntry[]) => void;
  setSteering: (steering: SteeringEntry[]) => void;
  setKasAvailableAgents: (agents: AgentEntry[]) => void;

  // Command UI actions
  setActiveCommand: (command: ActiveCommand | null) => void;
  executeCommandWithArg: (arg: string) => Promise<void>;
  /** Resume the session chosen in the `/sessions` panel (synthetic `/chat <id>`). */
  resumeSession: (
    sessionId: string,
    environment?: 'local' | 'cloud'
  ) => Promise<void>;
  setCommandInput: (value: string) => void;
  setActiveTrigger: (
    trigger: { key: string; position: number; type: 'start' | 'inline' } | null
  ) => void;
  setFilePickerHasResults: (hasResults: boolean) => void;
  setPromptHint: (hint: string | null) => void;
  setCommandShadowText: (text: string | null) => void;
  clearCommandInput: () => void;
  voiceStop: (() => void) | null;
  setVoiceStop: (fn: (() => void) | null) => void;
  voiceCancel: (() => void) | null;
  setVoiceCancel: (fn: (() => void) | null) => void;
  voiceLevel: number | null;
  setVoiceLevel: (level: number | null) => void;
  voiceAutoSubmit: boolean;
  toggleVoiceAutoSubmit: () => void;
  voiceHintIndex: number;
  incrementVoiceHint: () => void;
  voicePartialText: string | null;
  setVoicePartialText: (text: string | null) => void;
  pendingVoiceText: string | null;
  setPendingVoiceText: (text: string | null) => void;

  navigateHistory: (direction: 'up' | 'down') => string | null;

  // UI actions
  setMode: (
    mode: 'inline' | 'expanded' | 'crew-monitor' | 'session-view'
  ) => void;
  setUiMode: (uiMode: UiMode, notice?: string) => void;
  /** Lite-only: see {@link LiteState.staticSkipBefore}. */
  setLiteStaticSkipBefore: (idx: number) => void;
  addSubagentSession: (info: SubagentInfo) => void;
  updateSubagentSession: (sessionId: string, status: SubagentStatus) => void;
  pushSessionEvent: (sessionId: string, event: AgentStreamEvent) => void;
  addSession: (session: AgentSession) => void;
  updateSession: (id: string, updates: Partial<AgentSession>) => void;
  removeSession: (id: string) => void;
  cleanupTerminatedSession: (sessionId: string) => void;
  terminateAllCrewSessions: () => Promise<void>;
  setActiveSession: (id: string) => void;
  setSelectedSession: (id: string) => void;
  toggleCrewMonitor: () => void;
  incrementExitSequence: () => void;
  resetExitSequence: () => void;
  armSuspend: () => void;
  disarmSuspend: () => void;
  showTransientAlert: (alert: TransientAlert) => void;
  dismissTransientAlert: () => void;
  setRetryStatus: (status: RetryStatus | null) => void;
  setLoadingMessage: (message: string | null) => void;
  toggleToolOutputsExpanded: () => void;
  setHasExpandableToolOutputs: (has: boolean) => void;

  // Context usage actions
  setContextUsage: (percent: number) => void;
  /**
   * Merge a `_kiro/sessions/changed` delta into the roster and re-derive the
   * attached session's cloud status (null when the roster no longer tracks it,
   * so a retracted session clears the footer instead of going stale).
   */
  applySessionRosterDelta: (delta: SessionsChangedNotification) => void;
  /** Set the bound repo for the cloud footer; null when not a cloud session / New empty sandbox. */
  setCloudRepo: (repo: string | null) => void;
  /** Project an attached-repo set onto the footer (first repo + `(+N others)`)
   *  and the /repo pre-check list, in one place. When `branch` is given it
   *  lands in the same store update (no stale-branch frame between the two). */
  applyRepoFooter: (repos: string[], branch?: string | null) => void;
  /** Apply a sandbox-reported bound-repo set (footer + branch, one update).
   *  Cloud-gated so a stray report can never paint a repo onto a local
   *  session's footer. */
  applySessionRepositories: (repositories: SessionRepositoryEntry[]) => void;
  /** Clear the per-session cloud scope (bound repo, branch, extras, attached
   *  set) when switching to a different session — these describe ONE sandbox
   *  and must not leak into the next session's footer/picker. */
  resetCloudSessionScope: () => void;
  /** Snapshot the current cloud scope keyed by session id before switching
   *  away, so switching back restores the footer's repo/branch. */
  stashCloudSessionScope: (sessionId: string | null | undefined) => void;
  /** Restore a previously stashed cloud scope for `sessionId`.
   *  Returns whether a stash was found and applied. */
  restoreCloudSessionScope: (sessionId: string | null | undefined) => boolean;
  /** Set the bound repo's default branch for the cloud footer. */
  setCloudBranch: (branch: string | null) => void;
  /** Set the connected source provider display name for the startup checklist. */
  setCloudProvider: (provider: string | null) => void;
  /** Set the repository count for the startup checklist. */
  setCloudRepoCount: (count: number | null) => void;
  /** Arm/disarm the post-`/chat new` cloud checklist (see cloudNewSessionChecklist). */
  setCloudNewSessionChecklist: (armed: boolean) => void;
  /** Set the count of extra bound repos for the footer's `(+N others)` suffix. */
  setCloudExtraRepos: (count: number) => void;
  setKasMessageId: (kasMessageId: string) => void;
  setLastTurnTokens: (tokens: LastTurnTokens) => void;
  toggleContextBreakdown: () => void;
  setShowContextBreakdown: (
    show: boolean,
    breakdown?: ContextBreakdownData
  ) => void;
  setShowTuiPanel: (show: boolean) => void;
  setShowChangelogPanel: (show: boolean) => void;
  setShowHelpPanel: (
    show: boolean,
    commands?: Array<{
      name: string;
      description: string;
      usage: string;
      subcommands?: string[];
    }>
  ) => void;
  setShowUsagePanel: (show: boolean, data?: any) => void;
  setShowRewindExplorer: (show: boolean, rows?: RewindTurn[]) => void;
  setShowTangentExplorer: (
    show: boolean,
    rows?: Array<{
      id: string;
      label: string;
      title: string;
      isCurrent: boolean;
      isTangent: boolean;
      lastActive?: string;
    }>
  ) => void;
  setTangentName: (name: string | null) => void;
  setUpgradeDiagnostics: (
    rows: UpgradeAnalysisRow[],
    description: string
  ) => void;
  /** Map of picker bucket value → agent names, for the /upgrade-agent run preview. */
  setUpgradeRunPreview: (preview: Record<string, string[]>) => void;
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string,
    registryServers?: McpServerInfo[]
  ) => void;
  /** Clear display snapshots derived from the active transport client. */
  resetClientDisplayCaches: () => void;
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  /** Update the cached session tool listing without toggling the panel. */
  setToolsList: (tools: ToolInfo[]) => void;
  /** Merge live MCP server statuses into the store (updates open panel). */
  updateMcpServerStatuses: (
    servers: Array<{ name: string; status: string; toolCount: number }>
  ) => void;
  setShowGoalPanel: (show: boolean) => void;
  setShowStatsPanel: (
    show: boolean,
    stats?: RequestStat[],
    summary?: StatsSummary | null
  ) => void;
  setShowHooksPanel: (show: boolean, hooks?: HookInfo[]) => void;
  setShowRepoPicker: (
    show: boolean,
    resources?: SourceProviderResource[]
  ) => void;
  /** Attach the selected repos (emulated clone) and close the `/repo` picker. */
  submitRepoPicker: (selected: string[]) => Promise<void>;
  /** Open/close the cloud-entry source-provider gate with the setup URL. */
  setShowSourceProviderGate: (show: boolean, setupUrl?: string | null) => void;
  setCloudProviderChecked: (checked: boolean) => void;
  /** Re-probe the source-provider connection; dismisses the gate when now
   *  connected, otherwise leaves it up. Returns whether a provider is connected. */
  retrySourceProviderConnection: () => Promise<boolean>;
  /** Open/close the `/sessions` picker. `invokedAs` is the slash command the
   *  user actually typed (`/chat` or `/sessions`), echoed as the panel title. */
  setShowSessionPicker: (
    show: boolean,
    rows?: SessionPickerRow[],
    invokedAs?: string
  ) => void;
  setShowKeybindingsPanel: (show: boolean) => void;
  setShowDisplaySettingsPanel: (show: boolean) => void;
  setShowThemePanel: (show: boolean) => void;
  /** Open/close the cloud-session `/quit` prompt (keep-running vs turn-off). */
  setShowCloudQuitPrompt: (show: boolean) => void;
  setShowSettingsPanel: (show: boolean) => void;
  /** Set whether the current session is a cloud session (gates `/repo`). */
  setCloudSessionActive: (active: boolean) => void;
  setSettingsReturnOnEscape: (value: boolean) => void;
  /** Set the parent route consumed by the verbose menu's ESC handler. */
  setVerboseReturnOnEscape: (route: string | null) => void;
  /** Set the parent route consumed by the /theme menu's ESC handler. */
  setThemeReturnOnEscape: (route: string | null) => void;
  reopenSettingsMenu: () => void;
  setShowKnowledgePanel: (
    show: boolean,
    entries?: KnowledgeEntry[],
    status?: string
  ) => void;
  setShowCodePanel: (show: boolean, data?: CodePanelData) => void;

  // ── Spec artifact view actions ─────────────────────────────
  /**
   * Record an `fs_write` to a spec artifact path. Triggers a background
   * load of the artifact summary so the generation card can render the
   * latest state. Idempotent — calling twice for the same path with the
   * same content is safe.
   */
  notifyArtifactGenerationWrite: (args: {
    path: string;
    featureName: string;
    artifact: ArtifactKind;
  }) => void;
  /** Mark a generation-phase entry as complete (transitions card to its post-write state). */
  markArtifactGenerationComplete: (path: string) => void;
  /**
   * Re-parse a tracked spec-artifact entry from disk. Called when the
   * agent's write tool call finishes — the file is now fully flushed,
   * so a fresh parse is more authoritative than whatever interim state
   * the mid-stream parse captured. No-op when no entry is tracked for
   * `path` (e.g. after the user closed the panel or switched engines).
   */
  reparseArtifactGeneration: (path: string) => void;
  /** Open the artifact view panel; loads the summary and sets `artifactViewOpen`. */
  openArtifactView: (
    featureName: string,
    artifact: ArtifactKind
  ) => Promise<void>;
  /** Close the panel and clear cursor/expansion state. */
  closeArtifactView: () => void;
  moveArtifactCursor: (direction: 'prev' | 'next') => void;
  toggleArtifactExpand: (index: number) => void;
  enterArtifactDetail: () => void;
  leaveArtifactDetail: () => void;
  /**
   * Reset all artifact-view state on engine switch. The agent engine in
   * Kiro CLI is fixed at startup (see `kas-commands.ts`), so this is
   * a defence-in-depth no-op-safe action: callable from anywhere
   * without breaking the store.
   */
  clearArtifactViewOnEngineSwitch: () => void;
  // ── End spec artifact view actions ─────────────────────────

  // File attachment actions
  attachFile: (path: string) => void;
  removeAttachedFile: (path: string) => void;
  clearAttachedFiles: () => void;

  // User theme color callback (set by ThemeProvider bridge)
  _userColorsSetter:
    | ((prompt?: any, response?: any, diff?: any) => void)
    | null;
  registerUserColorsSetter: (
    setter: (prompt?: any, response?: any, diff?: any) => void
  ) => void;

  // Base theme setter (set by ThemeProvider bridge)
  _baseThemeSetter: ((theme: any) => void) | null;
  registerBaseThemeSetter: (setter: (theme: any) => void) => void;

  // Theme diff color getter (set by ThemeProvider bridge)
  _themeDiffHexGetter:
    | (() => {
        added: {
          background: TerminalColor;
          bar: TerminalColor;
          highlight: TerminalColor;
        };
        removed: {
          background: TerminalColor;
          bar: TerminalColor;
          highlight: TerminalColor;
        };
      })
    | null;
  registerThemeDiffHexGetter: (
    getter: () => {
      added: {
        background: TerminalColor;
        bar: TerminalColor;
        highlight: TerminalColor;
      };
      removed: {
        background: TerminalColor;
        bar: TerminalColor;
        highlight: TerminalColor;
      };
    }
  ) => void;

  // Auto preview getter (set by ThemeProvider bridge)
  _autoPreviewGetter: (() => string) | null;
  registerAutoPreviewGetter: (getter: () => string) => void;

  setPendingFileAttachment: (
    path: string | null,
    triggerPosition?: number
  ) => void;
  consumePendingFileAttachment: () => {
    path: string;
    triggerPosition: number;
  } | null;

  // Image attachment actions
  addPendingImage: (image: {
    base64: string;
    mimeType: string;
    width: number;
    height: number;
    sizeBytes: number;
  }) => void;
  removePendingImage: (index: number) => void;
  clearPendingImages: () => void;

  // Dual-mode interrupt behavior actions
  toggleInterruptMode: () => void;
  setActiveInterruptMode: (mode: InterruptMode) => void;

  // Task management actions
  setTasks: (tasks: TaskItem[]) => void;
  toggleActivityTray: () => void;

  // Announcement actions
  setAnnouncement: (msg: { id: string; maxLines: number } | null) => void;
  toggleAnnouncementExpanded: () => void;

  // Dispatch a slash command with an optional human-readable form for recall history
  dispatchSlashCommand: (execCmd: string, recordAs?: string) => Promise<void>;

  // Main orchestrator
  handleUserInput: (input: string) => Promise<void>;

  // Trust all tools acceptance
  confirmTrustAllTools: () => void;

  // Research survey actions
  /** Increment the counter we use to decide when to first show the prompt. */
  recordCompletedTurn: () => void;
  /** User accepted the notification and wants to take the survey. */
  openSurveyPanel: (survey?: SurveyDefinition) => void;
  /** Close the panel without submitting — treated as "in progress / ignored". */
  closeSurveyPanel: () => void;
  /** Called after the user submits answers; persists completion timestamp. */
  submitSurvey: (answers: Record<string, string>) => void;
  /** User dismissed the notification bar without opening the panel. */
  dismissSurveyPrompt: () => void;
  /** Trigger the plan-quality survey (called on plan mode exit / handoff). */
  triggerPlanSurvey: () => void;
  /** Trigger the implement-plan survey (called after all plan tasks complete). */
  triggerImplementPlanSurvey: () => void;
}

export const AppStoreContext = createContext<AppStoreApi | null>(null);

export type AppStoreApi = ReturnType<typeof createAppStore>;

/** A KAS session's lifecycle origin; null until it has been established. */
export type SessionOrigin = 'new' | 'resumed' | null;

/**
 * KAS-engine-specific store state, grouped so it is clear at a glance which
 * fields belong to the KAS agent path (the Rust V2 engine populates none of
 * these). Three concerns (so far):
 *
 * 1. Config-option caches (`available*`): the model / agent / effort options
 *    parsed from the ACP `configOptions` payload on session/new, session/load,
 *    set_config_option responses, and `config_option_update` notifications.
 *    Unlike V2 (which authoritatively round-trips the Rust backend on every
 *    `/model` and `/agent` invocation), KAS embeds these in session responses,
 *    so the TUI retains them here for the `/model`, `/agent`, and `/effort`
 *    menus.
 * 2. Per-model effort-default tracking (`sessionOrigin`, `previousModelId`,
 *    `effortExplicit`): drives when a model's saved effort default is
 *    auto-applied. Intended behavior — apply the saved default when the active
 *    model changes and either (a) the session is new, so a fresh launch adopts
 *    each model's saved default (unless the user passed an explicit `--effort`,
 *    which then wins), or (b) the user explicitly switched models mid-session.
 *    Never apply on a resumed session or on an autonomous backend change, so a
 *    resumed session and any hand-set effort are left as-is. See the individual
 *    fields below for how each contributes.
 * 3. Subagent routing state: correlates KAS tool events with the main
 *    transcript, crew panel, and independent subagent streams. These
 *    collections are UI state, not ACP transport state.
 */
export interface KasState {
  availableModels: ModelEntry[];
  availableAgents: AgentEntry[];
  availableEfforts: EffortEntry[];
  /**
   * Whether the active session is new or resumed.
   */
  sessionOrigin: SessionOrigin;
  /**
   * The active model id from the previous `KasModelConfigUpdate`. Used only to
   * detect when the model changes between updates, which gates re-applying the
   * model's saved effort default.
   */
  previousModelId: string | null;
  /**
   * Whether the process was launched with an explicit `--effort` flag. Used to
   * suppress the new-session auto-apply so the explicit flag wins.
   */
  effortExplicit: boolean;
  /** Non-reactive correlation state; use `kasSubagentRouting` actions to mutate it. */
  subagentRouting: KasSubagentRoutingState;
}

export interface LiteState {
  /**
   * Lower bound into `messages` for Lite resume rendering. Resume paths cap
   * replayed history with this index; mode changes reset it to 0 so the
   * destination UI renders the complete in-memory conversation.
   */
  staticSkipBefore: number;
  /**
   * Lite-mode banners that should fire once per *session*, not per *mount*.
   * Lives on the store (not a useRef in LiteLayout) so flipping /tui ↔ /lite
   * doesn't re-emit the same banner every time LiteLayout remounts.
   */
  welcomeEmitted: boolean;
  mcpFailureWarningEmitted: boolean;
  /**
   * Bumped when scrollback should be wiped and the lite render cache reset
   * (currently only on /chat <id> + /rewind session swaps). LiteLayout
   * subscribes to the token; the value itself is opaque — only the change
   * matters. See effects.ts:loadSession + LiteLayout.tsx for the consumer.
   */
  scrollbackClearToken: number;
}

export interface AppState {
  // Chat state
  messages: MessageType[];
  liveOutputs: Map<string, string[][]>;
  pendingSteerContent: string | null;
  /**
   * Pending input restore for queued slash-command drains that opened a
   * picker (e.g. `/model`, `/agent`). When the user has typed pending
   * text in the prompt while a slash command is queued, processQueue
   * snapshots that text before dispatching. If the dispatch opens a
   * picker (i.e. `activeCommand` becomes non-null after the await), an
   * inline restore would be invisible — PromptInput renders
   * `activeCommand.command.name` instead of segments while the picker is
   * up, AND the picker's close handlers (`handleActiveCommandClose` and
   * the no-hint `onSelect` branch) call `clearCommandInput()` which
   * would clobber any restored value the moment the picker dismisses.
   *
   * Stash the snapshot here instead. A `useLayoutEffect` in `LiteLayout`
   * watches `activeCommand` transitions from non-null → null and applies
   * the restore via `applyQueuedInputRestore()`. That path catches BOTH
   * Esc-dismissal (handleActiveCommandClose) and selection
   * (executeCommandWithArg → set activeCommand:null) without either
   * close-handler having to know about the queue-drain context.
   *
   * Stays null in the common case (no slash queued, or queued slash is a
   * non-picker command like `/verbose`). For non-picker commands the
   * existing inline restore in processQueue still applies.
   */
  queuedInputRestore: {
    commandInputValue: string;
    input: InputBufferState;
  } | null;
  /**
   * V2 slash commands only. Two cohorts:
   * 1. Hardcoded TUI host commands (`source: 'local'`), seeded at store
   *    init and never replaced. Visible in both engines via
   *    `selectVisibleSlashCommands`.
   * 2. V2 backend slash commands (`source: 'backend'`, no `meta.type`),
   *    populated by `setSlashCommands` from the
   *    `kiro.dev/commands/available` `commands` field in
   *    `BaseAcpClient.handleCommandsAdvertising`. Empty in KAS mode.
   *
   * Does NOT contain prompts, skills, or steering -- those live in their
   * own slices (`prompts`, `skills`, `steering`) and are merged with
   * `slashCommands` only at the autocomplete-selector layer.
   */
  slashCommands: SlashCommand[];
  /**
   * Static, TUI-owned KAS commands. Seeded from `KAS_COMMANDS` at boot
   * when `agentEngine === 'kas'`; empty otherwise. The dispatcher checks
   * this list first in KAS mode so KAS-side handlers take precedence
   * over the V2 dispatcher pipeline for the same command name.
   */
  kasCommands: KasCommand[];
  /** True for cloud sessions; gates cloud-only slash commands. */
  cloudSessionActive: boolean;
  /** Frozen at boot from props.agentEngine ?? process.env.KIRO_AGENT_ENGINE. */
  agentEngine: AgentEngine;
  /**
   * User-invocable prompt templates. Replace-on-update via
   * `setPrompts` from `kiro.onPromptsUpdate`. Populated by both engines:
   *
   * - V2: `BaseAcpClient.handleCommandsAdvertising` ingests the
   *   `kiro.dev/commands/available` payload, partitions out skill entries
   *   (`server_name` carrying the `skill:` prefix), and maps the rest's
   *   `server_name` to a discriminated `PromptSource` (`local` ->
   *   `workspace`, `global` -> `global`, otherwise `mcp`).
   * - KAS: `KasAcpClient.handleSessionUpdate` partitions
   *   `available_commands_update` entries by `_meta.kiro.type === 'prompt'`.
   *
   * Merged into `selectVisibleSlashCommands` at read time; not stored in
   * `slashCommands`.
   */
  prompts: PromptEntry[];
  /**
   * Skills. Replace-on-update via `setSkills` from `kiro.onSkillsUpdate`.
   * Populated by both engines:
   *
   * - V2: same `kiro.dev/commands/available` payload as prompts; entries
   *   whose `server_name` carries a `skill:` prefix are routed here at
   *   the TUI ingest boundary.
   * - KAS: `available_commands_update` entries with
   *   `_meta.kiro.type === 'skill'`.
   *
   * Merged into `selectVisibleSlashCommands` at read time.
   */
  skills: SkillEntry[];
  /**
   * KAS steering documents. Replace-on-update via `setSteering` from
   * `kiro.onSteeringUpdate`, populated from
   * `available_commands_update` entries with
   * `_meta.kiro.type === 'steering'`. Stays `[]` in V2 mode (V2 has no
   * steering concept).
   */
  steering: SteeringEntry[];
  /** KAS-engine-specific state; see {@link KasState}. Empty/null in V2 mode. */
  kas: KasState;

  // Kiro/Agent state
  kiro: Kiro;
  onExit?: () => void;
  sessionId: string | null;
  isProcessing: boolean;
  isCompacting: boolean;
  activeCompactionAttemptKey: number | null;
  compactionReportAnchor: { attemptKey: number; index: number } | null;
  wasCancelled: boolean;
  /** True when the most recent agent turn ended in an error (blocking or
   *  transient). Lets a caller that fired a turn (e.g. the /repo attach)
   *  distinguish "completed" from "failed" without parsing messages. */
  lastTurnErrored: boolean;
  agentError: string | null;
  agentErrorGuidance: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  approvalQueue: ApprovalRequestInfo[];
  pendingQuestion: QuestionRequestInfo | null;
  questionQueue: QuestionRequestInfo[];
  /**
   * Armed by `/spec new <name>`: the next submitted line is the feature
   * description for the spec kickoff prompt, not a chat message. The intro
   * block renders from this state in the live region (never the transcript)
   * so cancelling leaves no trace.
   */
  pendingSpecDescription: PendingSpecDescription | null;
  approvalMode: 'dropdown' | 'drill-in';
  autoApproveCrewTools: boolean;
  focusedCrewIndex: number;
  setFocusedCrewIndex: (index: number) => void;
  currentModel: { id: string; name: string } | null;
  currentEffort: string | null;
  currentAgent: { name: string } | null;
  previousAgentName: string | null;
  settings: Record<string, unknown> | null;
  goalStatus: {
    state: string;
    iteration: number;
    maxIterations: number;
    message?: string;
    elapsedSecs?: number;
    startedAt?: number;
  } | null;

  // Command UI state
  activeCommand: ActiveCommand | null;
  commandInputValue: string;
  activeTrigger: {
    key: string;
    position: number;
    type: 'start' | 'inline';
  } | null;
  filePickerHasResults: boolean;
  promptHint: string | null;
  commandShadowText: string | null;

  // Input state
  input: InputBufferState;
  reverseSearchActive: boolean;
  setReverseSearchActive: (active: boolean) => void;

  // UI state
  mode: 'inline' | 'expanded' | 'crew-monitor' | 'session-view';
  sessions: Map<string, AgentSession>;
  activeSessionId: string;
  selectedSessionId?: string;
  crewMonitorVisible: boolean;
  sessionEventBuffer: Record<string, AgentStreamEvent[]>;
  exitSequence: number;
  exitTimer: NodeJS.Timeout | null;
  suspendArmed: boolean;
  suspendTimer: NodeJS.Timeout | null;
  transientAlert: TransientAlert | null;
  /**
   * Active HTTP-retry banner, shown inline with the thinking spinner. `null` when no
   * retry is in flight. Cleared on cancel, on next request, and when the turn ends.
   */
  retryStatus: RetryStatus | null;
  /** Streaming thinking/reasoning content from the agent (cleared on turn end). */
  thinkingContent: string;
  /**
   * In-flight model content for the current streaming row. Updated per chunk
   * by the stream handler INSTEAD of mutating `messages[messages.length - 1]`,
   * which previously triggered a full `[...state.messages]` shallow-copy on
   * every 16ms flush — at session length 200+ that meant ~1000 array
   * reallocations/sec during streaming and forced every memo with `[messages]`
   * deps to invalidate. Components rendering the live row (LiteLiveRegion's
   * <Static> tail, modern TUI's StreamingMessage) subscribe to this slot
   * directly. The handler still appends a single Model row to `messages` on
   * first content of a turn (so layout/order is correct) and commits the
   * accumulated content into that row at turn end via `flushContentToStore`.
   * Empty string when no Model row is streaming.
   */
  streamingContent: string;
  /** Stable id of the in-flight Model row that `streamingContent` belongs to.
   *  Lets readers correlate the live string to the right row in `messages`
   *  without inferring it from the array tail. Null when nothing streaming. */
  streamingMessageId: string | null;
  loadingMessage: string | null;
  toolOutputsExpanded: boolean; // Global toggle for all tool outputs
  hasExpandableToolOutputs: boolean; // Whether there are any tool outputs that can be expanded

  // Context usage state
  contextUsagePercent: number | null;
  /** Live cloud session activity status for the status-line badge. */
  cloudSessionStatus: SessionActivityStatus | null;
  /** Provisioning-failure detail for the attached cloud session, when its status is `failed`. */
  cloudProvisioningFailure: { code: ProvisioningFailureCode } | null;
  /** Live roster of sessions from `_kiro/sessions/changed`; empty until KAS pushes deltas. */
  sessionRoster: ReadonlyMap<string, RosterEntry>;
  /** Repo bound to the cloud session, for the footer location indicator. */
  cloudRepo: string | null;
  /** Default branch of the bound repo, for the footer; null until known. */
  cloudBranch: string | null;
  /** Connected source provider display name (e.g. "GitHub") for the startup checklist. */
  cloudProvider: string | null;
  /** Number of repositories the connected provider exposes, for the startup checklist; null until known. */
  cloudRepoCount: number | null;
  /** Post-`/chat new` creation checklist, shown until the first message. */
  cloudNewSessionChecklist: boolean;
  /** One-way latch: true once the conversation has ever been non-empty.
   *  Store-held so it survives layout remounts (mode switches, lite↔tui). */
  hasEnteredConversation: boolean;
  /** Count of bound repos beyond the one shown in the footer (the `(+N others)` suffix). 0 = single/none. */
  cloudExtraRepos: number;
  /** Per-session snapshots of the cloud scope (repo/branch/extras/attached),
   *  so switching back to a session restores its footer without a re-fetch. */
  cloudScopeBySession: ReadonlyMap<
    string,
    {
      cloudRepo: string | null;
      cloudBranch: string | null;
      cloudExtraRepos: number;
      attachedRepos: string[];
    }
  >;
  lastTurnTokens: LastTurnTokens | null;
  turnSummaries: Map<string, string>; // turnId (user message id) → formatted summary text

  // Usage panel state
  showUsagePanel: boolean;
  usageData: UsageData | null;

  // Rewind explorer state
  showRewindExplorer: boolean;
  rewindRows: RewindTurn[];
  // Tangent explorer state
  showTangentExplorer: boolean;
  tangentRows: Array<{
    id: string;
    label: string;
    title: string;
    isCurrent: boolean;
    isTangent: boolean;
    lastActive?: string;
  }>;
  /** Name of the current tangent (null if on root session). */
  tangentName: string | null;

  // /upgrade-agent diagnostics data (rendered by UpgradeDiagnosticsMenu)
  upgradeAnalysisRows: UpgradeAnalysisRow[];
  upgradeAnalysisDescription: string;
  // /upgrade-agent run picker: bucket value → agent names (preview panel)
  upgradeRunPreview: Record<string, string[]>;

  // File attachments
  attachedFiles: string[];
  pendingFileAttachment: { path: string; triggerPosition: number } | null;
  pendingImages: Array<{
    base64: string;
    mimeType: string;
    width: number;
    height: number;
    sizeBytes: number;
  }>;
  showContextBreakdown: boolean;
  contextBreakdown: ContextBreakdownData | null;
  /** Raw context-usage snapshot pushed by the agent, independent of panel UI. */
  contextBreakdownCache: ContextBreakdownData | null;
  showTuiPanel: boolean;
  showChangelogPanel: boolean;
  showHelpPanel: boolean;
  helpCommands: Array<{
    name: string;
    description: string;
    usage: string;
    subcommands?: string[];
  }>;
  showMcpPanel: boolean;
  mcpServers: McpServerInfo[];
  mcpRegistryServers: McpServerInfo[];
  /** Latest KAS configured-server snapshot, independent of the open panel. */
  mcpServerCache: McpServerInfo[];
  /** Latest KAS registry snapshot, independent of the open panel. */
  mcpRegistryCache: McpServerInfo[];
  pendingOAuthServers: Map<string, string>; // serverName → oauthUrl
  initErrors: InitError[];
  /**
   * Per-MCP server init status. Feeds the boot indicator's aggregate
   * "Loading N/M MCP server(s)" row (selectBootIndicatorPhase) — individual
   * servers are no longer surfaced; failures route to a transient alert and
   * `/mcp` shows full per-server detail. Only `status` and `startTime` are
   * read (the indicator derives elapsed from `startTime` itself).
   */
  mcpInitStatus: Map<
    string,
    {
      status: 'loading' | 'ready' | 'failed';
      startTime: number;
    }
  >;
  /**
   * Pre-MCP boot stages (agent process spawn, ACP initialize handshake,
   * session create). These take priority over the MCP aggregate in the boot
   * indicator (selectBootIndicatorPhase) so the user sees progress through
   * the otherwise-opaque "connecting" window. Order matters — Map iteration
   * order is insertion order.
   */
  bootProgress: Map<
    string,
    {
      label: string;
      status: 'loading' | 'ready' | 'failed';
      startTime: number;
      elapsed?: number;
      error?: string;
    }
  >;
  setBootStage: (
    key: string,
    label: string,
    status: 'loading' | 'ready' | 'failed',
    error?: string
  ) => void;
  mcpMode: string;
  showToolsPanel: boolean;
  toolsList: ToolInfo[];
  showGoalPanel: boolean;
  showStatsPanel: boolean;
  statsList: RequestStat[];
  statsSummary: StatsSummary | null;
  showHooksPanel: boolean;
  hooksList: HookInfo[];
  /** `/repo` picker (cloud-only): open flag + the fetched repositories to choose from. */
  showRepoPicker: boolean;
  repoPickerResources: SourceProviderResource[];
  /** Repos attached to the cloud session, so reopening /repo pre-checks them. */
  attachedRepos: string[];
  /** True once the cloud-entry source-provider probe has resolved (connected or
   *  gate shown). Gates the connecting screen's welcome/checklist so neither
   *  flashes before the provider decision is made. */
  cloudProviderChecked: boolean;
  /** Cloud-entry gate shown when no source provider is connected: open flag + the setup URL. */
  showSourceProviderGate: boolean;
  sourceProviderSetupUrl: string | null;
  /** `/sessions` picker: open flag + the merged local/cloud rows. */
  showSessionPicker: boolean;
  sessionPickerRows: SessionPickerRow[];
  /** Panel title = the command the user typed (`/chat` or `/sessions`). */
  sessionPickerTitle: string;
  showKeybindingsPanel: boolean;
  showDisplaySettingsPanel: boolean;
  showThemePanel: boolean;
  /** Whether the cloud-session `/quit` prompt (keep-running vs turn-off) is open. */
  showCloudQuitPrompt: boolean;
  showSettingsPanel: boolean;
  /** Theme preview string rendered below the /theme menu during the lite flow. */
  themePreview: string | null;
  /** Set the lite /theme preview string (null clears it). */
  setThemePreview: (preview: string | null) => void;
  terminalTitleEnabled: boolean;
  setTerminalTitleEnabled: (enabled: boolean) => void;
  /**
   * When true, closing the currently open overlay re-opens the /settings
   * top-level menu instead of fully dismissing. Set by /settings subcommand
   * handlers before they hand off to showThemeMenu / setShowKeybindingsPanel,
   * consumed by the ESC handlers (CommandMenu and handleCloseKeybindingsPanel),
   * and reset whenever consumed.
   */
  settingsReturnOnEscape: boolean;
  /**
   * Parent route to re-dispatch on ESC from a /verbose sub-menu. Set by the
   * verboseConfig handler when it opens any non-root menu (e.g. the tool
   * sub-menu sets this to `'menu:top:tool'`). CommandMenu's escape handler clears
   * the active overlay, then if this is non-null, it re-runs `/verbose` with
   * the saved route so the user lands one level up instead of dropping out
   * of the entire menu. Cleared on consume and on full menu exit.
   */
  verboseReturnOnEscape: string | null;
  /**
   * Parent args to re-dispatch on ESC from a /theme sub-menu. The /theme menu
   * has three levels (top → custom → prompt|response|diff); without this
   * flag, ESC from any submenu drops the entire overlay. Set by
   * `showThemeMenu` at every menu-open path:
   *   - top-level (`/theme`): null — ESC closes the overlay.
   *   - `/theme custom`: `''` — ESC re-dispatches `/theme` (top level).
   *   - `/theme prompt|response|diff`: `'custom'` — ESC re-dispatches
   *     `/theme custom`.
   * CommandMenu's escape handler consumes and clears the flag, then re-runs
   * `/theme` (or `/theme <route>`) via handleUserInput.
   */
  themeReturnOnEscape: string | null;
  showKnowledgePanel: boolean;
  knowledgeEntries: KnowledgeEntry[];
  knowledgeStatus: string | null;
  showCodePanel: boolean;
  codeData: CodePanelData | null;
  codeIntelligenceActive: boolean;

  // ── Spec artifact view state ───────────────────────────────
  /**
   * Generation-phase tracker for the live artifact-generation card.
   * Holds at most one active entry at a time — switching to a new
   * artifact replaces the prior entry. The entry's `absolutePath`
   * doubles as the dedup key for idle timers.
   *
   * `null` when no spec artifact is being written. Entries are
   * created when the agent's first matching `fs_write` arrives and
   * flip to `complete: true` after 2 s of no writes (or when the
   * tool's `ToolCallFinished` event arrives).
   */
  artifactGenerating: ArtifactGenerationEntry | null;
  /**
   * Open artifact-view panel. Mutually exclusive with the generation
   * card path: the card can show alongside, but the panel takes over
   * keyboard input.
   */
  artifactViewOpen: OpenArtifactView | null;
  // ── End spec artifact view state ───────────────────────────

  // Dual-mode interrupt behavior state
  activeInterruptMode: InterruptMode;
  queuedMessages: string[];
  editingQueueIndex: number | null;
  /**
   * Which steer line (index into `pendingSteerContent.split('\n\n')`) the lite
   * input is currently editing, or null. Parallel to `editingQueueIndex` but
   * for steer-origin rows in the unified preview list — lets LiteLayout draw
   * the edit chevron on the right steer row. Steer and queue editing are
   * mutually exclusive (the input edits one entry at a time).
   */
  editingSteerLineIndex: number | null;

  // Task management state
  tasks: TaskItem[];
  activityTrayExpanded: boolean;

  // Voice state
  voiceStop: (() => void) | null;
  voiceCancel: (() => void) | null;
  voiceLevel: number | null;
  voiceAutoSubmit: boolean;
  voiceHintIndex: number;
  voicePartialText: string | null;
  pendingVoiceText: string | null;

  // Announcement state
  announcement: { id: string; maxLines: number } | null;
  announcementExpanded: boolean;

  // Abort controller for current stream
  currentAbortController: AbortController | null;
  cancelInProgress: Promise<void> | null;
  isShellEscape: boolean;
  _shellEscapeWriter: ((data: string) => void) | null;

  /** In-flight local renderer, retired synchronously on cancellation. */
  _activeStreamHandler: StreamEventHandler | null;

  /** Session-lifetime renderer for turns this client did not submit. */
  _liveStreamHandler: StreamEventHandler | null;
  setLiveStreamHandler: (handler: StreamEventHandler) => void;

  /** Recent optimistic rows used to suppress backend echoes. */
  _recentLocalUserMessages: {
    content: string;
    sentContent?: string;
    at: number;
  }[];

  /** True while an observer auth/session error blocks automatic queue drains. */
  _observerQueueBlocked: boolean;

  /** True only when the backend dropped a steer that must replay as a prompt. */
  _steerReplayArmed: boolean;

  // Initialization state — true once the ACP session is ready
  isInitialized: boolean;

  // Non-interactive mode
  noInteractive: boolean;

  // UI mode (lite or tui)
  uiMode: UiMode;

  /** Lite-specific state; see {@link LiteState}. */
  lite: LiteState;
  setLiteWelcomeEmitted: (value: boolean) => void;
  setLiteMcpFailureWarningEmitted: (value: boolean) => void;
  bumpLiteScrollbackClear: () => void;

  /**
   * Lite mode subagent inspection panel state. When non-null, the lite
   * layout has the Ctrl+O panel open over the activity strip; AppContainer's
   * top-level keypress dispatch reads this so Esc/Ctrl+O don't double-fire
   * as a stream cancel. The actual focused index lives inside LiteLayout —
   * here we only track open/closed.
   */
  subagentPanelOpen: boolean;
  setSubagentPanelOpen: (open: boolean) => void;

  /**
   * /prompts detail-view flag. PromptsMenu owns the picker↔detail toggle
   * locally; this mirror lets LiteLayout's always-armed Esc handler skip
   * its setActiveCommand(null) branch while the detail view has its own
   * Esc-back semantics. Same shape as `subagentPanelOpen`.
   */
  promptDetailOpen: boolean;
  setPromptDetailOpen: (open: boolean) => void;

  // Trust all tools mode
  trustAllToolsRequested: boolean;
  trustAllToolsConfirmed: boolean;

  /**
   * "Try Lite" nudge. Set to true at startup when the user is in the lite
   * rollout cohort on an interactive TTY. Gates the "Try Lite" startup tip
   * (see tips/tips.ts); nothing clears it since the tip is picked once per
   * launch.
   */
  recommendLiteUi: boolean;

  // Research-survey state
  /** Lazily-loaded snapshot of persisted survey state (eligibility, cooldown). */
  surveyState: SurveyState;
  /** How many assistant turns have completed in THIS session. */
  completedTurnCount: number;
  /** Whether the research-survey panel is currently open. */
  showSurveyPanel: boolean;
  /** Which survey is currently active (shown in the panel). */
  activeSurvey: SurveyDefinition | null;
  /** Whether the plan-quality survey was shown this session (gates implement-plan). */
  planSurveyShownThisSession: boolean;
  /** Active survey prompt bar (null = hidden). Separate from transientAlert. */
  surveyPrompt: { message: string; survey: SurveyDefinition } | null;

  // Streaming buffer control (typed properly instead of `any`)
  streamingBuffer: {
    startBuffering: (() => void) | null;
    stopBuffering: (() => void) | null;
  };
}

export const useAppStore = <T>(
  selector: (state: AppState & AppActions) => T
) => {
  const store = useContext(AppStoreContext);
  if (!store) throw new Error('Missing StoreContext.Provider in the tree');
  return useStore(store, selector);
};

// Lazily-created throwaway store used only when a component that reads the app
// store is rendered with no provider (Storybook, isolated snapshot tests).
// `kiro` is merely stored at creation — its methods only run on user actions
// that never fire in those contexts — so a null kiro is safe here.
let _fallbackAppStore: AppStoreApi | null = null;
const getFallbackAppStore = (): AppStoreApi => {
  if (!_fallbackAppStore) {
    _fallbackAppStore = createAppStore({ kiro: null as never });
  }
  return _fallbackAppStore;
};

/**
 * Like {@link useAppStore} but returns `fallback` when no AppStoreContext
 * provider is mounted instead of throwing. For shared components (e.g.
 * Message) that read a single store flag yet are also rendered in isolation
 * — Storybook, snapshot tests — without the full app provider. Mirrors
 * useStatusBar's no-provider fallback.
 */
export const useAppStoreOptional = <T>(
  selector: (state: AppState & AppActions) => T,
  fallback: T
): T => {
  const store = useContext(AppStoreContext);
  // Hooks must run unconditionally: subscribe to the real store when present,
  // else to the shared fallback so the selector still runs against valid state.
  const value = useStore(store ?? getFallbackAppStore(), selector);
  return store ? value : fallback;
};

const CONTEXT_WARNING_THRESHOLD = 60;

const LOCAL_USER_ECHO_TTL_MS = 3000;

/**
 * Sync the OSC 9;4 terminal progress indicator to the current app state.
 *
 * When active (processing/compacting):
 *   - Spinning green                       — normal processing
 *   - Static yellow at 100%                — waiting for approval
 *   - Pulsing red                          — error during processing
 *
 * When idle:
 *   - Static yellow bar with context %     — context ≥ warning threshold
 *   - Pulsing red                          — error
 *   - Hidden                               — everything normal
 */
function syncTerminalProgress(
  state: Pick<
    AppState,
    | 'agentError'
    | 'pendingApproval'
    | 'pendingQuestion'
    | 'isProcessing'
    | 'isCompacting'
    | 'contextUsagePercent'
  >
): void {
  let cmuxStatus: CmuxAgentStatus;

  if (state.isProcessing || state.isCompacting) {
    // Active — always spinning unless paused for approval
    if (state.agentError) {
      setTerminalProgressError(); // pulsing red
      cmuxStatus = 'error';
    } else if (state.pendingApproval || state.pendingQuestion) {
      setTerminalProgressWarning(100); // static yellow at 100%
      cmuxStatus = 'waiting-approval';
    } else if (state.isCompacting) {
      setTerminalProgressIndeterminate(); // spinning green
      cmuxStatus = 'compacting';
    } else {
      setTerminalProgressIndeterminate(); // spinning green
      cmuxStatus = 'thinking';
    }
  } else {
    // Idle — static bar or hidden
    if (state.agentError) {
      setTerminalProgressError(); // pulsing red
      cmuxStatus = 'error';
    } else if (
      state.contextUsagePercent != null &&
      state.contextUsagePercent >= CONTEXT_WARNING_THRESHOLD
    ) {
      setTerminalProgressWarning(state.contextUsagePercent); // static yellow with %
      cmuxStatus = 'idle';
    } else {
      clearTerminalProgress(); // hidden
      cmuxStatus = 'idle';
    }
  }

  syncCmuxStatus(cmuxStatus);
}

/** Extract task state from a ToolCallFinished event if it came from the task tool. */
function extractTaskState(
  event: { id: string; result: { status: string; output?: unknown } },
  get: () => AppState & AppActions
) {
  const finishedMsg = get().messages.find(
    (m) => m.role === MessageRole.ToolUse && m.id === event.id
  );
  if (
    finishedMsg?.role !== MessageRole.ToolUse ||
    !TASK_TOOL_NAMES.has(finishedMsg.name) ||
    event.result.status !== 'success' ||
    !event.result.output
  )
    return;

  try {
    const args = JSON.parse(finishedMsg.content);
    if (
      !args.command ||
      !['create', 'complete', 'add', 'remove', 'list'].includes(args.command)
    )
      return;

    let raw =
      typeof event.result.output === 'string'
        ? JSON.parse(event.result.output)
        : event.result.output;
    if (raw?.items?.[0]?.Json) {
      raw = raw.items[0].Json;
    }
    if (raw && Array.isArray(raw.tasks)) {
      const mapped = raw.tasks.map((t: RawTask) => ({
        id: t.id,
        subject: t.task_description ?? t.subject ?? '',
        status: t.completed ? ('completed' as const) : ('pending' as const),
      }));
      get().setTasks(mapped);
    }
  } catch {
    // Not a task tool or malformed output — ignore
  }
}

/**
 * Lite has no NotificationBar mounted, so route a command's failure alerts to
 * scrollback instead of a transient toast. Successes are dropped (commands that
 * want to surface a success should call ctx.announceSystem). activeCommand is
 * cleared only on warn/error: the dispatcher's success path fires showAlert with
 * the result message even when nothing visible happened, and clearing
 * unconditionally would clobber a menu a queued slash command just opened
 * mid-drain (e.g. /agent swap → queued /verbosity opens its picker → success
 * alert clears it). A failed command's menu shouldn't linger over its error.
 */
function applyLiteAlertRouting(
  ctx: CommandContext,
  state: AppState & AppActions,
  set: StoreApi<AppState & AppActions>['setState']
): void {
  if (state.uiMode !== 'lite') return;
  ctx.showAlert = (message, status) => {
    if (status === 'error' || status === 'warning') {
      ctx.addSystemMessage(message, false);
      set({ activeCommand: null });
    }
  };
}

function insertCompactionReport(
  messages: MessageType[],
  summary: string,
  index = messages.length
): MessageType[] {
  const report: MessageType = {
    id: crypto.randomUUID(),
    role: MessageRole.Model,
    content: summary,
    standalone: true,
  };
  const boundedIndex = Math.max(0, Math.min(index, messages.length));
  return [
    ...messages.slice(0, boundedIndex),
    report,
    ...messages.slice(boundedIndex),
  ];
}

const LEGACY_COMPACTION_ATTEMPT_KEY = 0;

function getNextCompactionAttemptKey(
  state: AppState,
  attemptId?: number
): number {
  if (attemptId != null) return attemptId;
  if (state.isCompacting && state.activeCompactionAttemptKey != null) {
    return state.activeCompactionAttemptKey;
  }
  return LEGACY_COMPACTION_ATTEMPT_KEY;
}

function isActiveCompactionTerminalEvent(
  state: AppState,
  attemptId?: number
): boolean {
  if (!state.isCompacting) return false;
  if (attemptId != null) {
    return (
      state.activeCompactionAttemptKey == null ||
      state.activeCompactionAttemptKey === attemptId
    );
  }
  return (
    state.activeCompactionAttemptKey == null ||
    state.activeCompactionAttemptKey === LEGACY_COMPACTION_ATTEMPT_KEY
  );
}

/**
 * Command set the lite "dispatch locally vs. send to agent" gate
 * (`isKnownSlashCommandToken`) should match against.
 *
 * In KAS mode the backend advertises commands (`/rewind`, `/usage`, `/spec`,
 * …) via `kasCommands` that are NOT mirrored into `slashCommands`; gating on
 * `slashCommands` alone made those tokens fall through the gate and get sent
 * to the model as chat text instead of dispatching (panel never opens, a
 * billed turn wasted). `selectVisibleSlashCommands` prepends `kasCommands`
 * (and folds prompt/skill/steering projections), so it's the correct gate set
 * in KAS mode.
 *
 * v2 is intentionally left on the raw `slashCommands` slice so this is a
 * provable no-op for the v2 backend: `selectVisibleSlashCommands` would also
 * fold in prompt/skill projections the v2 gate never recognized before, and
 * we are not changing v2 behavior here.
 */
function liteGateCommands(state: AppState): readonly AvailableCommand[] {
  return state.agentEngine === 'kas'
    ? selectVisibleSlashCommands(state)
    : state.slashCommands;
}

/** Build a CommandContext from the current AppState + setter. */
export function buildCommandContext(
  state: AppState & AppActions,
  set: StoreApi<AppState & AppActions>['setState'],
  get: StoreApi<AppState & AppActions>['getState'],
  extraClearState?: Partial<AppState>
): CommandContext {
  const addSystemMessage = (content: string, success: boolean) =>
    set((s) => ({
      messages: [
        ...s.messages,
        {
          id: generateMessageId(),
          role: MessageRole.System,
          content,
          success,
        },
      ],
    }));
  // Dispatch + lookup work against the merged visible list so
  // `/research` (a prompt projection) is reachable even though prompts
  // live in their own slice.
  const visibleSlashCommands = selectVisibleSlashCommands(state);
  return {
    kiro: state.kiro,
    agentEngine: state.agentEngine,
    cloudSessionActive: state.cloudSessionActive,
    slashCommands: visibleSlashCommands,
    kasCommands: state.kasCommands,
    prompts: state.prompts,
    skills: state.skills,
    steering: state.steering,
    kasAvailableModels: state.kas.availableModels,
    kasAvailableAgents: state.kas.availableAgents,
    kasAvailableEfforts: state.kas.availableEfforts,
    showAlert: (message, status, autoHideMs = 3000) =>
      state.showTransientAlert({ message, status, autoHideMs }),
    announceSystem: (
      message: string,
      success: boolean = true,
      autoHideMs: number = 3000
    ) => {
      if (state.uiMode === 'lite') {
        addSystemMessage(message, success);
      } else {
        state.showTransientAlert({
          message,
          status: success ? 'success' : 'error',
          autoHideMs,
        });
      }
    },
    setLoadingMessage: state.setLoadingMessage,
    setActiveCommand: state.setActiveCommand,
    setCurrentModel: state.setCurrentModel,
    beginKasSession: state.beginKasSession,
    getCurrentModel: () => get().currentModel,
    setCurrentEffort: state.setCurrentEffort,
    getCurrentEffort: () => get().currentEffort,
    setCurrentAgent: state.setCurrentAgent,
    getCurrentAgent: () => get().currentAgent,
    currentAgent: state.currentAgent,
    setContextUsage: state.setContextUsage,
    setShowContextBreakdown: state.setShowContextBreakdown,
    getContextBreakdownCache: () => get().contextBreakdownCache,
    setShowHelpPanel: state.setShowHelpPanel,
    setShowTuiPanel: state.setShowTuiPanel,
    setShowChangelogPanel: state.setShowChangelogPanel,
    setShowUsagePanel: state.setShowUsagePanel,
    setShowRewindExplorer: state.setShowRewindExplorer,
    setShowTangentExplorer: state.setShowTangentExplorer,
    setTangentName: state.setTangentName,
    setUpgradeDiagnostics: state.setUpgradeDiagnostics,
    setUpgradeRunPreview: state.setUpgradeRunPreview,
    setShowMcpPanel: state.setShowMcpPanel,
    mcpServerCache: state.mcpServerCache,
    mcpRegistryCache: state.mcpRegistryCache,
    setShowToolsPanel: state.setShowToolsPanel,
    toolsList: state.toolsList,
    setShowGoalPanel: state.setShowGoalPanel,
    setGoalStatus: state.setGoalStatus,
    setShowStatsPanel: state.setShowStatsPanel,
    setShowHooksPanel: state.setShowHooksPanel,
    hooksList: state.hooksList,
    setShowRepoPicker: state.setShowRepoPicker,
    setShowSessionPicker: state.setShowSessionPicker,
    resetCloudSessionScope: state.resetCloudSessionScope,
    stashCloudSessionScope: state.stashCloudSessionScope,
    restoreCloudSessionScope: state.restoreCloudSessionScope,
    applyRepoFooter: state.applyRepoFooter,
    setCloudNewSessionChecklist: state.setCloudNewSessionChecklist,
    setCloudSessionActive: state.setCloudSessionActive,
    setShowKeybindingsPanel: state.setShowKeybindingsPanel,
    setShowDisplaySettingsPanel: state.setShowDisplaySettingsPanel,
    setShowThemePanel: state.setShowThemePanel,
    setShowCloudQuitPrompt: state.setShowCloudQuitPrompt,
    setShowSettingsPanel: state.setShowSettingsPanel,
    setSettingsReturnOnEscape: state.setSettingsReturnOnEscape,
    setVerboseReturnOnEscape: state.setVerboseReturnOnEscape,
    setThemeReturnOnEscape: state.setThemeReturnOnEscape,
    setActiveInterruptMode: state.setActiveInterruptMode,
    settingsReturnOnEscape: state.settingsReturnOnEscape,
    reopenSettingsMenu: state.reopenSettingsMenu,
    setShowKnowledgePanel: state.setShowKnowledgePanel,
    setShowCodePanel: state.setShowCodePanel,
    openArtifactView: state.openArtifactView,
    clearMessages: state.clearMessages,
    resetMessages: state.resetMessages,
    bumpLiteScrollbackClear: state.bumpLiteScrollbackClear,
    sendMessage: state.sendMessage,
    createStreamEventHandler: state.createStreamEventHandler,
    setSessionId: (id: string | null) => {
      if (
        id &&
        readStringSetting(Settings.CHAT_HISTORY_MODE, 'session') === 'session'
      ) {
        CommandHistory.getInstance().setSessionId(id);
      }
      set({ sessionId: id, initErrors: [] });
    },
    addSystemMessage,
    setPendingSpecDescription: state.setPendingSpecDescription,
    addSession: state.addSession,
    setActiveSession: state.setActiveSession,
    sessions: state.sessions,
    setMode: state.setMode,
    clearUIState: () =>
      set({
        activeCommand: null,
        showContextBreakdown: false,
        showHelpPanel: false,
        showUsagePanel: false,
        showRewindExplorer: false,
        showTangentExplorer: false,
        showMcpPanel: false,
        showToolsPanel: false,
        showStatsPanel: false,
        showHooksPanel: false,
        showRepoPicker: false,
        showSourceProviderGate: false,
        showKeybindingsPanel: false,
        showThemePanel: false,
        showCloudQuitPrompt: false,
        settingsReturnOnEscape: false,
        verboseReturnOnEscape: null,
        themeReturnOnEscape: null,
        showKnowledgePanel: false,
        contextBreakdown: null,
        usageData: null,
        // Any session change clears the tangent chip; a real tangent switch
        // re-sets it immediately after via switchToKasSession/resolveTangentName.
        tangentName: null,
        pendingSpecDescription: null,
        ...extraClearState,
      }),
    getMessages: () => get().messages,
    setUserColors: (prompt?: any, response?: any, diff?: any) => {
      const setter = get()._userColorsSetter;
      if (setter) setter(prompt, response, diff);
    },
    setBaseTheme: (theme: any) => {
      const setter = get()._baseThemeSetter;
      if (setter) setter(theme);
    },
    setThemePreview: (preview: string | null) => {
      set({ themePreview: preview });
    },
    getThemeDiffHex: () => {
      const getter = get()._themeDiffHexGetter;
      if (getter) return getter();
      const d = kiroSafe.colors.diff;
      return {
        added: {
          background: d.added.background,
          bar: d.added.bar,
          highlight: d.added.highlight,
        },
        removed: {
          background: d.removed.background,
          bar: d.removed.bar,
          highlight: d.removed.highlight,
        },
      };
    },
    getAutoPreview: () => {
      const getter = get()._autoPreviewGetter;
      return getter ? getter() : '';
    },
    // Delegate to the canonical store setter so mode swaps go through
    // the coordinated path (tui→lite bookmarks skipBefore; lite→tui
    // bumps the clear token). A direct set({ uiMode }) here would
    // break the asymmetric scrollback contract.
    setUiMode: (uiMode: UiMode, notice?: string) =>
      get().setUiMode(uiMode, notice),
    getUiMode: () => get().uiMode,
    setLiteStaticSkipBefore: (idx: number) =>
      get().setLiteStaticSkipBefore(idx),
    processQueue: () => get().processQueue(),
    setVoiceStop: state.setVoiceStop,
    setVoiceCancel: state.setVoiceCancel,
    setVoiceLevel: state.setVoiceLevel,
    voiceAutoSubmit: state.voiceAutoSubmit,
    toggleVoiceAutoSubmit: state.toggleVoiceAutoSubmit,
    voiceHintIndex: state.voiceHintIndex,
    incrementVoiceHint: state.incrementVoiceHint,
    setPendingVoiceText: state.setPendingVoiceText,
    setVoicePartialText: state.setVoicePartialText,
  };
}

export const createAppStore = (props: AppStoreProps) => {
  const agentEngine: AgentEngine = props.agentEngine ?? resolveAgentEngine();
  const store = createStore<AppState & AppActions>((set, get) => ({
    // Initial state
    messages: [],
    liveOutputs: new Map(),
    queuedMessages: [],
    editingQueueIndex: null,
    editingSteerLineIndex: null,
    queuedInputRestore: null,
    pendingSteerContent: null,
    slashCommands: [
      {
        name: '/editor',
        description: 'Open $EDITOR to compose a prompt',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/spawn',
        description: 'Spawn a new agent session with a task',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        // Always registered so cold-boot users discovering /switch via
        // autocomplete get the correct "No active sessions" alert instead of
        // having "/switch" sent to the agent as a chat message. The
        // switchSession effect at effects.ts handles the empty-sessions case.
        name: '/switch',
        description: 'Switch to a spawned agent session',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/copy',
        description:
          'Copy last response to clipboard (use /transcript for full conversation)',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/transcript',
        description: 'Open conversation transcript in $PAGER (quit with q)',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/quit',
        description: 'Quit the application',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/exit',
        description: 'Quit the application',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/settings',
        description:
          'Configure theme, terminal, keybindings, and other preferences',
        source: 'local' as const,
        // inputType is set dynamically in showSettingsMenu rather than here:
        // a static 'selection' would make the dispatcher fetch options from
        // the backend for what is a local command.
        meta: { local: true },
      },
      {
        name: '/theme',
        description:
          '(moved to /settings theme) Select a theme that looks best for your terminal',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        // /lite and /tui are the symmetric session swaps. The handlers route
        // through setUiMode, which clears scrollback and re-renders the
        // conversation in the destination mode's form. From TUI mode, /tui
        // falls through to the info panel (origin/main behavior).
        name: '/lite',
        description: '[EXPERIMENTAL] Switch to Lite UI',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/tui',
        description: 'Switch to TUI mode',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        // /verbosity is the most-touched config menu (filters, density,
        // truncation), so it earns a top-level shortcut despite most other
        // /settings entries lacking one. /settings verbosity is also wired
        // (settings-subcommands.ts) for users who discover the menu via
        // /settings; both paths land in the same handler. Always a peer in
        // lite; on the TUI it only surfaces inside the Lite rollout cohort
        // (liteOnly hides it otherwise — the port doesn't exist off-cohort).
        name: '/verbosity',
        description:
          'Configure rendering: tool args, reasoning, output filters, density, subagent sections.',
        source: 'local' as const,
        meta: {
          local: true,
          liteOnly: process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1',
        },
      },
      {
        name: '/changelog',
        description: 'Show recent release notes',
        source: 'local' as const,
        meta: { local: true, inputType: 'panel' as const },
      },
      {
        name: '/session-id',
        description: 'Print the current session ID',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/title',
        description: 'Set, clear, or show the terminal window title',
        source: 'local' as const,
        meta: { local: true },
      },
    ]
      // /lite is a dead entry off the rollout (switchToLite no-ops), so drop
      // it from the menu there. /tui stays — KAS lite↔TUI switching dispatches
      // it through this same local list (liteGateCommands).
      .filter(
        (cmd) =>
          cmd.name !== '/lite' || process.env.KIRO_LITE_ROLLOUT_ENABLED === '1'
      ),
    kasCommands: agentEngine === 'kas' ? [...getKasCommands()] : [],
    cloudSessionActive: false,
    agentEngine,
    prompts: [],
    skills: [],
    steering: [],
    kas: {
      availableModels: [],
      availableAgents: [],
      availableEfforts: [],
      sessionOrigin: null,
      previousModelId: null,
      effortExplicit: false,
      subagentRouting: createInitialKasSubagentRoutingState(),
    },
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    isCompacting: false,
    activeCompactionAttemptKey: null,
    compactionReportAnchor: null,
    wasCancelled: false,
    lastTurnErrored: false,
    agentError: null,
    agentErrorGuidance: null,
    pendingApproval: null,
    approvalQueue: [],
    pendingQuestion: null,
    questionQueue: [],
    pendingSpecDescription: null,
    approvalMode: 'dropdown',
    autoApproveCrewTools: false,
    focusedCrewIndex: 0,
    currentModel: null,
    currentEffort: null,
    currentAgent: null,
    previousAgentName: null,
    settings: null,
    goalStatus: null,

    activeCommand: null,
    commandInputValue: '',
    activeTrigger: null,
    filePickerHasResults: false,
    promptHint: null,
    commandShadowText: null,
    voiceStop: null,
    voiceCancel: null,
    voiceLevel: null,
    voiceAutoSubmit: false,
    voiceHintIndex: 0,
    voicePartialText: null,
    pendingVoiceText: null,

    input: initialInputBufferState(),
    reverseSearchActive: false,
    setReverseSearchActive: (active: boolean) => {
      set({ reverseSearchActive: active });
    },

    mode: 'inline',
    sessions: new Map(),
    activeSessionId: '',
    selectedSessionId: undefined,
    crewMonitorVisible: false,
    sessionEventBuffer: {},

    exitSequence: 0,
    exitTimer: null,
    suspendArmed: false,
    suspendTimer: null,
    transientAlert: null,
    retryStatus: null,
    thinkingContent: '',
    streamingContent: '',
    streamingMessageId: null as string | null,
    loadingMessage: null as string | null,
    toolOutputsExpanded: false,
    hasExpandableToolOutputs: false,

    contextUsagePercent: null,
    cloudSessionStatus: null,
    cloudProvisioningFailure: null,
    sessionRoster: new Map<string, RosterEntry>(),
    cloudRepo: null,
    cloudBranch: null,
    cloudProvider: null,
    cloudRepoCount: null,
    cloudNewSessionChecklist: false,
    hasEnteredConversation: false,
    cloudExtraRepos: 0,
    cloudScopeBySession: new Map(),
    lastTurnTokens: null,
    turnSummaries: new Map(),
    showContextBreakdown: false,
    contextBreakdown: null,
    contextBreakdownCache: null,
    showTuiPanel: false,
    showChangelogPanel: false,
    showHelpPanel: false,
    helpCommands: [],
    showUsagePanel: false,
    usageData: null,
    showRewindExplorer: false,
    rewindRows: [],
    showTangentExplorer: false,
    tangentRows: [],
    tangentName: null,
    upgradeAnalysisRows: [],
    upgradeAnalysisDescription: '',
    upgradeRunPreview: {},
    showMcpPanel: false,
    mcpServers: [],
    mcpRegistryServers: [],
    mcpServerCache: [],
    mcpRegistryCache: [],
    pendingOAuthServers: new Map(),
    initErrors: [],
    mcpInitStatus: new Map(),
    bootProgress: new Map(),
    setBootStage: (key, label, status, error) => {
      set((state) => {
        const next = new Map(state.bootProgress);
        const prev = next.get(key);
        if (status === 'loading') {
          if (!prev) next.set(key, { label, status, startTime: Date.now() });
        } else {
          const startTime = prev?.startTime ?? Date.now();
          next.set(key, {
            label,
            status,
            startTime,
            elapsed: Date.now() - startTime,
            error,
          });
        }
        return { bootProgress: next };
      });
    },
    mcpMode: 'list',
    showToolsPanel: false,
    showGoalPanel: false,
    toolsList: [],
    showStatsPanel: false,
    statsList: [],
    statsSummary: null,
    showHooksPanel: false,
    hooksList: [],
    showRepoPicker: false,
    repoPickerResources: [],
    attachedRepos: [],
    cloudProviderChecked: false,
    showSourceProviderGate: false,
    sourceProviderSetupUrl: null,
    showSessionPicker: false,
    sessionPickerRows: [],
    sessionPickerTitle: '/sessions',
    showKeybindingsPanel: false,
    showDisplaySettingsPanel: false,
    showThemePanel: false,
    showCloudQuitPrompt: false,
    showSettingsPanel: false,
    themePreview: null,
    terminalTitleEnabled: readBoolSetting(Settings.CHAT_TERMINAL_TITLE, false),
    settingsReturnOnEscape: false,
    verboseReturnOnEscape: null,
    themeReturnOnEscape: null,
    showKnowledgePanel: false,
    knowledgeEntries: [],
    knowledgeStatus: null,
    showCodePanel: false,
    codeData: null,
    codeIntelligenceActive: existsSync(
      join(process.cwd(), '.kiro', 'settings', 'lsp.json')
    ),
    artifactGenerating: null,
    artifactViewOpen: null,
    attachedFiles: [],
    _userColorsSetter: null,
    _baseThemeSetter: null,
    _themeDiffHexGetter: null,
    _autoPreviewGetter: null,
    pendingFileAttachment: null,
    pendingImages: [],
    currentAbortController: null,
    cancelInProgress: null,
    isShellEscape: false,
    _shellEscapeWriter: null,
    _activeStreamHandler: null,
    _liveStreamHandler: null,
    setLiveStreamHandler: (handler: StreamEventHandler) => {
      // Retire pending timers before replacing a session renderer.
      get()._liveStreamHandler?.dispose();
      set({ _liveStreamHandler: handler });
    },
    _recentLocalUserMessages: [],
    _observerQueueBlocked: false,
    _steerReplayArmed: false,
    streamingBuffer: { startBuffering: null, stopBuffering: null },

    // Dual-mode interrupt behavior
    activeInterruptMode: parseInterruptMode(
      readStringSetting(
        Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR,
        DEFAULT_INTERRUPT_MODE
      )
    ),

    // Task management
    tasks: [],
    activityTrayExpanded: false,

    // Announcement
    announcement: null,
    announcementExpanded: false,

    isInitialized: false,
    noInteractive: props.noInteractive ?? false,
    uiMode: props.uiMode ?? 'tui',
    lite: {
      staticSkipBefore: 0,
      welcomeEmitted: false,
      mcpFailureWarningEmitted: false,
      scrollbackClearToken: 0,
    },
    setLiteWelcomeEmitted: (value: boolean) =>
      set((s) => ({ lite: { ...s.lite, welcomeEmitted: value } })),
    setLiteMcpFailureWarningEmitted: (value: boolean) =>
      set((s) => ({
        lite: { ...s.lite, mcpFailureWarningEmitted: value },
      })),
    bumpLiteScrollbackClear: () =>
      set((s) => ({
        lite: {
          ...s.lite,
          scrollbackClearToken: s.lite.scrollbackClearToken + 1,
        },
      })),
    subagentPanelOpen: false,
    setSubagentPanelOpen: (open: boolean) => set({ subagentPanelOpen: open }),
    promptDetailOpen: false,
    setPromptDetailOpen: (open: boolean) => set({ promptDetailOpen: open }),
    trustAllToolsRequested: props.trustAllTools ?? false,
    trustAllToolsConfirmed: false,

    // Resolved at startup in index.tsx; defaults to false so out-of-cohort
    // users never see the nudge.
    recommendLiteUi: false,

    // Research survey — eligibility is resolved lazily on first boot.
    surveyState: (() => {
      const loaded = loadSurveyState(SESSION_FEEDBACK_SURVEY.id);
      return resolveEligibility(SESSION_FEEDBACK_SURVEY, loaded).state;
    })(),
    completedTurnCount: 0,
    showSurveyPanel: false,
    activeSurvey: null,
    planSurveyShownThisSession: false,
    surveyPrompt: null,

    sendMessage: async (
      content: string,
      images?: Array<{ base64: string; mimeType: string }>,
      displayContent?: string
    ) => {
      const {
        kiro,
        isProcessing,
        isCompacting,
        loadingMessage,
        isInitialized,
        attachedFiles,
        pendingImages,
      } = get();
      if (!isInitialized || isProcessing || isCompacting || loadingMessage) {
        get().queueMessage(displayContent ?? content);
        return;
      }

      // Merge explicitly passed images with store pending images
      const allImages = [
        ...pendingImages.map(({ base64, mimeType }) => ({ base64, mimeType })),
        ...(images ?? []),
      ];

      logger.debug('[store] sendMessage', { contentLength: content.length });

      // Add to history
      CommandHistory.getInstance().add(displayContent ?? content);

      // Expand @file: references and attached files
      let expandedContent = expandFileReferences(content);
      for (const filePath of attachedFiles) {
        const fileContent = readFileContent(filePath);
        if (fileContent) {
          expandedContent += `\n<attached_file path="${filePath}">\n${fileContent}\n</attached_file>`;
        }
      }

      const shouldCollectCloudAttachments =
        kiro.isCloudSessionActive?.() ?? false;
      // Scan only user-authored text; expanded @file bodies may contain unrelated paths.
      const cloudAttachmentText =
        displayContent && displayContent !== content
          ? `${content}\n${displayContent}`
          : content;

      const abortController = new AbortController();
      set({ currentAbortController: abortController });

      const userMessageId = generateMessageId();

      // Build display content: use image labels with dimensions when no text
      const shownContent =
        displayContent ||
        content ||
        (pendingImages.length > 0
          ? pendingImages.map(formatImageLabel).join(' ')
          : allImages.length > 0
            ? '[pasted image]'
            : '');
      const userMessage: MessageType = {
        id: userMessageId,
        role: MessageRole.User,
        content: shownContent,
        agentName: get().currentAgent?.name,
      };

      set((state) => {
        const now = Date.now();
        const recentLocalUserMessages = [
          ...state._recentLocalUserMessages.filter(
            (m) => now - m.at < LOCAL_USER_ECHO_TTL_MS
          ),
          { content: shownContent, sentContent: expandedContent, at: now },
        ];
        return {
          isProcessing: true,
          agentError: null,
          agentErrorGuidance: null,
          _observerQueueBlocked: false,
          wasCancelled: false,
          lastTurnErrored: false,
          autoApproveCrewTools: false,
          // The first prompt dismisses the post-create checklist.
          cloudNewSessionChecklist: false,
          messages: [...state.messages, userMessage],
          _recentLocalUserMessages: recentLocalUserMessages,
          attachedFiles: [], // Clear attachments after sending
          pendingImages: [], // Clear pending images after sending
          // Reset expandable content flag for new turn (expanded state persists)
          hasExpandableToolOutputs: false,
          // A fresh request starts — any retry banner from a previous request is stale.
          retryStatus: null,
        };
      });

      // The handler is declared outside the try block so the catch can
      // dispose it (even if stream setup failed before kiro.streamMessage
      // returned). Dispose prevents the handler's pending batched-flush
      // timers from committing stale content to the next turn — critical
      // for cancel + replay correctness (see app-store.test.ts).
      let eventHandler: StreamEventHandler | null = null;
      try {
        const cloudAttachments = shouldCollectCloudAttachments
          ? await collectCloudAttachments(
              cloudAttachmentText,
              abortController.signal
            )
          : { images: [], resources: [], blobs: [] };
        const allImagesWithCloud = [
          ...allImages,
          ...cloudAttachments.images.map(({ base64, mimeType }) => ({
            base64,
            mimeType,
          })),
        ];

        eventHandler = get().createStreamEventHandler();
        // Track the active handler so cancelMessage can dispose it FIRST
        // (commit partial content + cancel pending flush timers) before the
        // abort + backend cancel round-trips run. See cancelMessage.
        set({ _activeStreamHandler: eventHandler });
        await kiro.streamMessage(
          expandedContent,
          abortController.signal,
          eventHandler,
          allImagesWithCloud.length > 0 ? allImagesWithCloud : undefined,
          cloudAttachments.resources.length > 0
            ? cloudAttachments.resources
            : undefined,
          cloudAttachments.blobs.length > 0 ? cloudAttachments.blobs : undefined
        );
        eventHandler.flush();
        set({ _activeStreamHandler: null });

        // Mark any remaining tool calls as finished and mark turn as complete
        // Clear agentError on successful completion (Requirement 4.3)
        set((state) => {
          // Check if any tool calls need to be marked finished
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );

          if (hasUnfinishedToolCalls) {
            const messages = state.messages.map((msg) => {
              if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
                return { ...msg, isFinished: true };
              }
              return msg;
            });
            return {
              messages,
              isProcessing: false,
              currentAbortController: null,
              agentError: null,
              agentErrorGuidance: null,
            };
          }

          return {
            isProcessing: false,
            currentAbortController: null,
            agentError: null,
            agentErrorGuidance: null,
          };
        });

        // After isProcessing is cleared, drain the next queued message (if any).
        // Must be after the set() above so the double-send guard in processQueue
        // sees isProcessing === false.
        await get().processQueue();
      } catch (error) {
        // Drop buffered content from the cancelled/failed stream; see
        // StreamEventHandler for why this matters.
        eventHandler?.dispose();
        set({ currentAbortController: null, _activeStreamHandler: null });
        logger.error('[store] sendMessage: caught error', error);
        if (error instanceof DOMException && error.name === 'AbortError') {
          // cancelMessage owns the cleanup on user-triggered abort: it has
          // already disposed the active handler, cleared isProcessing, and
          // queued processQueue from its finally block. If we drain here
          // too, two processQueue calls race over queuedMessages.slice(1)
          // and may stomp on each other's sendMessage state — historically
          // this manifested as queued messages getting stuck or sent in the
          // wrong order after an interrupt. The non-cancel abort path
          // (e.g. shutdown) keeps the original drain so it doesn't regress.
          if (get().cancelInProgress) {
            return;
          }
          set({ isProcessing: false });
          return;
        }
        // Extract error message. Most agent errors arrive already-extracted
        // from kiro.ts::streamMessage (which uses extractRpcErrorMessage), but
        // call it again here as a defensive fallback for any error that
        // reaches this catch from another source (e.g. event-handler throws,
        // non-kiro errors).
        const errorMessage = extractRpcErrorMessage(error, 'Unknown error');

        // Mark any remaining tool calls as finished on error
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );
          if (hasUnfinishedToolCalls) {
            return {
              messages: state.messages.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? { ...msg, isFinished: true }
                  : msg
              ),
            };
          }
          return state;
        });

        // Determine error category and handle accordingly
        const category = detectErrorCategory(errorMessage);
        let displayMessage = simplifyErrorMessage(errorMessage);

        // If retries happened, enrich the message so the user knows we tried
        const retryInfo = get().retryStatus;
        if (retryInfo && category === 'network') {
          displayMessage = `${displayMessage} (failed after ${retryInfo.maxAttempts} attempts)`;
        }

        // Only auth and session errors are blocking (require user action)
        if (category === 'auth' || category === 'session') {
          set({
            agentError: displayMessage,
            agentErrorGuidance: getErrorGuidance(errorMessage).message,
            isProcessing: false,
            lastTurnErrored: true,
          });
        } else {
          // All other errors are non-blocking.
          get().showTransientAlert({
            message: displayMessage,
            status: 'error',
            autoHideMs: 5000,
          });
          // Keep the failure visible after the transient alert fades.
          set((s) => ({
            isProcessing: false,
            lastTurnErrored: true,
            messages: [
              ...s.messages,
              {
                id: generateMessageId(),
                role: MessageRole.System,
                content: displayMessage,
                success: false,
                turnOwned: true,
              },
            ],
          }));
          // Turn ended (non-blocking error) — drain any queued messages.
          // processQueue handles steer-first priority internally.
          await get().processQueue();
        }
      }
    },

    kasSubagentRouting: createKasSubagentRoutingActions(
      () => get().kas.subagentRouting
    ),

    /**
     * Create a stream event handler for one prompt turn. Call as a function
     * to dispatch events; call `.flush()` on happy-path completion to commit
     * any buffered content; call `.dispose()` on cancel/error to abandon
     * the handler and drop buffered content. See `StreamEventHandler`.
     */
    createStreamEventHandler: (options?: {
      fromHistory?: boolean;
      cloudReplay?: boolean;
    }) => {
      // Replayed history rows must not stamp tool timing: the real durations
      // aren't persisted, so a fresh Date.now() would show a bogus ~0ms elapsed
      // chip. Leaving both timestamps unset omits the chip entirely.
      let fromHistory = options?.fromHistory === true;
      // From the client's live placement — the store's cloudSessionActive
      // still describes the outgoing session while a switch's replay streams.
      const cloudReplay = options?.cloudReplay === true;
      let isBuffering = false;
      let bufferedContent = '';
      // Only the first model refusal in a turn is surfaced; the model can emit
      // several (e.g. across retries), but repeating the same notice is noise.
      let refusalShownThisTurn = false;
      // Set by `.dispose()` to make this handler inert. Guards the event
      // entry point and both batched-flush timers against late firings
      // after the owning turn was cancelled — see StreamEventHandler.
      let disposed = false;
      let bufferedThinking = '';
      // Reasoning timing: timestamp of the first Thought for the current model
      // message, and the duration once reasoning ends (first text Content or a
      // ToolCall). Drives the "Thought for Ns" header.
      let thinkingStart: number | null = null;
      let thinkingMs: number | null = null;

      // Batching: accumulate content chunks and flush to the store
      // on a timer so Ink's render loop isn't starved by rapid-fire
      // synchronous set() calls from the ACP notification handler.
      let pendingContentFlush: ReturnType<typeof setTimeout> | null = null;
      let lastContentEventId: string | null = null;

      // User messages inside an open observer turn are steers.
      let turnOpen = false;
      let observerTurnBlocked = false;

      // A seen persisted id is a redelivery only after its contiguous chunk run ends.
      const seenPersistedIds = new Set<string>();
      let currentPersistedId: string | null = null;
      const isPersistedRedelivery = (event: AgentStreamEvent): boolean => {
        const meta = (event as { meta?: { kiro?: { messageId?: unknown } } })
          .meta;
        const id = meta?.kiro?.messageId;
        // An id-less delta ends the contiguous persisted chunk run.
        if (typeof id !== 'string') {
          currentPersistedId = null;
          return false;
        }
        if (seenPersistedIds.has(id) && currentPersistedId !== id) return true;
        seenPersistedIds.add(id);
        currentPersistedId = id;
        return false;
      };
      // A lone replayed boundary must not pin the steer queue forever.
      let observerTurnWatchdog: ReturnType<typeof setTimeout> | null = null;
      const OBSERVER_TURN_SILENCE_MS = 30_000;
      const clearObserverTurnWatchdog = () => {
        if (observerTurnWatchdog) {
          clearTimeout(observerTurnWatchdog);
          observerTurnWatchdog = null;
        }
      };
      const armObserverTurnWatchdog = () => {
        clearObserverTurnWatchdog();
        observerTurnWatchdog = setTimeout(() => {
          observerTurnWatchdog = null;
          if (disposed || !turnOpen) return;
          logger.warn(
            '[stream] observer turn silent for 30s — clearing processing state'
          );
          turnOpen = false;
          if (get().isProcessing) set({ isProcessing: false });
          if (get().isInitialized) void get().processQueue();
        }, OBSERVER_TURN_SILENCE_MS);
      };

      // Per-tool-call live output buffering. ToolCallUpdate events for
      // verbose commands (e.g. a Gradle build streaming thousands of lines)
      // would otherwise trigger a React re-render per line. We batch per
      // tool_call_id and flush on a timer, mirroring the assistant content
      // batching above.
      const toolOutputBuffers = new Map<string, string>();
      let pendingToolOutputFlush: ReturnType<typeof setTimeout> | null = null;

      const flushToolOutputs = () => {
        pendingToolOutputFlush = null;
        if (disposed) return;
        if (toolOutputBuffers.size === 0) return;
        const buffers = Array.from(toolOutputBuffers.entries());
        toolOutputBuffers.clear();
        set((state) => {
          const newLiveOutputs = new Map(state.liveOutputs);
          for (const [id, text] of buffers) {
            if (!text) continue;
            const newLines = text.split('\n');
            if (newLines.length > 0 && newLines[newLines.length - 1] === '')
              newLines.pop();
            if (newLines.length === 0) continue;
            const prev = newLiveOutputs.get(id) ?? [];
            newLiveOutputs.set(id, [...prev, newLines]);
          }
          return { liveOutputs: newLiveOutputs };
        });
      };

      const startBuffering = () => {
        isBuffering = true;
      };

      // Stable id for the in-flight Model row this handler is writing to.
      // null means no row has been appended for this turn yet — the next
      // content flush will create one. Captured here (not just in state) so
      // the handler compares cheaply on every chunk without a selector call.
      let streamingMsgId: string | null = null;

      // Match KAS's STEERING_RESPONSE_PATTERN. Replace [STEERING steer-XXX: response]
      // with just "response". The `s` flag lets `.` match newlines so multi-line
      // acknowledgments are captured as a single response.
      const STEERING_TAG_PATTERN = /\[STEERING (steer-[^\s:]+): (.+?)\]/gs;
      // Detects a partially-streamed STEERING tag at the tail of the buffer
      // (opener seen but no closing `]` yet). Used to hold back the in-progress
      // tag from display so the user doesn't see `[STEERING steer-abc: …` flicker
      // in before it gets stripped on the next chunk.
      const PARTIAL_STEERING_TAG_PATTERN =
        /\[STEERING (steer-[^\s:]+)(?::[^\]]*)?$/s;
      const visibleAssistantContent = (
        content: string,
        holdHistoryOnlyPrefix = false
      ): string => {
        const stripped = content.replace(STEERING_TAG_PATTERN, '$2');
        const hidden = holdHistoryOnlyPrefix
          ? isHistoryOnlyAssistantMessagePrefix(stripped)
          : isHistoryOnlyAssistantMessage(stripped);
        return hidden ? '' : stripped;
      };

      /**
       * Commit the buffer into the live Model row and clear the streaming slot.
       * This is the only path that copies [...state.messages] for streaming;
       * per-chunk updates only mutate the streamingContent primitive so
       * `messages` subscribers don't invalidate at 60Hz on long responses.
       */
      const commitBufferedContent = () => {
        const content = visibleAssistantContent(bufferedContent);
        if (!content && !bufferedThinking) {
          // Nothing to write. Still clear the streaming slots so stale
          // empty strings don't outlive the boundary.
          if (streamingMsgId != null) {
            streamingMsgId = null;
            set({
              streamingContent: '',
              streamingMessageId: null,
              thinkingContent: '',
            });
          }
          return;
        }
        const idToCommit = streamingMsgId;
        set((state) => {
          // Locate the live row by id (set on the first chunk of this turn).
          // Fall back to the last Model row if we somehow lost the id —
          // matches the previous selector's behavior and keeps stopBuffering
          // (called from external code) functional.
          const idx =
            idToCommit != null
              ? state.messages.findIndex(
                  (m) => m.role === MessageRole.Model && m.id === idToCommit
                )
              : state.messages.findLastIndex(
                  (m) => m.role === MessageRole.Model
                );
          if (idx === -1) {
            return {
              streamingContent: '',
              streamingMessageId: null,
              thinkingContent: '',
            };
          }
          const msg = state.messages[idx];
          if (!msg || msg.role !== MessageRole.Model) {
            return {
              streamingContent: '',
              streamingMessageId: null,
              thinkingContent: '',
            };
          }
          const messages = [...state.messages];
          messages[idx] = {
            ...msg,
            content,
            thinking: bufferedThinking || msg.thinking,
            thinkingMs: thinkingMs ?? msg.thinkingMs,
          };
          return {
            messages,
            streamingContent: '',
            streamingMessageId: null,
            // Clear the live thinking slot at commit so the indicator falls
            // back to "inference" between rounds. Stale thinking text would
            // keep the thinking branch sticky after the model emits its tool
            // call — readers can't tell the round boundary moved.
            thinkingContent: '',
          };
        });
        streamingMsgId = null;
      };

      /**
       * Per-chunk flush. The first chunk of a turn appends a placeholder Model
       * row (one [...state.messages] copy per turn) and captures its id;
       * subsequent chunks only update the streamingContent primitive. Live
       * components subscribe to streamingContent + streamingMessageId; the
       * placeholder row is filtered from <Static> by selectStaticEligible.
       *
       * STEERING tags (`[STEERING steer-XXX: response]`) emitted by the KAS
       * backend when the model acknowledges a mid-turn steer are stripped from
       * the displayed/committed content here (see `displayContent` below) so
       * the raw tag never reaches the renderer.
       */
      const flushContentToStore = () => {
        pendingContentFlush = null;
        if (disposed) return;
        if (!bufferedContent && !bufferedThinking) return;

        // If the tail of the buffer looks like a partially-streamed
        // `[STEERING steer-…` opener with no closing `]` yet, hold it back
        // from display until the next chunk arrives and we see the full
        // tag. Without this guard the user would see a brief flicker of the
        // raw opener before `STEERING_TAG_PATTERN` strips it on the next
        // flush.
        let renderable = bufferedContent;
        const partialMatch = renderable.match(PARTIAL_STEERING_TAG_PATTERN);
        if (partialMatch && partialMatch.index !== undefined) {
          renderable = renderable.slice(0, partialMatch.index);
          // Re-schedule a flush so the held-back tail renders once the
          // closing bracket arrives (or streaming stalls long enough that
          // it's clearly not a STEERING tag after all).
          if (!pendingContentFlush) {
            pendingContentFlush = setTimeout(flushContentToStore, 16);
          }
        }

        // The display/commit content with any complete STEERING tags
        // collapsed to just the acknowledgment text. `bufferedContent`
        // (raw, tag-bearing) is intentionally NOT mutated — only the
        // rendered/persisted projection is stripped.
        const displayContent = visibleAssistantContent(renderable, true);

        // Bail only when there's nothing to persist. Thinking-only flushes
        // (think→tool-call path) have empty displayContent but carry
        // bufferedThinking + thinkingMs, which must still reach the model
        // message so the "Thought for Ns" header renders.
        if (!displayContent && !bufferedThinking) return;

        if (streamingMsgId == null) {
          // First flush of this turn — append the placeholder row with
          // buffered content inlined so external readers (transcript export,
          // integ tests) that don't consult streamingContent stay in sync.
          const newId = lastContentEventId ?? crypto.randomUUID();
          streamingMsgId = newId;
          set((state) => ({
            messages: [
              ...state.messages,
              {
                id: newId,
                role: MessageRole.Model,
                content: displayContent,
                thinking: bufferedThinking || undefined,
                thinkingMs: thinkingMs ?? undefined,
                agentName: state.currentAgent?.name,
              },
            ],
            streamingContent: displayContent,
            streamingMessageId: newId,
            // Mirror buffered thinking text into the live store slot so
            // LiteLiveRegion's "thinking" branch can render reasoning tokens
            // as they arrive. Without this, Thought events only landed on the
            // committed Model row's `thinking` field at commit time, and the
            // live indicator stayed stuck on "inference".
            thinkingContent: bufferedThinking,
          }));
          return;
        }

        // Subsequent chunk — patch the placeholder row in place for external
        // readers. ConversationView substitutes streamingContent for
        // message.content when id === streamingMessageId && isProcessing, so
        // this in-place write does not double-render in modern TUI.
        const idToPatch = streamingMsgId;
        set((state) => {
          const idx = state.messages.findIndex(
            (m) => m.role === MessageRole.Model && m.id === idToPatch
          );
          if (idx === -1) {
            return {
              streamingContent: displayContent,
              thinkingContent: bufferedThinking,
            };
          }
          const msg = state.messages[idx];
          if (!msg || msg.role !== MessageRole.Model) {
            return {
              streamingContent: displayContent,
              thinkingContent: bufferedThinking,
            };
          }
          const messages = [...state.messages];
          messages[idx] = {
            ...msg,
            content: displayContent,
            thinking: bufferedThinking || msg.thinking,
            thinkingMs: thinkingMs ?? msg.thinkingMs,
          };
          return {
            messages,
            streamingContent: displayContent,
            thinkingContent: bufferedThinking,
          };
        });
      };

      const stopBuffering = () => {
        if (isBuffering && bufferedContent) {
          commitBufferedContent();
          isBuffering = false;
        }
      };

      set({ streamingBuffer: { startBuffering, stopBuffering } });

      const baseHandler = (event: AgentStreamEvent) => {
        // Once disposed (cancelled turn), drop everything. The handler
        // may still be briefly subscribed via `onUpdate` during the
        // deferred-unsubscribe window in `kiro.ts::streamMessage`, but it
        // must not mutate the store with events that belong to an
        // abandoned turn.
        if (disposed) return;
        // The retry banner reflects the wait between the SDK's HTTP attempts. Once any
        // other stream event arrives (a new message, content chunk, error, etc.) the
        // retry window is over — clear it so the "Thinking..." line reverts. We leave
        // the banner untouched for RetryWarning itself (that's what's being rendered).
        if (event.type !== AgentEventType.RetryWarning && get().retryStatus) {
          get().setRetryStatus(null);
        }

        if (
          (event.type === AgentEventType.Content ||
            event.type === AgentEventType.Thought) &&
          isPersistedRedelivery(event)
        ) {
          return;
        }

        // Once an observer turn produces activity, silence can be legitimate.
        if (turnOpen && event.type !== AgentEventType.TurnStart) {
          clearObserverTurnWatchdog();
        }

        // Disambiguate reused tool-call ids before any case reads them. The
        // event is cloned, never mutated — raw events are also buffered
        // elsewhere (e.g. pushSessionEvent) and must keep their wire ids.
        // A client-synthesized ToolCall replays a call the store may already
        // have finished (e.g. rejected-before-exec), so it resolves to the
        // existing row instead of allocating a new generation.
        if (event.type === AgentEventType.ToolCall) {
          const internalId = event.synthesized
            ? resolveToolCallId(get().messages, event.id)
            : allocateToolCallId(get().messages, event.id);
          if (internalId !== event.id) event = { ...event, id: internalId };
        } else if (
          event.type === AgentEventType.ToolCallUpdate ||
          event.type === AgentEventType.ToolCallFinished
        ) {
          const internalId = resolveToolCallId(get().messages, event.id);
          if (internalId !== event.id) event = { ...event, id: internalId };
        } else if (event.type === AgentEventType.ApprovalRequest) {
          const wireId = event.value?.toolCall?.toolCallId;
          if (wireId) {
            const internalId = resolveToolCallId(get().messages, wireId);
            if (internalId !== wireId) {
              event = {
                ...event,
                value: {
                  ...event.value,
                  toolCall: { ...event.value.toolCall, toolCallId: internalId },
                },
              };
            }
          }
        } else if (event.type === AgentEventType.QuestionRequest) {
          // A question either attaches to its still-active tool row or opens a
          // new one, so it allocates rather than resolves: attaching it to a
          // finished row would rewrite settled scrollback as pending.
          const wireId = event.value?.toolCallId;
          if (wireId) {
            const internalId = allocateToolCallId(get().messages, wireId);
            if (internalId !== wireId) {
              event = {
                ...event,
                value: { ...event.value, toolCallId: internalId },
              };
            }
          }
        }

        switch (event.type) {
          case AgentEventType.UserMessage:
            // Historical user message from a resumed session.
            // Flush any buffered assistant content from the previous turn
            // before adding the user message so turns don't bleed together.
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
              flushContentToStore();
            }
            // Commit whatever buffered content belonged to the prior turn
            // before starting a new turn — otherwise it would leak into the
            // next Model row when the next Content event lands.
            if (streamingMsgId != null) {
              commitBufferedContent();
            }
            // Reset buffer for the next assistant turn
            bufferedContent = '';
            bufferedThinking = '';
            lastContentEventId = null;
            thinkingStart = null;
            thinkingMs = null;

            if (event.content.type === 'text') {
              const text = event.content.text;
              const id = event.id;
              // Empty persisted steer artifacts carry no displayable row.
              if (text === '') break;
              // Cloud switches APPEND each load's replay, so the re-loaded
              // copy legitimately repeats persisted user ids — dedupe only
              // live echoes, or the replay renders responses without prompts.
              const isCloudReplay = fromHistory && cloudReplay;
              const isRenderedDuplicate =
                !isCloudReplay &&
                get().messages.some(
                  (m) =>
                    m.role === MessageRole.User &&
                    (m.id === id || m.kasMessageId === id)
                );
              if (isRenderedDuplicate) break;
              // Consume text fallback matches so intentional repeats still render.
              const now = Date.now();
              const recent = get()._recentLocalUserMessages;
              const matchIdx = recent.findIndex(
                (m) =>
                  (m.content === text || m.sentContent === text) &&
                  now - m.at < LOCAL_USER_ECHO_TTL_MS
              );
              if (matchIdx !== -1) {
                set({
                  _recentLocalUserMessages: recent.filter(
                    (_, i) => i !== matchIdx
                  ),
                });
                break;
              }
              set((state) => ({
                messages: [
                  ...state.messages,
                  {
                    // The static renderer dedupes rows by id, so a repeated
                    // persisted id needs a fresh row id; live-echo dedupe and
                    // rewind still match via kasMessageId.
                    ...(isCloudReplay
                      ? { id: generateMessageId(), kasMessageId: id }
                      : { id }),
                    role: MessageRole.User,
                    content: text,
                    agentName: state.currentAgent?.name,
                    // Real prompts precede turn_start; in-turn user rows are steers.
                    ...(turnOpen ? { steered: true } : {}),
                  },
                ],
              }));
            }
            break;
          case AgentEventType.Content:
            if (event.content.type === 'text') {
              const text = event.content.text;
              // First text after reasoning ends the thinking phase.
              if (thinkingStart !== null && thinkingMs === null) {
                thinkingMs = Date.now() - thinkingStart;
              }
              bufferedContent += text;
              lastContentEventId = event.id;

              if (!isBuffering) {
                // Schedule a batched flush instead of calling set() for
                // every chunk.  This lets Ink render between flushes.
                if (!pendingContentFlush) {
                  pendingContentFlush = setTimeout(flushContentToStore, 16);
                }
              }
            }
            break;
          case AgentEventType.Thought:
            // Thinking content — tracked separately for distinct rendering
            if (event.content.type === 'text') {
              if (thinkingStart === null) thinkingStart = Date.now();
              bufferedThinking += event.content.text;
              lastContentEventId = event.id;

              if (!isBuffering) {
                if (!pendingContentFlush) {
                  pendingContentFlush = setTimeout(flushContentToStore, 16);
                }
              }
            }
            break;
          case AgentEventType.ToolCall:
            // A tool call also ends the thinking phase.
            if (thinkingStart !== null && thinkingMs === null) {
              thinkingMs = Date.now() - thinkingStart;
            }
            if (isBuffering && bufferedContent) {
              commitBufferedContent();
              isBuffering = false;
            }
            // Flush buffered content before adding the tool message. We flush
            // unconditionally (not just when a timer is pending): on the common
            // think→tool-call path the thinking text's 16ms timer has already
            // fired, so the just-computed thinkingMs would otherwise be reset
            // away before reaching the model message. flushContentToStore()
            // no-ops when nothing is buffered.
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
            }
            flushContentToStore();
            // Commit the streaming row so the model speech that preceded the
            // tool call lands in scrollback before the tool row. Without
            // this, the placeholder row keeps empty content in messages
            // even though the user saw the text live, and the next chunk
            // after the tool would create a second placeholder.
            if (streamingMsgId != null) {
              commitBufferedContent();
            }
            // Report tool use to cmux sidebar
            syncCmuxStatus('tool-use', event.name);
            // Reset buffer so the next Model message after this tool
            // doesn't repeat text from before the tool call.
            bufferedContent = '';
            bufferedThinking = '';
            lastContentEventId = null;
            thinkingStart = null;
            thinkingMs = null;

            set((state) => {
              const existingIndex = state.messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );

              // Capture purpose + synthesize content. See
              // synthesizeToolUseContent for the per-shape rules.
              const { purpose, content } = synthesizeToolUseContent(event);
              // Structured diff for the modern TUI's <Tool> renderer (main's
              // path). Lite parses the synthesized `content` above instead;
              // carrying both keeps each renderer on its own source.
              const diff = deriveToolDiff(event);
              const isQuestion = event.meta?.kiro?.toolId === 'user_input';

              if (existingIndex !== -1) {
                const existingMsg = state.messages[existingIndex];
                if (existingMsg && existingMsg.role === MessageRole.ToolUse) {
                  const hasNewContent =
                    Object.keys(event.args).length > 0 || event.toolContent;
                  if (hasNewContent || isQuestion) {
                    const messages = [...state.messages];
                    messages[existingIndex] = {
                      ...existingMsg,
                      isQuestion: existingMsg.isQuestion || isQuestion,
                      sessionId: event.sessionId ?? existingMsg.sessionId,
                      pipelineGroupId:
                        event.meta?.kiro?.pipeline?.groupId ??
                        existingMsg.pipelineGroupId,
                      content,
                      purpose: purpose ?? existingMsg.purpose,
                      kind: event.kind || existingMsg.kind,
                      locations: event.locations || existingMsg.locations,
                      diff: diff ?? existingMsg.diff,
                    };
                    return { messages };
                  }
                }
                return state;
              }

              const isNotReady = NOT_READY_TOOLS.has(event.name);
              logger.debug('[tool-created]', event.name, event.id);
              // Wipe previous subagent state when a new crew invocation starts
              let clearedMessages = state.messages;
              let clearedSessions = state.sessions;
              let clearedEventBuffer = state.sessionEventBuffer;
              if (SESSION_TOOL_NAMES.has(event.name)) {
                const activeParentGroups = new Set<string>();
                const incomingPipelineGroup =
                  event.meta?.kiro?.pipeline?.groupId;
                if (incomingPipelineGroup !== undefined) {
                  activeParentGroups.add(incomingPipelineGroup);
                }
                let hasActiveUngroupedParent = false;
                for (const message of state.messages) {
                  if (
                    message.role !== MessageRole.ToolUse ||
                    !isParentSubagentTool(message.name) ||
                    message.isFinished
                  ) {
                    continue;
                  }
                  if (message.pipelineGroupId === undefined) {
                    hasActiveUngroupedParent = true;
                  } else {
                    activeParentGroups.add(message.pipelineGroupId);
                  }
                }

                const staleNames = new Set<string>();
                const staleSessionIds = new Set<string>();
                const newSessions = new Map<string, AgentSession>();
                for (const [id, s] of state.sessions) {
                  const belongsToActiveInvocation =
                    s.group === undefined
                      ? hasActiveUngroupedParent
                      : activeParentGroups.has(s.group);
                  if (
                    s.type === 'ephemeral' &&
                    id !== state.sessionId &&
                    !belongsToActiveInvocation
                  ) {
                    staleNames.add(s.name);
                    staleSessionIds.add(id);
                  } else {
                    newSessions.set(id, s);
                  }
                }
                if (staleSessionIds.size > 0) {
                  clearedSessions = newSessions;
                  clearedMessages = state.messages.filter((message) => {
                    if (message.role !== MessageRole.ToolUse) return true;
                    if (
                      message.sessionId &&
                      staleSessionIds.has(message.sessionId)
                    ) {
                      return false;
                    }
                    if (
                      message.pipelineGroupId &&
                      activeParentGroups.has(message.pipelineGroupId)
                    ) {
                      return true;
                    }
                    return (
                      !message.agentName || !staleNames.has(message.agentName)
                    );
                  });
                  clearedEventBuffer = { ...state.sessionEventBuffer };
                  for (const id of staleSessionIds) {
                    delete clearedEventBuffer[id];
                  }
                }
              }
              // Resolve agent name. For a stage tool (sessionId differs from
              // main) we MUST NOT fall back to the main agent's name: lite's
              // isInnerSubagentTool hides a tool iff agentName !== mainAgentName,
              // so stamping the main name leaks the stage's tools into the main
              // live region/scrollback. When the session isn't registered yet
              // (its first tool call beat the subagent_list_update), stamp the
              // raw sessionId as a placeholder; addSession backfills the real
              // name once the update lands.
              const mainSessionId = state.sessionId;
              let agentName: string | undefined;
              if (event.sessionId && event.sessionId !== mainSessionId) {
                agentName =
                  state.sessions.get(event.sessionId)?.name ?? event.sessionId;
              } else {
                agentName = state.currentAgent?.name;
              }
              return {
                sessions: clearedSessions,
                sessionEventBuffer: clearedEventBuffer,
                messages: [
                  ...clearedMessages,
                  {
                    id: event.id,
                    role: MessageRole.ToolUse,
                    name: event.name,
                    ...(isQuestion && { isQuestion: true }),
                    sessionId: event.sessionId,
                    pipelineGroupId: event.meta?.kiro?.pipeline?.groupId,
                    kind: event.kind,
                    content,
                    purpose,
                    diff,
                    locations: event.locations,
                    agentName,
                    ...(event.sessionId && { isSubagentTool: true }),
                    ...(fromHistory ? {} : { startTime: Date.now() }),
                    ...(isNotReady && {
                      isFinished: true,
                      result: {
                        status: 'error' as const,
                        error: `Tool "${event.name}" is not available`,
                      },
                    }),
                  },
                ],
              };
            });
            break;
          case AgentEventType.ToolCallUpdate:
            if (event.content.type === 'text') {
              const text = event.content.text;
              // Accumulate into per-tool buffer; flush on a short timer to
              // avoid re-rendering the whole message list for every line of
              // streamed output.
              toolOutputBuffers.set(
                event.id,
                (toolOutputBuffers.get(event.id) ?? '') + text
              );
              if (!pendingToolOutputFlush) {
                pendingToolOutputFlush = setTimeout(flushToolOutputs, 32);
              }
            }
            break;
          case AgentEventType.ToolCallFinished:
            // Flush any buffered live output before we mark the tool finished
            if (pendingToolOutputFlush) {
              clearTimeout(pendingToolOutputFlush);
              pendingToolOutputFlush = null;
            }
            flushToolOutputs();
            set((state) => {
              const messages = [...state.messages];
              const toolMsgIndex = messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );
              const newLiveOutputs = new Map(state.liveOutputs);
              newLiveOutputs.delete(event.id);
              if (toolMsgIndex !== -1) {
                const toolMsg = messages[toolMsgIndex];
                if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
                  logger.debug('[tool-finished]', toolMsg.name, event.id);
                  // Detect "denied by user" on replay: the V2 backend collapses
                  // user-rejected and actually-failed tool calls into the same
                  // wire shape (status: Failed, content: error text) and tunnels
                  // the distinction through the reason text. `isUserDeniedReason`
                  // localizes that V2-specific string coupling (no-op for KAS) so
                  // a replayed denial renders with the rejected glyph, not failed.
                  const errText =
                    event.result?.status === 'error'
                      ? event.result.error
                      : undefined;
                  const wasDeniedByUser = isUserDeniedReason(errText);
                  // If the user cancelled the tool (e.g. denied approval),
                  // the local cancellation flow already marked the message
                  // as 'cancelled'. Preserve that — don't let a subsequent
                  // backend-emitted ToolCallFinished (e.g. KAS sends
                  // status:'failed' for cancelled tools) overwrite it.
                  if (
                    toolMsg.isFinished &&
                    toolMsg.result?.status === 'cancelled'
                  ) {
                    return { messages, liveOutputs: newLiveOutputs };
                  }
                  // If the finished event carries diff content (e.g. from KAS,
                  // which only emits the file diff once the write completes),
                  // attach it as a structured field so <Write> can render it.
                  const wireDiff = event.toolContent?.[0];
                  const diff: ToolDiff | undefined = wireDiff
                    ? {
                        path: wireDiff.path,
                        newText: wireDiff.newText,
                        oldText: wireDiff.oldText,
                      }
                    : toolMsg.diff;
                  // Spread `...toolMsg` so any optional sibling fields the
                  // ToolCall handler stamped onto the message (notably the
                  // typed `purpose` carrying __tool_use_purpose) survive
                  // the finalize-on-finish rewrite. An explicit field list
                  // here previously dropped `purpose` the moment Rust
                  // emitted ToolCallFinished, wiping the lite TUI's purple
                  // reasoning text right after the user approved. `diff` is
                  // overridden below with the freshly-derived value.
                  messages[toolMsgIndex] = {
                    ...toolMsg,
                    diff,
                    isFinished: true,
                    status: wasDeniedByUser
                      ? ToolUseStatus.Rejected
                      : toolMsg.status,
                    result: event.result,
                    ...(fromHistory ? {} : { finishTime: Date.now() }),
                  };
                }
              } else {
                logger.debug(
                  '[tool-finished-NOT-FOUND]',
                  event.id,
                  'result:',
                  event.result?.status
                );
              }
              return { messages, liveOutputs: newLiveOutputs };
            });

            // Extract task state from task tool results
            extractTaskState(event, get);
            break;
          case AgentEventType.ApprovalRequest: {
            const {
              autoApproveCrewTools,
              agentEngine,
              sessionId: mainSessionId,
              trustAllToolsConfirmed,
            } = get();

            // --trust-all-tools: auto-approve all permission requests
            // Prefer allow_always (V2 parity: server learns tool is trusted),
            // fall back to allow_once if always isn't offered.
            if (
              trustAllToolsConfirmed &&
              (agentEngine !== 'kas' || hasKasToolConsentTarget(event.value))
            ) {
              const opt =
                event.value.permissionOptions.find(
                  (o: { kind: string }) => o.kind === 'allow_always'
                ) ??
                event.value.permissionOptions.find(
                  (o: { kind: string }) => o.kind === 'allow_once'
                );
              if (opt) {
                const resolvedMeta =
                  agentEngine === 'kas'
                    ? buildKasConsentMeta(event.value, opt.kind, {
                        kasWholeCapability: true,
                      })
                    : undefined;
                event.value.resolve({
                  outcome: 'selected',
                  optionId: opt.optionId,
                  ...(resolvedMeta ? { _meta: resolvedMeta } : {}),
                });
                break;
              }
            }

            const isCrewApproval = !!(
              event.value.sessionId &&
              mainSessionId &&
              event.value.sessionId !== mainSessionId
            );
            if (autoApproveCrewTools && isCrewApproval) {
              // Match by `kind` so KAS approvals (optionId='accept', kind='allow_once')
              // resolve correctly alongside Rust ones (optionId='allow_once').
              const opt = event.value.permissionOptions.find(
                (o: { optionId: string; kind?: string }) =>
                  o.kind === 'allow_once'
              );
              if (opt) {
                event.value.resolve({
                  outcome: 'selected',
                  optionId: opt.optionId,
                });
                break;
              }
            }
            const wasEditing = get().editingQueueIndex != null;

            set((state) => {
              const newQueue = [...state.approvalQueue, event.value];
              const toolCallId = event.value.toolCall.toolCallId;
              return {
                approvalQueue: newQueue,
                pendingApproval: state.pendingApproval ?? event.value,
                // Cancel any active queue edit when an approval arrives
                editingQueueIndex: null,
                commandInputValue:
                  state.editingQueueIndex != null
                    ? ''
                    : state.commandInputValue,
                messages: state.messages.map((msg) =>
                  msg.role === MessageRole.ToolUse && msg.id === toolCallId
                    ? { ...msg, status: ToolUseStatus.Pending }
                    : msg
                ),
              };
            });

            if (wasEditing) {
              get().showTransientAlert({
                message: 'Queue message edit cancelled — approval required',
                status: 'info',
                autoHideMs: 3000,
              });
            }
            break;
          }
          case AgentEventType.QuestionRequest: {
            if (get().wasCancelled) {
              event.value.resolve({ action: 'dismissed' });
              break;
            }
            if (
              get().questionQueue.some(
                (question) => question.toolCallId === event.value.toolCallId
              )
            ) {
              event.value.resolve({ action: 'dismissed' });
              break;
            }
            const wasEditing = get().editingQueueIndex != null;
            set((state) => {
              const isSubagentQuestion =
                event.value.sessionId !== state.sessionId;
              const agentName = isSubagentQuestion
                ? (state.sessions.get(event.value.sessionId)?.name ??
                  event.value.sessionId)
                : state.currentAgent?.name;
              const existing = state.messages.some(
                (message) =>
                  message.role === MessageRole.ToolUse &&
                  message.id === event.value.toolCallId
              );
              const messages = existing
                ? state.messages.map((message) =>
                    message.role === MessageRole.ToolUse &&
                    message.id === event.value.toolCallId
                      ? {
                          ...message,
                          name: event.value.question,
                          isQuestion: true,
                          status: ToolUseStatus.Pending,
                        }
                      : message
                  )
                : [
                    ...state.messages,
                    {
                      id: event.value.toolCallId,
                      role: MessageRole.ToolUse as const,
                      name: event.value.question,
                      isQuestion: true,
                      sessionId: event.value.sessionId,
                      content: '{}',
                      agentName,
                      ...(isSubagentQuestion && { isSubagentTool: true }),
                      status: ToolUseStatus.Pending,
                      startTime: Date.now(),
                    },
                  ];
              const questionQueue = [...state.questionQueue, event.value];
              return {
                messages,
                questionQueue,
                pendingQuestion: state.pendingQuestion ?? event.value,
                editingQueueIndex: null,
                commandInputValue:
                  state.editingQueueIndex != null
                    ? ''
                    : state.commandInputValue,
              };
            });
            if (wasEditing) {
              get().showTransientAlert({
                message: 'Queue message edit cancelled - answer required',
                status: 'info',
                autoHideMs: 3000,
              });
            }
            break;
          }
          case AgentEventType.ContextUsage:
            get().setContextUsage(event.percent);
            break;
          case AgentEventType.ContextBreakdownUpdate:
            set({ contextBreakdownCache: event.breakdown });
            break;
          case AgentEventType.McpServerSnapshot:
            set({ mcpServerCache: event.servers });
            break;
          case AgentEventType.McpRegistrySnapshot:
            set({ mcpRegistryCache: event.registryServers });
            break;
          case AgentEventType.SessionRosterDelta:
            get().applySessionRosterDelta(event.delta);
            break;
          case AgentEventType.SessionRepositoriesUpdate:
            // The sandbox's authoritative bound-repo set (attach/detach
            // mid-session).
            get().applySessionRepositories(event.repositories);
            break;
          case AgentEventType.KasMessageIdAssigned:
            get().setKasMessageId(event.kasMessageId);
            break;
          case AgentEventType.EffortUpdate:
            get().setCurrentEffort(event.effort);
            break;
          case AgentEventType.GoalStatus:
            if (event.state === 'cleared') {
              get().setGoalStatus(null);
            } else {
              const prev = get().goalStatus;
              // When the iteration advances, flush and reset the content
              // buffer so new-iteration content can't overwrite the
              // previous iteration's Model message in scrollback.
              if (
                prev &&
                event.iteration !== undefined &&
                event.iteration > prev.iteration
              ) {
                if (pendingContentFlush) {
                  clearTimeout(pendingContentFlush);
                  pendingContentFlush = null;
                }
                flushContentToStore();
                bufferedContent = '';
                bufferedThinking = '';
                lastContentEventId = null;
                thinkingStart = null;
                thinkingMs = null;
              }
              get().setGoalStatus({
                state: event.state,
                iteration: event.iteration,
                maxIterations: event.maxIterations,
                message: event.message ?? prev?.message,
                elapsedSecs: event.elapsedSecs,
                startedAt: prev?.startedAt,
              });
              if (event.state === 'completed' || event.state === 'exhausted') {
                setTimeout(() => get().setGoalStatus(null), 3000);
              }
            }
            break;
          case AgentEventType.Metadata:
            if (
              event.inputTokens !== undefined ||
              event.outputTokens !== undefined
            ) {
              get().setLastTurnTokens({
                input: event.inputTokens ?? 0,
                output: event.outputTokens ?? 0,
                cached: event.cachedTokens ?? 0,
              });
            }
            break;
          case AgentEventType.CompactionStatus:
            void get().handleCompactionEvent(event);
            break;
          case AgentEventType.AuthError:
            {
              const guidance = getAuthErrorGuidance(event.errorType);
              turnOpen = false;
              observerTurnBlocked = true;
              clearObserverTurnWatchdog();
              reset();
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
                _observerQueueBlocked: true,
              });
            }
            break;
          case AgentEventType.SessionError:
            {
              const guidance = getSessionErrorGuidance(
                event.errorType,
                event.pid
              );
              turnOpen = false;
              observerTurnBlocked = true;
              clearObserverTurnWatchdog();
              reset();
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
                lastTurnErrored: true,
                _observerQueueBlocked: true,
              });
            }
            break;
          case AgentEventType.McpServerInitFailure:
            {
              const current = get().initErrors;
              // Deduplicate by server name
              const updated = [
                ...current.filter(
                  (e) =>
                    !(
                      e.type === 'mcp_failure' &&
                      e.serverName === event.serverName
                    )
                ),
                {
                  type: 'mcp_failure' as const,
                  serverName: event.serverName,
                  error: event.error,
                },
              ];
              // Track MCP init status for the boot indicator's aggregate
              // count. Failure detail is carried by `initErrors` (above) and
              // the transient alert below — the per-server entry only needs
              // status + startTime.
              const mcpStatus = new Map(get().mcpInitStatus);
              const prev = mcpStatus.get(event.serverName);
              mcpStatus.set(event.serverName, {
                status: 'failed',
                startTime: prev?.startTime ?? Date.now(),
              });
              // A forced re-auth that failed (not-loaded path) must also clear the
              // `authenticating` overlay so the row reflects the failure, not a
              // perpetual "auth-required".
              const mcpServers = get().mcpServers.some(
                (s) => s.name === event.serverName && s.authenticating
              )
                ? get().mcpServers.map((s) =>
                    s.name === event.serverName
                      ? { ...s, authenticating: false }
                      : s
                  )
                : get().mcpServers;
              set({
                initErrors: updated,
                mcpInitStatus: mcpStatus,
                mcpServers,
              });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: severityForInitErrors(updated),
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.McpOauthRequest:
            {
              set((state) => {
                const updated = new Map(state.pendingOAuthServers);
                updated.set(event.serverName, event.oauthUrl);
                return { pendingOAuthServers: updated };
              });
            }
            break;
          case AgentEventType.McpServerInitialized:
            {
              set((state) => {
                const mcpStatus = new Map(state.mcpInitStatus);
                const prev = mcpStatus.get(event.serverName);
                mcpStatus.set(event.serverName, {
                  status: 'ready',
                  startTime: prev?.startTime ?? Date.now(),
                });

                // A server reaching "initialized" also resolves any forced re-auth
                // shadow targeting it (promoted, aborted, or failed-then-reloaded):
                // drop the pending-OAuth prompt AND clear the `authenticating`
                // overlay so the master row stops showing "auth-required".
                const hadPending = state.pendingOAuthServers.has(
                  event.serverName
                );
                const wasAuthenticating = state.mcpServers.some(
                  (s) => s.name === event.serverName && s.authenticating
                );
                if (!hadPending && !wasAuthenticating) {
                  return { mcpInitStatus: mcpStatus };
                }
                const pendingOAuthServers = new Map(state.pendingOAuthServers);
                pendingOAuthServers.delete(event.serverName);
                const mcpServers = wasAuthenticating
                  ? state.mcpServers.map((s) =>
                      s.name === event.serverName
                        ? { ...s, authenticating: false }
                        : s
                    )
                  : state.mcpServers;
                return {
                  pendingOAuthServers,
                  mcpInitStatus: mcpStatus,
                  mcpServers,
                };
              });
            }
            break;
          case AgentEventType.RateLimitError:
            {
              get().showTransientAlert({
                message: event.message,
                status: 'error',
                autoHideMs: 5000,
              });
            }
            break;
          case AgentEventType.ModelRefusal:
            {
              // Surface only the first refusal per turn (see refusalShownThisTurn).
              if (refusalShownThisTurn) break;
              refusalShownThisTurn = true;
              const message =
                event.explanation ??
                "The selected model couldn't process this request. Try a different model with /model, rewind with /rewind, or start a new session with /chat new.";
              set((s) => ({
                messages: [
                  ...s.messages,
                  {
                    id: generateMessageId(),
                    role: MessageRole.System,
                    content: message,
                    success: false,
                    ...(s.isProcessing ? { turnOwned: true } : {}),
                  },
                ],
              }));
            }
            break;
          case AgentEventType.SystemNotice:
            // A transient status remark (e.g. a cloud mode revert), shown as an
            // auto-dismissing banner — the same pattern as the cloud-only
            // command refusals ("… is not available for a cloud session yet.").
            // It is not persisted to chat history.
            get().showTransientAlert({
              message: event.message,
              status: event.success ? 'success' : 'error',
              autoHideMs: 5000,
            });
            break;
          case AgentEventType.RetryWarning:
            {
              get().setRetryStatus({
                attempt: event.attempt,
                maxAttempts: event.maxAttempts,
                delaySecs: event.delaySecs,
                message: event.message,
              });
            }
            break;
          case AgentEventType.AgentSwitched:
            get().setCurrentAgent({
              name: event.agentName,
              welcomeMessage: event.welcomeMessage,
            });
            if (event.previousAgentName) {
              set({ previousAgentName: event.previousAgentName });
            }
            if (event.model) {
              get().setCurrentModel({ id: event.model, name: event.model });
            }
            break;
          case AgentEventType.AgentNotFound:
            {
              const updated = [
                ...get().initErrors,
                {
                  type: 'agent_not_found' as const,
                  requestedAgent: event.requestedAgent,
                  fallbackAgent: event.fallbackAgent,
                },
              ];
              set({ initErrors: updated });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: severityForInitErrors(updated),
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.AgentConfigError:
            {
              const updated = [
                ...get().initErrors,
                {
                  type: 'agent_config_error' as const,
                  path: event.path,
                  error: event.error,
                },
              ];
              set({ initErrors: updated });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: severityForInitErrors(updated),
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.TurnSummary:
            // Handled by global handleTurnSummaryEvent, not here
            break;
          case AgentEventType.TurnStart:
            turnOpen = true;
            observerTurnBlocked = false;
            armObserverTurnWatchdog();
            if (!get().isProcessing) {
              set({ isProcessing: true });
            }
            break;
          case AgentEventType.TurnEnd: {
            const drainQueuedInput = !observerTurnBlocked;
            turnOpen = false;
            observerTurnBlocked = false;
            clearObserverTurnWatchdog();
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
            }
            flushContentToStore();
            reset();
            // A replayed cancel must close tools that never emitted a result.
            if (event.stopReason === 'cancelled') {
              set((state) => {
                if (
                  !state.messages.some(
                    (m) => m.role === MessageRole.ToolUse && !m.isFinished
                  )
                ) {
                  return {};
                }
                const newLiveOutputs = new Map(state.liveOutputs);
                const messages = state.messages.map((msg) => {
                  if (msg.role !== MessageRole.ToolUse || msg.isFinished)
                    return msg;
                  const result = buildCancelledResult(
                    state.liveOutputs.get(msg.id)
                  );
                  newLiveOutputs.delete(msg.id);
                  return {
                    ...msg,
                    isFinished: true,
                    status:
                      msg.status === ToolUseStatus.Approved
                        ? ToolUseStatus.Approved
                        : ToolUseStatus.Rejected,
                    result,
                  };
                });
                return { messages, liveOutputs: newLiveOutputs };
              });
            }
            if (get().isProcessing) {
              set({ isProcessing: false });
            }
            if (drainQueuedInput && get().isInitialized) {
              void get().processQueue();
            }
            break;
          }
          case AgentEventType.McpGovernanceDisabled:
            {
              const updated = [
                ...get().initErrors,
                {
                  type: 'mcp_governance_disabled' as const,
                  apiFailure: event.apiFailure,
                },
              ];
              set({ initErrors: updated });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: severityForInitErrors(updated),
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.WebToolsGovernanceDisabled:
            {
              const updated = [
                ...get().initErrors.filter(
                  (e) => e.type !== 'web_tools_governance_disabled'
                ),
                {
                  type: 'web_tools_governance_disabled' as const,
                  apiFailure: event.apiFailure,
                },
              ];
              set({ initErrors: updated });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: severityForInitErrors(updated),
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.SteeringQueued:
            // Backend-held steers must not also replay as prompts.
            set({
              pendingSteerContent: event.message,
              _steerReplayArmed: false,
            });
            break;
          case AgentEventType.SteeringConsumed:
            // The persistent handler owns steering state; the local handler owns content.
            get()._activeStreamHandler?.reset();
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
              flushContentToStore();
            }
            if (streamingMsgId != null) {
              commitBufferedContent();
            }
            bufferedContent = '';
            lastContentEventId = null;
            bufferedThinking = '';
            thinkingStart = null;
            thinkingMs = null;

            set((state) => ({
              pendingSteerContent: null,
              _steerReplayArmed: false,
              editingSteerLineIndex: null,
              messages: [
                ...state.messages,
                {
                  id: generateMessageId(),
                  role: MessageRole.User,
                  content: event.content,
                  agentName: state.currentAgent?.name,
                  steered: true,
                },
              ],
            }));
            break;
          case AgentEventType.SteeringCleared:
            // Backend cleared the queue without consuming it (cancel, or
            // explicit TUI clear request). Reset the activity-tray display
            // without adding a user bubble. Drop any steer-row edit chevron.
            // NOTE: replaceSteerMessage's clear→resteer also flows through
            // here, but it sets pendingSteerContent optimistically AND re-fires
            // SteeringQueued, so the display lands back on the edited text.
            set({ pendingSteerContent: null, editingSteerLineIndex: null });
            break;
          case AgentEventType.HooksUpdate:
            // Update cached hooks list. If the panel is open, it will
            // re-render with the new data automatically.
            set({ hooksList: event.hooks });
            break;
          case AgentEventType.ToolsUpdate:
            // Cache the latest session tool listing (pushed by KAS via
            // _kiro/tools/didChange). If the /tools panel is open it
            // re-renders automatically; otherwise the handler reads this
            // cache when opening the panel.
            set({ toolsList: event.tools });
            break;
        }
      };

      // Thin dispatch wrapper. `baseHandler` already guards `disposed`, so
      // this just forwards events; it exists so the returned value is the
      // `StreamEventHandler` callable that `Object.assign` augments with
      // `flush`/`dispose` below.
      const handle = (event: AgentStreamEvent) => {
        baseHandler(event);
      };

      // Happy-path commit. Cancel any pending batched flush and commit the
      // remaining buffered content synchronously.
      const flush = () => {
        if (disposed) return;
        if (pendingContentFlush) {
          clearTimeout(pendingContentFlush);
          pendingContentFlush = null;
        }
        // End-of-stream commit: drain the live streaming slot into the row.
        // flushContentToStore (the per-chunk path) only writes the primitive
        // after the first chunk; the buffered text never lands in messages
        // unless we explicitly commit. Need to ensure the placeholder row
        // exists first (in case stream ended on the very first chunk arriving
        // simultaneously with end-of-stream — rare, but commitBufferedContent
        // looks for a Model row by id and finds nothing).
        if (streamingMsgId == null && (bufferedContent || bufferedThinking)) {
          flushContentToStore();
        }
        commitBufferedContent();
        set({ streamingBuffer: { startBuffering: null, stopBuffering: null } });
      };

      // Discard everything pending without committing it to the store.
      // Called by cancelMessage when the user interrupts mid-turn — at that
      // point any setTimeout-scheduled flush would otherwise fire AFTER the
      // next turn's User message has been appended, and would either mutate
      // the new turn's Model message or (if no Model exists yet) append a
      // ghost Model with the previous turn's content. Setting `disposed`
      // makes baseHandler and both flush callbacks inert if they still fire
      // in the deferred-unsubscribe window (see kiro.ts::streamMessage).
      //
      // Partial-response preservation: under the streaming-slot design the
      // live row in `messages` is empty until commitBufferedContent runs.
      // Commit BEFORE zeroing so the partial response that the user already
      // saw in the live region lands in scrollback at cancel time — matches
      // pre-E1 behavior where mid-stream `[...state.messages]` writes had
      // already left the row partially populated. (This is the lite
      // behavioral contract; commitBufferedContent does not consult
      // `disposed`, so the direct call here still writes.)
      const dispose = () => {
        if (disposed) return;
        disposed = true;
        clearObserverTurnWatchdog();
        // Finalize an in-flight reasoning block. When a turn is abandoned
        // (cancel/error) while the model is still reasoning — before any
        // answer text or tool call ended the thinking phase — `thinkingMs`
        // was never stamped. Stamp it now so <ThinkingDisplay> closes the
        // block ("Thought for Ns") instead of rendering a permanently-live
        // "Thinking..." header in scrollback.
        if (thinkingStart !== null && thinkingMs === null) {
          thinkingMs = Date.now() - thinkingStart;
          const finalizedThinkingMs = thinkingMs;
          set((state) => {
            const idx = state.messages.findLastIndex(
              (msg) => msg.role === MessageRole.Model
            );
            if (idx === -1) return {};
            const msg = state.messages[idx];
            if (
              msg &&
              msg.role === MessageRole.Model &&
              msg.thinking &&
              msg.thinkingMs == null
            ) {
              const messages = [...state.messages];
              messages[idx] = { ...msg, thinkingMs: finalizedThinkingMs };
              return { messages };
            }
            return {};
          });
        }
        if (pendingContentFlush) {
          clearTimeout(pendingContentFlush);
          pendingContentFlush = null;
        }
        if (pendingToolOutputFlush) {
          clearTimeout(pendingToolOutputFlush);
          pendingToolOutputFlush = null;
        }
        commitBufferedContent();
        bufferedContent = '';
        bufferedThinking = '';
        lastContentEventId = null;
        toolOutputBuffers.clear();
        set({ streamingBuffer: { startBuffering: null, stopBuffering: null } });
      };

      // Reset commits the visible partial response without retiring the handler.
      const reset = () => {
        if (disposed) return;
        turnOpen = false;
        clearObserverTurnWatchdog();
        if (thinkingStart !== null && thinkingMs === null) {
          thinkingMs = Date.now() - thinkingStart;
        }
        if (pendingContentFlush) {
          clearTimeout(pendingContentFlush);
          pendingContentFlush = null;
        }
        if (pendingToolOutputFlush) {
          clearTimeout(pendingToolOutputFlush);
          pendingToolOutputFlush = null;
        }
        // Materialize the row before committing a buffer that beat its timer.
        if (streamingMsgId == null && (bufferedContent || bufferedThinking)) {
          flushContentToStore();
        }
        commitBufferedContent();
        currentPersistedId = null;
        bufferedContent = '';
        bufferedThinking = '';
        lastContentEventId = null;
        thinkingStart = null;
        thinkingMs = null;
        refusalShownThisTurn = false;
        toolOutputBuffers.clear();
      };

      const resetSession = () => {
        reset();
        seenPersistedIds.clear();
        observerTurnBlocked = false;
      };

      const setHistoryReplay = (value: boolean) => {
        fromHistory = value;
      };

      return Object.assign(handle, {
        flush,
        dispose,
        setHistoryReplay,
        reset,
        resetSession,
      });
    },

    /**
     * Backward-compatible wrapper: consumes an async generator using the
     * event handler. Used by unit tests that pass mock async generators.
     */
    processMessageStream: async (stream: AsyncGenerator<AgentStreamEvent>) => {
      const handler = get().createStreamEventHandler();
      for await (const event of stream) {
        handler(event);
      }
      handler.flush();
    },

    cancelMessage: async () => {
      const { kiro, currentAbortController, cancelInProgress } = get();
      if (!kiro) return;
      // If a cancel is already in flight, await it instead of starting a
      // second concurrent cancel. Two cancels back-to-back would race the
      // `finally` block that clears isProcessing — the second would see
      // currentAbortController already nulled and might issue a duplicate
      // kiro.cancel() against the new turn after the first one returns.
      if (cancelInProgress) return cancelInProgress;
      let resolveCancelPromise: () => void;
      const cancelPromise = new Promise<void>((resolve) => {
        resolveCancelPromise = resolve;
      });
      set({ cancelInProgress: cancelPromise, wasCancelled: true });

      // Capture the pending steer CONTENT (not just a boolean) before issuing
      // cancel. `kiro.cancel()` makes the backend drop its queued steer and
      // emit `SteeringCleared`, whose handler sets `pendingSteerContent: null`
      // (see the SteeringCleared case in createStreamEventHandler). Without
      // capturing it here, by the time `processQueue()` runs below the steer
      // is already gone and the user's mid-turn message is silently lost —
      // even though the intended UX is "cancel = redirect": the steer should
      // replay as a fresh prompt immediately after the interrupt. We re-seed
      // it after cancel resolves so processQueue's steer-first replay fires.
      // (`queuedMessages` is a local buffer and survives cancel untouched.)
      const capturedSteer = get().pendingSteerContent;
      const messageCountBeforeCancel = get().messages.length;
      const hasPendingMessages =
        capturedSteer != null || get().queuedMessages.length > 0;

      try {
        // Dispose the active stream event handler FIRST, before anything
        // async runs. This (a) commits any buffered streaming content into
        // the placeholder Model row so the partial response the user just
        // watched stream lands in scrollback at cancel time, and (b) drops
        // any pending content/tool-output flush setTimeouts so they can't
        // fire after the next sendMessage has started a new turn — without
        // this, a setTimeout scheduled by the cancelled turn would run
        // after the next User message has been appended and would either
        // mutate the new turn's Model message or append a ghost Model with
        // the cancelled turn's content.
        const activeHandler = get()._activeStreamHandler;
        if (activeHandler) {
          activeHandler.dispose?.();
          set({ _activeStreamHandler: null });
        }
        // Cancelling an observer turn must leave its session renderer reusable.
        else {
          get()._liveStreamHandler?.reset();
        }
        // Clear the live streaming slot so LiteLiveRegion doesn't repaint
        // the previous turn's partial text on the next isProcessing flip.
        // (commitBufferedContent above already wrote it into messages.)
        set({
          streamingContent: '',
          streamingMessageId: null,
          thinkingContent: '',
        });

        // Abort local stream first
        if (currentAbortController) {
          currentAbortController.abort();
          set({ currentAbortController: null });
        }

        // Flip isProcessing OFF now, before the (potentially slow) backend
        // round-trip below. `kiro.cancel()` awaits the in-flight prompt with
        // a 5s timeout race (see Kiro.cancel) — if the agent hung mid-tool,
        // that's a multi-second block. Leaving isProcessing=true across it
        // means a fresh prompt the user types right after Ctrl+C ("cancel =
        // redirect") gets misrouted by handleUserInput into the STEER/queue
        // path (default interrupt mode is STEER) and sent as `_session/steer`
        // against a session that's being torn down — silently lost. Resetting
        // here lets the next prompt route as a normal `session/prompt`. The
        // `finally` below still clears it as the belt-and-suspenders safety net.
        set({ isProcessing: false });

        // Resolve pending UI callbacks before cancelling the agent turn.
        get().cancelApproval();
        get().cancelQuestion();

        // Mark any unfinished tool uses as finished with cancelled status
        // immediately — before async calls. This stops spinners and prevents
        // a leak if kiro.cancel() is slow or throws.
        //
        // For shell-class tools the user has watched stdout stream past in
        // the live region as the command ran; that buffered partial output
        // lives in `state.liveOutputs` keyed by tool-call id. Snapshot it
        // into the cancelled tool's `result.output` so the renderer surfaces
        // it under the yellow `✗ cancelled` header instead of dropping the
        // whole body. Without this the user only sees the chip and has no
        // record of what the command had produced before the interrupt
        // landed — even though some commands complete useful work before
        // we can kill them.
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );

          if (hasUnfinishedToolCalls) {
            const newLiveOutputs = new Map(state.liveOutputs);
            const messages = state.messages.map((msg) => {
              if (msg.role !== MessageRole.ToolUse || msg.isFinished)
                return msg;
              const result = buildCancelledResult(
                state.liveOutputs.get(msg.id)
              );
              newLiveOutputs.delete(msg.id);
              return {
                ...msg,
                isFinished: true,
                // Clear Pending so the shimmer gate
                // (effectiveFinished = isFinished && status !== Pending)
                // resolves. A tool the user already Approved stays
                // Approved — `result.status === 'cancelled'` drives the
                // user-visible 'Cancelled' label regardless of the internal
                // status.
                status:
                  msg.status === ToolUseStatus.Approved
                    ? ToolUseStatus.Approved
                    : ToolUseStatus.Rejected,
                result,
              };
            });
            return { messages, liveOutputs: newLiveOutputs };
          }

          return {};
        });

        // Terminate any active crew sessions so subagents don't keep running
        if (get().sessions.size > 0) {
          await get().terminateAllCrewSessions();
        }

        // Then notify backend — this must complete before a new prompt
        // can be sent, otherwise the backend rejects with
        // "Prompt already in progress".
        await kiro.cancel();

        // If there are pending messages (steer or queue), skip the generic
        // "Cancelled streaming" toast since a new turn will start immediately.
        if (!hasPendingMessages) {
          get().showTransientAlert({
            message: 'Cancelled streaming',
            status: 'info',
            autoHideMs: 2000,
          });
        }
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : 'Cancel failed';
        set({
          agentError: errorMessage,
          agentErrorGuidance: getErrorGuidance(errorMessage).message,
        });
      } finally {
        // Always clear isProcessing — this is the safety net that prevents
        // the "Prompt already in progress" desync. Without this, if
        // kiro.cancel() throws or the abort signal doesn't propagate,
        // isProcessing stays true forever and blocks all future prompts.
        set({
          isProcessing: false,
          currentAbortController: null,
          // Cancelling ends the turn — drop any retry banner so it doesn't
          // linger under the next "Thinking..." spinner.
          retryStatus: null,
        });
        resolveCancelPromise!();
        set({ cancelInProgress: null });
      }

      // Re-seed the steer the backend's SteeringCleared wiped during cancel,
      // so processQueue's steer-first replay sends it as a fresh prompt. Only
      // restore if nothing newer arrived in the meantime (a fresh steer typed
      // during the cancel await wins). Without this, the captured steer is
      // lost — the bug this guards against.
      const steerWasConsumed =
        capturedSteer != null &&
        get()
          .messages.slice(messageCountBeforeCancel)
          .some(
            (message) =>
              message.role === MessageRole.User &&
              message.steered &&
              message.content === capturedSteer
          );
      if (capturedSteer != null && !steerWasConsumed) {
        // A newer steer typed during cancellation wins over the captured redirect.
        set({
          ...(get().pendingSteerContent == null
            ? { pendingSteerContent: capturedSteer }
            : {}),
          _steerReplayArmed: true,
        });
      }
      // Drain pending messages after cancel resolves. processQueue handles
      // steer-first priority internally: steer replays first, then queue drains.
      // Done outside the try/finally so it doesn't race with the
      // isProcessing reset — sendMessage() will flip it back on.
      await get().processQueue();
    },

    setProcessing: (isProcessing) => set({ isProcessing }),
    setAgentError: (agentError, guidance) =>
      set({
        agentError,
        agentErrorGuidance: guidance ?? null,
        ...(agentError == null ? { _observerQueueBlocked: false } : {}),
      }),
    setCurrentModel: (currentModel) => set({ currentModel }),

    beginKasSession: (origin) => {
      const { sessionOrigin, previousModelId } = get().kas;
      set((s) => ({
        kas: { ...s.kas, sessionOrigin: origin, previousModelId: null },
      }));
      // Restore the prior tracking state, for callers to invoke if the
      // session RPC fails and the previous session is still active.
      return () =>
        set((s) => ({ kas: { ...s.kas, sessionOrigin, previousModelId } }));
    },
    setCurrentEffort: (currentEffort) => set({ currentEffort }),

    handleKasModelConfigEvent: (event) => {
      const currModel = event.currentModelId
        ? event.models.find((m) => m.id === event.currentModelId)
        : undefined;
      // Update the models/efforts cache along with the current model. An
      // empty models list is the cloud config-surface reset:
      // the displayed current model belongs to the previous session, so
      // blank it until the sandbox reports its own.
      set((s) => ({
        kas: {
          ...s.kas,
          availableModels: event.models,
          availableEfforts: event.efforts,
        },
        currentEffort: event.currentLevel,
        ...(currModel
          ? { currentModel: { id: currModel.id, name: currModel.name } }
          : event.models.length === 0
            ? { currentModel: null }
            : {}),
      }));

      // Track the active model so the next update can tell whether it changed.
      // Session origin and baseline reset are owned by the session-start callers
      // (see beginKasSession); this handler only reacts to model updates.
      // hadPriorModel must be captured before the set() below overwrites it.
      const currentModelId = event.currentModelId ?? null;
      const hadPriorModel = get().kas.previousModelId != null;
      const modelChanged = currentModelId !== get().kas.previousModelId;
      set((s) => ({ kas: { ...s.kas, previousModelId: currentModelId } }));

      // Decide whether to auto-apply this model's saved effort default, then
      // resolve the concrete level (validity/idempotency checks) and push it.
      const effortToApply = resolveEffortToApply({
        currentModelId,
        availableEfforts: event.efforts.map((e) => e.value),
        currentEffort: event.currentLevel,
        savedEffortForModel: event.currentModelId
          ? (readSavedEffortDefault(event.currentModelId) ?? null)
          : null,
        shouldApply: shouldApplyEffortDefault({
          origin: event.origin,
          sessionOrigin: get().kas.sessionOrigin,
          modelChanged,
          hasExplicitEffort: get().kas.effortExplicit,
          hadPriorModel,
        }),
      });
      if (effortToApply) {
        void get()
          .kiro.setConfigOption('effortLevel', effortToApply)
          .catch((err) => {
            logger.error('[store] auto-apply effort default failed', err);
          });
      }
    },

    setGoalStatus: (goalStatus) => {
      const prev = get().goalStatus;
      if (goalStatus && goalStatus.state === 'active' && !prev) {
        const rawDesc = goalStatus.message ?? 'Goal started';
        const desc =
          rawDesc.length > 80 ? rawDesc.slice(0, 77) + '...' : rawDesc;
        const maxIter = goalStatus.maxIterations ?? 5;
        set((s) => ({
          goalStatus,
          messages: [
            ...s.messages,
            {
              id: generateMessageId(),
              role: MessageRole.System,
              content: `⟳ Goal: "${desc}" · ${maxIter} iteration${maxIter === 1 ? '' : 's'} max`,
              success: true,
              ...(s.isProcessing ? { turnOwned: true } : {}),
            },
          ],
        }));
      } else {
        set({ goalStatus });
      }
    },
    setCurrentAgent: (agent, options) => {
      const prevAgent = get().currentAgent;
      // The artifact-generation card belongs to the spec workflow's
      // active agent. Switching agents (e.g. spec → kiro_planner →
      // anything else) means any in-flight card is stale. Clear it.
      // We only update the field when there's actually an agent change
      // and an entry to clear, so no-op rerenders are avoided.
      const isAgentChanging = prevAgent?.name !== agent?.name;
      const hasGenerating = get().artifactGenerating !== null;
      // Same staleness rule for the /spec new description step: every path
      // that changes the agent funnels through here (commands, pickers,
      // shift+tab, backend switches), so leaving spec voids the step.
      const dropsSpecStep =
        isAgentChanging &&
        agent?.name !== 'spec' &&
        get().pendingSpecDescription !== null;
      set({
        currentAgent: agent ? { name: agent.name } : null,
        ...(isAgentChanging && hasGenerating
          ? { artifactGenerating: null }
          : {}),
        ...(dropsSpecStep ? { pendingSpecDescription: null } : {}),
      });
      if (dropsSpecStep) {
        get().showTransientAlert({
          message: 'Spec setup cancelled',
          status: 'info',
          autoHideMs: 3000,
        });
      }

      // Trigger plan quality survey when switching away from planner
      // (the handoff moment — plan was presented and user approved it).
      if (
        prevAgent?.name === 'kiro_planner' &&
        agent?.name &&
        agent.name !== 'kiro_planner'
      ) {
        queueMicrotask(() => get().triggerPlanSurvey());
      }

      // Welcome banner rides on the agent-switch payload from its single owner:
      // V2's backend pushes it on `kiro.dev/agent/switched`; KAS resolves it
      // from the current mode option (new/load via the session result, switches
      // via `emitConfigOptions`). No store lookup — the text is always inline.
      // Gated on an actual agent change so a re-assertion of the same agent
      // (e.g. KAS echoing `current_mode_update` at session start, or an
      // autonomous `config_option_update` that didn't change the mode) never
      // re-fires the banner. This is the single idempotency point: emitters can
      // broadcast the current agent unconditionally and rely on this guard.
      const welcomeMessage = agent?.welcomeMessage;
      if (welcomeMessage && isAgentChanging && !options?.suppressWelcome) {
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: generateMessageId(),
              role: MessageRole.Model,
              content: welcomeMessage,
              agentName: agent!.name,
              standalone: true,
            },
          ],
        }));
      }
    },
    setPreviousAgentName: (previousAgentName) => set({ previousAgentName }),

    handleCompactionEvent: async (event) => {
      if (event.type === AgentEventType.ContextUsage) {
        logger.debug(
          '[context-usage] ContextUsage event in compactionHandler, percent=',
          event.percent
        );
        get().setContextUsage(event.percent);
        return;
      }
      if (event.type === AgentEventType.ContextBreakdownUpdate) {
        set({ contextBreakdownCache: event.breakdown });
        return;
      }
      if (event.type === AgentEventType.KasMessageIdAssigned) {
        get().setKasMessageId(event.kasMessageId);
        return;
      }
      if (event.type === AgentEventType.EffortUpdate) {
        get().setCurrentEffort(event.effort);
        return;
      }
      if (event.type === AgentEventType.SessionRosterDelta) {
        get().applySessionRosterDelta(event.delta);
        // A fresh cloud session's startup checklist repaints on these deltas; if
        // a /clear or /chat new armed the scrollback reconcile, re-wipe now so
        // the wipe lands after this repaint rather than a guessed instant.
        noteCloudScrollbackRepaint();
        return;
      }
      if (event.type === AgentEventType.SessionRepositoriesUpdate) {
        // Sandbox repo attach/detach pushes arrive at turn boundaries (idle
        // time), so this lane must apply them too.
        get().applySessionRepositories(event.repositories);
        noteCloudScrollbackRepaint();
        return;
      }
      if (event.type !== AgentEventType.CompactionStatus) return;
      if (event.status === 'started') {
        // No empty User message: the previous push left a phantom blank row in
        // scrollback every /compact. `loadingMessage` drives a proper
        // "Compacting conversation..." spinner in both UIs (lite spinner row /
        // TUI NotificationBar) instead of the generic isProcessing "thinking"
        // indicator. Keep isProcessing untouched; isCompacting/loadingMessage
        // are the busy gates for compaction.
        set({
          isCompacting: true,
          activeCompactionAttemptKey: getNextCompactionAttemptKey(
            get(),
            event.attemptId
          ),
          loadingMessage: 'Compacting conversation...',
        });
      } else if (event.status === 'completed') {
        const summary = event.summary;
        let shouldDrainQueue = false;
        set((state) => {
          const isActiveEvent = isActiveCompactionTerminalEvent(
            state,
            event.attemptId
          );
          shouldDrainQueue = isActiveEvent;
          if (!isActiveEvent) {
            const anchor = state.compactionReportAnchor;
            if (
              summary &&
              event.attemptId != null &&
              anchor?.attemptKey === event.attemptId
            ) {
              return {
                compactionReportAnchor: null,
                messages: insertCompactionReport(
                  state.messages,
                  summary,
                  anchor.index
                ),
              };
            }
            return {};
          }
          return {
            isCompacting: false,
            activeCompactionAttemptKey: null,
            compactionReportAnchor:
              summary || event.attemptId == null
                ? null
                : {
                    attemptKey: event.attemptId,
                    index: state.messages.length,
                  },
            loadingMessage: null,
            transientAlert: null,
            messages: summary
              ? insertCompactionReport(state.messages, summary)
              : state.messages,
          };
        });
        if (shouldDrainQueue) await get().processQueue();
      } else if (event.status === 'failed') {
        let shouldDrainQueue = false;
        let showAlert = false;
        set((state) => {
          const isActiveEvent = isActiveCompactionTerminalEvent(
            state,
            event.attemptId
          );
          shouldDrainQueue = isActiveEvent;
          showAlert = isActiveEvent;
          if (!isActiveEvent) return {};
          return {
            isCompacting: false,
            activeCompactionAttemptKey: null,
            compactionReportAnchor:
              event.attemptId != null &&
              state.compactionReportAnchor?.attemptKey === event.attemptId
                ? null
                : state.compactionReportAnchor,
            loadingMessage: null,
          };
        });
        if (showAlert) {
          get().showTransientAlert({
            message: event.error
              ? `Compaction failed: ${event.error}`
              : 'Compaction failed',
            status: 'error',
            autoHideMs: 5000,
          });
        }
        if (shouldDrainQueue) await get().processQueue();
      }
    },

    handleTurnSummaryEvent: (event) => {
      if (event.type !== AgentEventType.TurnSummary) return;
      // Aggregate by unit (e.g. multiple "credits" entries → single total)
      const totals = new Map<string, { value: number; label: string }>();
      for (const u of event.meteringUsage) {
        const key = u.unitPlural;
        const existing = totals.get(key);
        if (existing) {
          existing.value += u.value;
        } else {
          totals.set(key, {
            value: u.value,
            label: key.charAt(0).toUpperCase() + key.slice(1),
          });
        }
      }
      const parts: string[] = [];
      for (const { value, label } of totals.values()) {
        parts.push(`${label}: ${(Math.floor(value * 100) / 100).toFixed(2)}`);
      }
      if (event.turnDurationMs != null) {
        const s = Math.floor(event.turnDurationMs / 1000);
        parts.push(
          `Time: ${s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`}`
        );
      }
      if (parts.length === 0) return;
      const text = `${parts.join(' • ')}`;
      // Find the last User message ID as the turn key
      const msgs = get().messages;
      let turnId: string | undefined;
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i]!.role === MessageRole.User) {
          turnId = msgs[i]!.id;
          break;
        }
      }
      if (!turnId) return;
      set((state) => {
        const m = new Map(state.turnSummaries);
        m.set(turnId, text);
        return { turnSummaries: m };
      });
    },

    respondToApproval: (
      optionId: string,
      target?: ApprovalRequestInfo,
      _meta?: Record<string, unknown>
    ) => {
      const { pendingApproval, approvalQueue, messages } = get();
      const approval = target ?? pendingApproval;
      if (approval) {
        const toolCallId = approval.toolCall.toolCallId;
        const resolvedKind = resolveApprovalOptionKind(approval, optionId);
        const isRejected =
          resolvedKind === ApprovalOptionId.RejectOnce ||
          resolvedKind === ApprovalOptionId.RejectAlways;
        const isKasApproval = get().agentEngine === 'kas';
        const isTrust =
          resolvedKind === ApprovalOptionId.AllowAlways &&
          !_meta?.trustOption &&
          !hasKasResourceTrustMeta(_meta) &&
          (!isKasApproval ||
            usesKasWholeCapabilityResource(resolvedKind, _meta));

        // When trusting a tool, cascade to all pending approvals of the same tool
        let cascadeApprovals: ApprovalRequestInfo[] = [];
        if (isTrust) {
          const trustedIdentity = approvalTrustIdentity(
            approval,
            messages,
            get().agentEngine
          );
          if (trustedIdentity) {
            cascadeApprovals = approvalQueue.filter((a) => {
              if (a === approval) return false;
              return (
                approvalTrustIdentity(a, messages, get().agentEngine) ===
                trustedIdentity
              );
            });
          }
        }

        const cascadeIds = new Set(
          cascadeApprovals.map((a) => a.toolCall.toolCallId)
        );

        // Update the tool call status based on user response
        const remainingQueue = approvalQueue.filter(
          (a) => a !== approval && !cascadeIds.has(a.toolCall.toolCallId)
        );
        const nextApproval = remainingQueue[0] ?? null;

        const feedbackText = (_meta as Record<string, unknown>)?.feedback as
          | string
          | undefined;
        const now = Date.now();

        set((state) => {
          const updatedMessages = state.messages.map((msg) => {
            if (msg.role !== MessageRole.ToolUse) return msg;
            if (msg.id === toolCallId) {
              return {
                ...msg,
                status: isRejected
                  ? ToolUseStatus.Rejected
                  : ToolUseStatus.Approved,
                isFinished: isRejected ? true : msg.isFinished,
                startTime: isRejected ? msg.startTime : now,
              };
            }
            // Cascade the trusted approval to other queued tool calls of
            // the same tool name.
            if (msg.role === MessageRole.ToolUse && cascadeIds.has(msg.id)) {
              return { ...msg, status: ToolUseStatus.Approved };
            }
            // When the last approval resolves, reset startTime for all other
            // unfinished tools that were blocked waiting so the timer doesn't
            // show inflated wait time.
            if (
              !isRejected &&
              remainingQueue.length === 0 &&
              msg.role === MessageRole.ToolUse &&
              !msg.isFinished
            ) {
              return { ...msg, startTime: now };
            }
            return msg;
          });

          // Show feedback as a user message in scrollback (display-only, not sent as prompt)
          if (isRejected && feedbackText) {
            updatedMessages.push({
              id: crypto.randomUUID(),
              role: MessageRole.User,
              content: feedbackText,
              agentName: state.currentAgent?.name,
            });
          }

          return {
            messages: updatedMessages,
            approvalQueue: remainingQueue,
            pendingApproval:
              state.pendingApproval === approval ||
              cascadeIds.has(state.pendingApproval?.toolCall.toolCallId ?? '')
                ? nextApproval
                : state.pendingApproval,
            approvalMode: 'dropdown',
          };
        });

        const resolvedMeta =
          get().agentEngine === 'kas'
            ? buildKasConsentMeta(approval, resolvedKind, _meta)
            : _meta;

        approval.resolve({
          outcome: 'selected',
          optionId,
          ...(resolvedMeta ? { _meta: resolvedMeta } : {}),
        });

        // Auto-resolve cascaded approvals with allow_once (trust is already applied)
        for (const cascaded of cascadeApprovals) {
          const allowOnceOptionId = resolveApprovalOptionIdByKind(
            cascaded,
            ApprovalOptionId.AllowOnce
          );
          const cascadedKind = resolveApprovalOptionKind(
            cascaded,
            allowOnceOptionId
          );
          const cascadedMeta =
            get().agentEngine === 'kas'
              ? buildKasConsentMeta(cascaded, cascadedKind, undefined)
              : undefined;
          cascaded.resolve({
            outcome: 'selected',
            optionId: allowOnceOptionId,
            ...(cascadedMeta ? { _meta: cascadedMeta } : {}),
          });
        }
      }
    },

    cancelApproval: () => {
      const { pendingApproval, approvalQueue } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;

        // Cancel all queued approvals, not just the current one
        const remainingQueue = approvalQueue.filter(
          (a) => a !== pendingApproval
        );

        // Collect all tool call IDs to cancel (current + queued)
        const cancelIds = new Set<string>();
        cancelIds.add(toolCallId);
        for (const queued of remainingQueue) {
          cancelIds.add(queued.toolCall.toolCallId);
        }

        // Mark all cancelled tool calls as finished. Approval-stage cancels
        // usually have no live output yet (the call hasn't started), but
        // snapshot liveOutputs anyway so the rare race where the user
        // declines mid-execution still preserves whatever stdout streamed.
        set((state) => {
          const newLiveOutputs = new Map(state.liveOutputs);
          const messages = state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && cancelIds.has(msg.id)) {
              const result = buildCancelledResult(
                state.liveOutputs.get(msg.id)
              );
              newLiveOutputs.delete(msg.id);
              return {
                ...msg,
                isFinished: true,
                // Clear Pending status so `effectiveFinished` evaluates to true
                // and the shimmer stops. Use Rejected to mirror an explicit
                // user-driven denial.
                status: ToolUseStatus.Rejected,
                result,
              };
            }
            return msg;
          });
          return { messages, liveOutputs: newLiveOutputs };
        });

        pendingApproval.resolve({ outcome: 'cancelled' });

        // Cancel all remaining queued approvals too
        for (const queued of remainingQueue) {
          queued.resolve({ outcome: 'cancelled' });
        }

        set({
          pendingApproval: null,
          approvalQueue: [],
          approvalMode: 'dropdown',
        });
      }
    },

    respondToQuestion: (answer, target, answerForAgent = answer) => {
      const { pendingQuestion, questionQueue } = get();
      if (!pendingQuestion || target !== pendingQuestion) return false;

      const remainingQueue = questionQueue.filter(
        (question) => question !== pendingQuestion
      );
      set((state) => {
        const answerAgentName =
          pendingQuestion.sessionId !== state.sessionId
            ? (state.sessions.get(pendingQuestion.sessionId)?.name ??
              pendingQuestion.sessionId)
            : state.currentAgent?.name;
        const messages = state.messages.map((message) =>
          message.role === MessageRole.ToolUse &&
          message.id === pendingQuestion.toolCallId
            ? { ...message, status: ToolUseStatus.Approved }
            : message
        );
        const answerMessage: MessageType = {
          id: crypto.randomUUID(),
          role: MessageRole.User,
          content: answer,
          agentName: answerAgentName,
          questionToolCallId: pendingQuestion.toolCallId,
        };
        const questionIndex = messages.findIndex(
          (message) =>
            message.role === MessageRole.ToolUse &&
            message.id === pendingQuestion.toolCallId
        );
        messages.splice(
          questionIndex < 0 ? messages.length : questionIndex + 1,
          0,
          answerMessage
        );
        return {
          messages,
          questionQueue: remainingQueue,
          pendingQuestion: remainingQueue[0] ?? null,
        };
      });
      pendingQuestion.resolve({
        action: 'answered',
        answer: answerForAgent,
      });
      return true;
    },

    cancelQuestion: () => {
      const { questionQueue } = get();
      if (questionQueue.length === 0) return;
      const questionIds = new Set(
        questionQueue.map((question) => question.toolCallId)
      );
      for (const question of questionQueue) {
        question.resolve({ action: 'dismissed' });
      }
      set((state) => ({
        pendingQuestion: null,
        questionQueue: [],
        messages: state.messages.map((message) =>
          message.role === MessageRole.ToolUse &&
          questionIds.has(message.id) &&
          !message.isFinished
            ? {
                ...message,
                isFinished: true,
                status: ToolUseStatus.Rejected,
                result: { status: 'cancelled' as const },
              }
            : message
        ),
      }));
    },

    setApprovalMode: (mode) => set({ approvalMode: mode }),

    setPendingSpecDescription: (pending) =>
      set({ pendingSpecDescription: pending }),

    cancelPendingSpecDescription: () => {
      if (!get().pendingSpecDescription) return;
      set({ pendingSpecDescription: null });
      get().showTransientAlert({
        message: 'Spec setup cancelled',
        status: 'info',
        autoHideMs: 3000,
      });
    },

    setAutoApproveCrewTools: (value) => set({ autoApproveCrewTools: value }),
    setFocusedCrewIndex: (index) => set({ focusedCrewIndex: index }),

    // Keeps last turn visible for /clear
    clearMessages: () => {
      const msgs = get().messages;
      if (msgs.length < 2) {
        set({
          activeCompactionAttemptKey: null,
          compactionReportAnchor: null,
        });
        return;
      }

      // Find the last user message to keep the entire last turn
      let lastUserIndex = -1;
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i]?.role === MessageRole.User) {
          lastUserIndex = i;
          break;
        }
      }

      if (lastUserIndex === -1) {
        set({
          activeCompactionAttemptKey: null,
          compactionReportAnchor: null,
        });
        return;
      }
      set({
        messages: msgs.slice(lastUserIndex),
        activeCompactionAttemptKey: null,
        compactionReportAnchor: null,
      });
    },

    resetMessages: () => {
      get().cancelQuestion();
      // Single coordinated session reset: drop the messages array AND every
      // piece of view-state derived from it, in one atomic update.
      //
      // Before this consolidation, callers had to remember three steps —
      // resetMessages(), setLiteStaticSkipBefore(0), bumpLiteScrollbackClear()
      // — and any handler that forgot one half-cleared and produced glitches:
      // /chat new kept old scrollback because it skipped the bump; the stale
      // skipBefore bookmark made new messages slice out to nothing and only
      // flash through the live region. Folding it all here means every
      // resetMessages() call (now or future) wipes correctly.
      //
      // - lite.staticSkipBefore reset to 0: the bookmark is a slice index into
      //   the messages array. With messages now empty, any non-zero value
      //   would skip the entire next session's scrollback.
      // - lite.scrollbackClearToken bumped: triggers LiteLayout's terminal
      //   wipe + cursor reset + ref clear (LiteLayout.tsx:539). Also picked
      //   up by ConversationView (TUI) for the symmetric singleton wipe so
      //   tui→lite→tui swaps don't accumulate state across modes.
      // - lite.welcomeEmitted reset: lets the next mount re-emit the banner
      //   (per-session welcome on /chat new, fresh boot, etc.).
      // - tasks cleared + activityTrayExpanded collapsed: the task list is
      //   populated by the agent's todo_list/task tool calls in the active
      //   conversation. With the conversation gone, the tray's contents
      //   would be stale — and Ctrl+X (gated on tasks.length > 0) would
      //   still toggle that stale tray, leaking the prior session's todos
      //   into the new chat.
      set((s) => ({
        messages: [],
        activeCompactionAttemptKey: null,
        compactionReportAnchor: null,
        lite: {
          ...s.lite,
          staticSkipBefore: 0,
          // Gate the cross-mode token bump to lite mode only. Modern TUI's
          // ConversationView is a consumer of lite.scrollbackClearToken
          // (added for tui→lite swap symmetry), but in main /chat new does NOT
          // wipe ConversationView's singletons. Leaving this unconditional
          // would cross-mode-leak the bump into modern TUI on /chat new and
          // force a singleton wipe that main never performed.
          ...(s.uiMode === 'lite'
            ? { scrollbackClearToken: s.lite.scrollbackClearToken + 1 }
            : {}),
          welcomeEmitted: false,
          mcpFailureWarningEmitted: false,
        },
        tasks: [],
        activityTrayExpanded: false,
      }));
    },

    setSlashCommands: (commands: SlashCommand[]) => {
      set((state) => {
        const localCommands = state.slashCommands.filter(
          (cmd) => cmd.source === 'local'
        );
        return { slashCommands: [...localCommands, ...commands] };
      });
    },

    setKasCommands: (commands: KasCommand[]) => {
      set({ kasCommands: commands });
    },

    setPrompts: (prompts) => {
      set({ prompts });
    },

    setSkills: (skills) => {
      set({ skills });
    },

    setSteering: (steering) => {
      set({ steering });
    },

    setKasAvailableAgents: (agents) => {
      set((s) => ({ kas: { ...s.kas, availableAgents: agents } }));
    },

    setActiveCommand: (command: ActiveCommand | null) => {
      set({ activeCommand: command });
    },

    setCommandInput: (value: string) => {
      set({ commandInputValue: value });
    },

    setActiveTrigger: (trigger) => {
      set({ activeTrigger: trigger });
    },

    setFilePickerHasResults: (hasResults) => {
      set({ filePickerHasResults: hasResults });
    },

    setPromptHint: (hint) => {
      set({ promptHint: hint });
    },

    setCommandShadowText: (text) => {
      set({ commandShadowText: text });
    },

    setVoiceStop: (fn) => {
      set({ voiceStop: fn });
    },

    setVoiceCancel: (fn) => {
      set({ voiceCancel: fn });
    },

    setVoiceLevel: (level) => {
      set({ voiceLevel: level });
    },

    toggleVoiceAutoSubmit: () => {
      set((s) => {
        const next = !s.voiceAutoSubmit;
        return { voiceAutoSubmit: next };
      });
    },

    incrementVoiceHint: () => {
      set((s) => {
        const next = s.voiceHintIndex + 1;
        return { voiceHintIndex: next };
      });
    },

    setVoicePartialText: (text) => {
      set({ voicePartialText: text });
    },
    setPendingVoiceText: (text) => {
      set({ pendingVoiceText: text });
    },

    clearCommandInput: () => {
      set({
        commandInputValue: '',
        activeTrigger: null,
        filePickerHasResults: false,
        promptHint: null,
        commandShadowText: null,
      });
    },

    executeCommandWithArg: async (arg: string) => {
      const { activeCommand } = get();
      if (!activeCommand) return;

      const cmdName = activeCommand.command.name.replace(/^\//, '');
      set({ activeCommand: null });

      const state = get();
      const ctx: CommandContext = buildCommandContext(state, set, get, {
        showTuiPanel: false,
        showChangelogPanel: false,
        showCodePanel: false,
        codeData: null,
      });

      applyLiteAlertRouting(ctx, state, set);

      await executeCommandWithArg(cmdName, arg, ctx);
    },

    resumeSession: async (sessionId, environment) => {
      // Resume a session chosen in the `/sessions` panel. The picker knows the
      // row's store, so route the load to it explicitly — a cloud row must not
      // be probed against local stores (not-found), and a local row picked from
      // inside a cloud session must not be sent to the remote store.
      set({
        showSessionPicker: false,
        sessionPickerRows: [],
        activeCommand: null,
      });
      const state = get();
      const ctx: CommandContext = buildCommandContext(state, set, get, {
        showTuiPanel: false,
        showChangelogPanel: false,
        showCodePanel: false,
        codeData: null,
      });
      applyLiteAlertRouting(ctx, state, set);
      if (environment) {
        await loadExistingSession(ctx, sessionId, {
          source: environment === 'cloud' ? 'remote' : 'local',
        });
      } else {
        await executeCommandWithArg('chat', sessionId, ctx);
      }
    },

    queueMessage: (content: string) => {
      const trimmed = content.trim();
      if (!trimmed) return;
      const {
        kiro,
        sessionId,
        isInitialized,
        activeInterruptMode,
        uiMode,
        isProcessing,
      } = get();

      // Lite local queue: it renders pending entries from `queuedMessages`
      // (the "(N queued)" strip) and drains them via processQueue. Keep BOTH
      // (a) known slash tokens (the slash-command queue) AND (b) any input
      // that arrives while no agent turn is active — pre-init, or a
      // `loadingMessage` window (/agent swap, /chat resume, /<cmd> options
      // fetch), all isProcessing=false. Only a genuine mid-turn chat message
      // (isProcessing) falls through to the STEER routing below, so lite
      // matches TUI steering. Routing a slash command — or a message during a
      // resume window where sessionId is stale/absent and there's no turn to
      // drain into — to steerMessage would silently lose it.
      if (
        uiMode === 'lite' &&
        (!isProcessing ||
          isKnownSlashCommandToken(trimmed, liteGateCommands(get())))
      ) {
        set((state) => ({
          queuedMessages: [...state.queuedMessages, trimmed],
        }));
        return;
      }

      // Pre-init: buffer locally on `pendingSteerContent` regardless of mode.
      // This reuses the same display slot as the backend steer queue
      // (ActivityTray, prompt placeholder, etc.) so the user sees what they
      // typed immediately. Multiple submissions concatenate with "\n\n",
      // matching the backend steer queue's format. Drained by the init path
      // in index.tsx as a fresh `sendMessage` once init completes — pre-init
      // input is semantically a first prompt, not a mid-turn steer.
      if (!isInitialized || !sessionId) {
        set((state) => ({
          pendingSteerContent:
            state.pendingSteerContent != null
              ? `${state.pendingSteerContent}\n\n${trimmed}`
              : trimmed,
          // Pre-init input exists only locally and must replay after initialization.
          _steerReplayArmed: true,
        }));
        return;
      }

      // Steering mode: send to backend via ACP
      if (activeInterruptMode === InterruptMode.STEER) {
        kiro.steerMessage(sessionId, trimmed).catch((err) => {
          logger.error('queueMessage: steerMessage failed', err);
          get().showTransientAlert({
            message: 'Failed to queue message — try again',
            status: 'error',
            autoHideMs: 3000,
          });
        });
        return;
      }

      // Queueing mode: append to local buffer.
      // No dedup: queuing the same slash command twice is allowed (the user
      // may genuinely want to re-run it). The genuine double-send guard lives
      // in processQueue (`if (isProcessing) return`), not here.
      set((state) => ({
        queuedMessages: [...state.queuedMessages, trimmed],
      }));
    },

    processQueue: async () => {
      const { cancelInProgress } = get();

      if (cancelInProgress) {
        await cancelInProgress;
      }

      const {
        isProcessing,
        isCompacting,
        loadingMessage,
        _observerQueueBlocked,
        pendingSteerContent,
        queuedMessages,
      } = get();

      // Observer auth/session failures require recovery before queued work resumes.
      if (_observerQueueBlocked) return;

      // Don't drain while the session is busy (prevents double-send races).
      if (isProcessing || isCompacting || loadingMessage) return;

      // Only a backend-dropped cancel redirect may replay as a prompt.
      if (pendingSteerContent != null && get()._steerReplayArmed) {
        const steer = pendingSteerContent;
        set({ pendingSteerContent: null, _steerReplayArmed: false });
        await get().sendMessage(
          normalizeAtPrompt(steer, selectVisibleSlashCommands(get())),
          undefined,
          steer
        );
        return; // After this turn ends, processQueue will be called again for the queue.
      }

      // Then drain the queue
      const nextMessage = queuedMessages[0];
      if (!nextMessage) return;

      // Adjust editing index since we're removing index 0
      let newEditingIndex = get().editingQueueIndex;
      if (newEditingIndex != null) {
        if (newEditingIndex === 0) {
          newEditingIndex = null;
        } else {
          newEditingIndex = newEditingIndex - 1;
        }
      }

      // Clear commandInputValue if we just exited editing mode
      const wasEditing = get().editingQueueIndex != null;
      const stoppedEditing = wasEditing && newEditingIndex == null;

      set((state) => ({
        queuedMessages: state.queuedMessages.slice(1),
        editingQueueIndex: newEditingIndex,
        commandInputValue: stoppedEditing ? '' : state.commandInputValue,
      }));
      // Lite mode queues known slash commands so they fire at turn-end
      // (handleUserInput's queue branch). Dispatch them via handleUserInput
      // so slash commands like /tui actually run rather than getting sent
      // to the agent as a chat message. Mode-check is on `nextMessage`
      // shape (slash + known) rather than current uiMode — a queued /lite
      // following a queued /tui must still dispatch as a slash command
      // even though the swap put us in TUI mode mid-drain.
      const isSlash = nextMessage.startsWith('/');
      if (
        isSlash &&
        isKnownSlashCommandToken(nextMessage, liteGateCommands(get()))
      ) {
        // Emit a scrollback marker so users have a record that a queued
        // slash command ran. Most painful for picker-opening commands
        // (/model, /agent, /effort, /theme, /chat): if the user dismisses
        // the picker with Esc, no announcement lands and scrollback
        // contains no evidence the queued command fired at all. The row
        // is dim-styled so it reads as a turn-boundary marker; lite
        // renders it inline in scrollback, modern TUI surfaces it in the
        // conversation view — both are useful.
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: crypto.randomUUID(),
              role: MessageRole.System,
              content: chalk.dim(`[queue] ${nextMessage}`),
              success: true,
            },
          ],
        }));
        // Snapshot the user's current prompt buffer before dispatching the
        // queued slash command. handleUserInput's main path clears
        // `commandInputValue` before dispatching — fine when the user just
        // hit enter, but during a queue drain the user may have typed new
        // text after queueing the command. Without the snapshot, that mid-
        // typed text disappears the moment the drain fires (P438912313).
        //
        // Three guards on the restore:
        //   1. `userTypedExtra` — the snapshot has to differ from the
        //      queued command itself; otherwise we'd put the very command
        //      we just dispatched back in the input.
        //   2. `userTypedDuringDispatch` — handleUserInput is async; for
        //      RPC-bound commands the user can type into the cleared
        //      buffer during the await. Restoring the pre-dispatch
        //      snapshot in that window would CLOBBER the new typing.
        //   3. Picker open vs. closed — when activeCommand is non-null
        //      after the await, stash the snapshot in `queuedInputRestore`
        //      and let LiteLayout's effect apply it on the picker's close
        //      transition. Inline restore in that case would be invisible
        //      while the picker is up (PromptInput renders the command
        //      name) and gets clobbered by clearCommandInput on dismiss.
        const commandSnapshot = get().commandInputValue;
        const inputSnapshot = get().input;
        const userTypedExtra = commandSnapshot.trim() !== nextMessage.trim();
        await get().handleUserInput(nextMessage);
        if (userTypedExtra) {
          const userTypedDuringDispatch = !!get().commandInputValue.trim();
          if (!userTypedDuringDispatch) {
            if (get().activeCommand != null) {
              set({
                queuedInputRestore: {
                  commandInputValue: commandSnapshot,
                  input: inputSnapshot,
                },
              });
            } else {
              set({
                commandInputValue: commandSnapshot,
                input: inputSnapshot,
              });
            }
          }
        }
        // Slash commands that don't trigger sendMessage (e.g. mode swaps,
        // /clear, panel toggles) leave isProcessing false — the next
        // queued item won't drain on its own. Re-enter processQueue
        // so the rest of the queue keeps draining in FIFO order.
        if (!get().isProcessing) {
          await get().processQueue();
        }
        return;
      }

      // Queued @prompts are stored verbatim; resolve them against the
      // prompt registry as they leave the queue, not when they entered it.
      // The typed text stays as display content so the rendered row and
      // history match what the user submitted.
      await get().sendMessage(
        normalizeAtPrompt(nextMessage, selectVisibleSlashCommands(get())),
        undefined,
        nextMessage
      );
    },

    clearQueue: () => {
      set((state) => ({
        queuedMessages: [],
        editingQueueIndex: null,
        commandInputValue:
          state.editingQueueIndex != null ? '' : state.commandInputValue,
      }));
    },

    removeQueuedMessage: (index: number) => {
      const { queuedMessages, editingQueueIndex } = get();
      if (index < 0 || index >= queuedMessages.length) return;

      let newEditingIndex = editingQueueIndex;
      if (newEditingIndex != null) {
        if (newEditingIndex === index) {
          newEditingIndex = null;
        } else if (newEditingIndex > index) {
          newEditingIndex = newEditingIndex - 1;
        }
      }

      // Clear commandInputValue if we just exited editing mode
      const wasEditing = editingQueueIndex != null;
      const stoppedEditing = wasEditing && newEditingIndex == null;

      set((state) => ({
        queuedMessages: queuedMessages.filter((_, i) => i !== index),
        editingQueueIndex: newEditingIndex,
        commandInputValue: stoppedEditing ? '' : state.commandInputValue,
      }));
    },

    replaceQueuedMessage: (index: number, content: string) => {
      const { queuedMessages } = get();
      if (index < 0 || index >= queuedMessages.length) {
        set({ editingQueueIndex: null });
        return;
      }
      const trimmed = content.trim();
      if (!trimmed) return;

      set({
        queuedMessages: queuedMessages.map((msg, i) =>
          i === index ? trimmed : msg
        ),
        editingQueueIndex: null,
      });
    },

    startEditingQueue: (index: number) => {
      const { queuedMessages } = get();
      if (index < 0 || index >= queuedMessages.length) return;
      // Load the message text into commandInputValue so PromptInput picks it up
      set({
        editingQueueIndex: index,
        commandInputValue: queuedMessages[index],
      });
    },

    cancelEditingQueue: () => {
      set({ editingQueueIndex: null, commandInputValue: '' });
    },

    setEditingQueueIndex: (index: number | null) => {
      // Mutually exclusive with steer-line editing: entering a queue edit
      // clears any active steer-row chevron, and vice versa.
      set({
        editingQueueIndex: index,
        editingSteerLineIndex:
          index == null ? get().editingSteerLineIndex : null,
      });
    },

    setEditingSteerLineIndex: (index: number | null) => {
      set({
        editingSteerLineIndex: index,
        editingQueueIndex: index == null ? get().editingQueueIndex : null,
      });
    },

    applyQueuedInputRestore: () => {
      const restore = get().queuedInputRestore;
      if (!restore) return;
      // Single set so the input buffer (lower-level lines/cursor) and
      // commandInputValue (PromptInput's syncToStore target) land in the
      // same render — otherwise PromptInput's commandInputValue effect
      // could fire mid-restore against a half-applied state and resync
      // segments to the snapshot value before lines is restored, leaving
      // a transient row where the cursor sits at column 0 of a segment
      // that says the right text. The matched-pair set keeps the
      // visible row, the cursor column, and the segments stack
      // consistent across the single render.
      set({
        commandInputValue: restore.commandInputValue,
        input: restore.input,
        queuedInputRestore: null,
      });
    },

    clearSteerMessage: (targetLine?: string) => {
      const { kiro, sessionId, pendingSteerContent, isInitialized } = get();
      if (pendingSteerContent == null) return;

      // Line-aware delete: when a specific steer row is targeted and the buffer
      // holds more than that one line, remove ONLY that line and re-stage the
      // remainder — deleting one staged steer must not discard its siblings.
      // `removeSteerLine` returns null when the target was the only line (or
      // wasn't found), which falls through to the whole-buffer clear below.
      const remainder =
        targetLine != null
          ? removeSteerLine(pendingSteerContent, targetLine)
          : null;

      if (remainder != null) {
        set({ pendingSteerContent: remainder, editingSteerLineIndex: null });
        const hasBackendQueue = isInitialized && sessionId != null;
        if (hasBackendQueue) {
          // Resync the backend to the trimmed buffer: clear then resteer the
          // remaining lines so the backend holds exactly what's displayed.
          kiro
            .clearSteering(sessionId)
            .then(() => kiro.steerMessage(sessionId, remainder))
            .catch((err) => {
              logger.error('clearSteerMessage (line) failed', err);
              get().showTransientAlert({
                message: 'Failed to update queued message',
                status: 'error',
                autoHideMs: 3000,
              });
            });
        }
        return;
      }

      // Optimistically clear locally. The backend `SteeringCleared`
      // notification (if we made a backend call) will reconfirm. If the
      // clear request fails we'll re-receive a `SteeringQueued` snapshot
      // that restores the display. Drop any steer-row edit chevron too.
      set({ pendingSteerContent: null, editingSteerLineIndex: null });

      // Pre-init clear is local-only — there's no backend queue to sync
      // with until init dispatches the buffered content as a fresh prompt
      // (see index.tsx init path). A session-live queue still needs the
      // explicit `_session/steer/clear` round-trip to keep the backend in
      // sync.
      const hasBackendQueue = isInitialized && sessionId != null;
      if (hasBackendQueue) {
        kiro.clearSteering(sessionId).catch((err) => {
          logger.error('clearSteerMessage failed', err);
          get().showTransientAlert({
            message: 'Failed to clear queued message',
            status: 'error',
            autoHideMs: 3000,
          });
        });
      }
    },

    replaceSteerMessage: (content: string, targetLine?: string) => {
      const trimmed = content.trim();
      // Empty edit reads as "discard the steer" — defer to clearSteerMessage
      // so the local-clear + backend round-trip stays in one place. Pass the
      // target line through so only that row is removed from a multi-steer.
      if (!trimmed) {
        get().clearSteerMessage(targetLine);
        return;
      }

      const { kiro, sessionId, pendingSteerContent, isInitialized } = get();
      // Nothing staged to replace — no-op rather than steering out of band.
      if (pendingSteerContent == null) return;

      // Clear-and-resteer: the steer buffer is the single source of truth for
      // steer content (NOT queuedMessages — appending there would double-send,
      // once via processQueue and once via the backend's own injection). When a
      // specific row is targeted, splice only that line of the `\n\n`-joined
      // buffer so sibling steer lines survive the edit; otherwise replace the
      // whole buffer (single-steer case). `spliceSteerLine` falls back to the
      // edited text when the target isn't found, so an edit is never dropped.
      const next =
        targetLine != null
          ? spliceSteerLine(pendingSteerContent, targetLine, trimmed)
          : trimmed;

      const hasBackendQueue = isInitialized && sessionId != null;
      if (hasBackendQueue) {
        // Optimistically reflect the edited buffer. The backend echoes
        // SteeringCleared (→ null) then SteeringQueued (→ next); landing on
        // the same value we set here, so there's no visible flicker. Sequence
        // the clear before the resteer so the backend ends with ONLY the
        // edited content rather than concatenating onto the old steer.
        set({ pendingSteerContent: next });
        kiro
          .clearSteering(sessionId)
          .then(() => kiro.steerMessage(sessionId, next))
          .catch((err) => {
            logger.error('replaceSteerMessage failed', err);
            get().showTransientAlert({
              message: 'Failed to update queued message — try again',
              status: 'error',
              autoHideMs: 3000,
            });
          });
        return;
      }

      // Pre-init: the steer buffer is a local-only first-prompt staging slot
      // (drained by the init path as a fresh sendMessage). Replace it in place
      // — there's no backend to round-trip with yet.
      set({ pendingSteerContent: next });
    },

    // Input actions

    insert: (char: string) => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;
        const newLines = [...lines];
        const line = newLines[cursorRow] ?? '';
        newLines[cursorRow] =
          line.slice(0, cursorCol) + char + line.slice(cursorCol);

        return {
          input: {
            ...state.input,
            lines: newLines,
            cursorCol: cursorCol + char.length,
            preferredCursorCol: cursorCol + char.length,
          },
        };
      });
    },
    newline: () => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;
        const newLines = [...lines];
        const currentLine = newLines[cursorRow] ?? '';
        const beforeCursor = currentLine.slice(0, cursorCol);
        const afterCursor = currentLine.slice(cursorCol);

        newLines[cursorRow] = beforeCursor;
        newLines.splice(cursorRow + 1, 0, afterCursor);

        return {
          input: {
            ...state.input,
            lines: newLines,
            cursorRow: cursorRow + 1,
            cursorCol: 0,
            preferredCursorCol: 0,
          },
        };
      });
    },
    backspace: () => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;

        if (cursorCol === 0 && cursorRow === 0) {
          return state;
        }

        const newLines = [...lines];

        if (cursorCol === 0) {
          // At start of line, merge with previous line
          const prevLine = newLines[cursorRow - 1] ?? '';
          const currentLine = newLines[cursorRow] ?? '';
          newLines[cursorRow - 1] = prevLine + currentLine;
          newLines.splice(cursorRow, 1);

          return {
            input: {
              ...state.input,
              lines: newLines,
              cursorRow: cursorRow - 1,
              cursorCol: prevLine.length,
              preferredCursorCol: prevLine.length,
            },
          };
        } else {
          // Delete character before cursor
          const line = newLines[cursorRow] ?? '';
          newLines[cursorRow] =
            line.slice(0, cursorCol - 1) + line.slice(cursorCol);

          return {
            input: {
              ...state.input,
              lines: newLines,
              cursorCol: cursorCol - 1,
              preferredCursorCol: cursorCol - 1,
            },
          };
        }
      });
    },
    delete: () => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;
        const newLines = [...lines];
        const line = newLines[cursorRow] ?? '';

        if (cursorCol < line.length) {
          // Delete character at cursor position
          newLines[cursorRow] =
            line.slice(0, cursorCol) + line.slice(cursorCol + 1);
          return { input: { ...state.input, lines: newLines } };
        } else if (cursorRow < lines.length - 1) {
          // At end of line, merge with next line
          const nextLine = newLines[cursorRow + 1] ?? '';
          newLines[cursorRow] = line + nextLine;
          newLines.splice(cursorRow + 1, 1);
          return { input: { ...state.input, lines: newLines } };
        }

        return state;
      });
    },
    clearWord: () => {
      set((state) => {
        // todo
        return state;
      });
    },
    clearLine: () => {
      set((state) => {
        // todo
        return state;
      });
    },
    clearInput: () => {
      set(() => ({
        input: initialInputBufferState(),
      }));
    },

    moveCursor: (_dir: MoveCursorDir) => {
      set((state) => {
        // todo
        return state;
      });
    },
    setViewport: (width: number, height: number) => {
      set((state) => {
        if (
          state.input.viewportWidth === width &&
          state.input.viewportHeight === height
        ) {
          return state;
        }
        return {
          input: {
            ...state.input,
            viewportWidth: width,
            viewportHeight: height,
          },
        };
      });
    },

    navigateHistory: (direction: 'up' | 'down') => {
      const history = CommandHistory.getInstance();
      const command = history.navigate(direction);
      return command;
    },

    // UI actions
    setMode: (mode) => {
      // The artifact-generation card is tied to the spec workflow.
      // Clear it on any mode change so the user doesn't see stale
      // generation state after switching to the default agent (or
      // away from spec mode in general). The open artifact-view
      // panel is left alone — the user explicitly opened it and
      // dismisses with Q.
      set((state) =>
        state.artifactGenerating === null
          ? { mode }
          : { mode, artifactGenerating: null }
      );
    },
    setUiMode: (uiMode: UiMode, notice?: string) => {
      // Both directions CLEAR scrollback and re-render the full conversation
      // in the destination mode's form. Symmetric: the user sees their entire
      // session styled consistently for whichever mode they're in, with the
      // current verbosity and theme applied uniformly. No half-and-half
      // (some turns in TUI Card chrome, others with lite `You:`/`<agent>:`
      // headers).
      //
      // Mechanism: bump lite.scrollbackClearToken. LiteLayout and
      // ConversationView both subscribe to it; on bump each wipes its
      // module-level singletons, writes \x1b[3J\x1b[H\x1b[2J (twinki's
      // stdout interceptor catches that and drops accumulatedStaticOutput
      // too), and resets twinki's monotonic cursor. The destination
      // renderer then paints from messages[] from scratch.
      //
      // skipBefore is reset to 0 in both directions so the destination
      // renderer paints every message in the array (the cap on resume is
      // re-applied by loadSession after replay if needed).
      //
      // Same-mode noop: dispatching setUiMode('lite') while already in
      // lite must NOT bump the clear token — that would wipe the user's
      // scrollback for a no-op call (e.g., a stale dispatch).
      set((state) => {
        if (state.uiMode === uiMode) return state;
        return {
          uiMode,
          ...(notice
            ? {
                messages: [
                  ...state.messages,
                  {
                    id: generateMessageId(),
                    role: MessageRole.System,
                    content: notice,
                    success: true,
                  },
                ],
              }
            : {}),
          lite: {
            ...state.lite,
            staticSkipBefore: 0,
            scrollbackClearToken: state.lite.scrollbackClearToken + 1,
          },
        };
      });
    },
    setLiteStaticSkipBefore: (idx: number) =>
      set((s) => ({
        lite: { ...s.lite, staticSkipBefore: Math.max(0, idx) },
      })),

    addSubagentSession: (info) => {
      set((state) => {
        const newSessions = new Map(state.sessions);
        // Convert SubagentInfo to AgentSession format
        const session: AgentSession = {
          id: info.sessionId,
          name: info.agentName || info.sessionId,
          role: '',
          status: info.status === 'working' ? 'busy' : 'idle',
          type: 'ephemeral',
          created: new Date(),
          lastActivity: new Date(),
        };
        newSessions.set(info.sessionId, session);
        return { sessions: newSessions };
      });
    },

    updateSubagentSession: (sessionId, status) => {
      set((state) => {
        const newSessions = new Map(state.sessions);
        const existing = newSessions.get(sessionId);
        if (existing) {
          const agentStatus = status === 'working' ? 'busy' : 'idle';
          newSessions.set(sessionId, {
            ...existing,
            status: agentStatus,
            lastActivity: new Date(),
          });
        }
        return { sessions: newSessions };
      });
    },

    pushSessionEvent: (sessionId, event) => {
      set((state) => {
        const newBuffer = { ...state.sessionEventBuffer };
        newBuffer[sessionId] = [...(newBuffer[sessionId] ?? []), event];
        return { sessionEventBuffer: newBuffer };
      });
    },

    addSession: (session) =>
      set((state) => {
        const newSessions = new Map(state.sessions);
        const staleIds: string[] = [];
        // Clear old terminated sessions when a new active session arrives
        if (session.status === 'busy' && !newSessions.has(session.id)) {
          for (const [id, s] of newSessions) {
            if (s.status === 'terminated') {
              staleIds.push(id);
              newSessions.delete(id);
            }
          }
        }
        newSessions.set(session.id, session);
        // Backfill the placeholder names stamped by the ToolCall handler: when
        // a stage's first tool call beat this subagent_list_update, its rows
        // hold the raw sessionId. Now that we have the real name, rewrite them
        // so isInnerSubagentTool keeps treating them as inner and the stage's
        // summary block resolves.
        const sessionName = session.name;
        const placeholderId = session.id;
        const needsBackfill =
          sessionName &&
          sessionName !== placeholderId &&
          state.messages.some(
            (m) =>
              m.role === MessageRole.ToolUse && m.agentName === placeholderId
          );
        const backfilledMessages = needsBackfill
          ? state.messages.map((m) =>
              m.role === MessageRole.ToolUse && m.agentName === placeholderId
                ? { ...m, agentName: sessionName }
                : m
            )
          : state.messages;
        if (staleIds.length === 0) {
          return needsBackfill
            ? { sessions: newSessions, messages: backfilledMessages }
            : { sessions: newSessions };
        }
        // Also clear stale messages and event buffers
        const staleNames = new Set(
          staleIds.map((id) => state.sessions.get(id)?.name).filter(Boolean)
        );
        const newBuffer = { ...state.sessionEventBuffer };
        for (const id of staleIds) {
          delete newBuffer[id];
        }
        return {
          sessions: newSessions,
          sessionEventBuffer: newBuffer,
          messages: backfilledMessages.filter(
            (msg) =>
              msg.role !== MessageRole.ToolUse ||
              !msg.agentName ||
              !staleNames.has(msg.agentName)
          ),
        };
      }),

    updateSession: (id, updates) =>
      set((state) => {
        const newSessions = new Map(state.sessions);
        const existing = newSessions.get(id);
        if (existing) {
          newSessions.set(id, { ...existing, ...updates });
        }
        return { sessions: newSessions };
      }),

    removeSession: (id) =>
      set((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.delete(id);
        // Clean up event buffer for terminated session
        const newBuffer = { ...state.sessionEventBuffer };
        delete newBuffer[id];
        return {
          sessions: newSessions,
          sessionEventBuffer: newBuffer,
          activeSessionId:
            state.activeSessionId === id ? '' : state.activeSessionId,
          selectedSessionId:
            state.selectedSessionId === id
              ? undefined
              : state.selectedSessionId,
        };
      }),

    cleanupTerminatedSession: (sessionId) => {
      const { approvalQueue, pendingApproval, questionQueue, pendingQuestion } =
        get();
      // Cancel pending approvals for this session
      const sessionApprovals = approvalQueue.filter(
        (a) => a.sessionId === sessionId
      );
      for (const a of sessionApprovals) {
        a.resolve({ outcome: 'cancelled' });
      }
      const sessionQuestions = questionQueue.filter(
        (question) => question.sessionId === sessionId
      );
      const remainingQuestions = questionQueue.filter(
        (question) => question.sessionId !== sessionId
      );
      for (const question of sessionQuestions) {
        question.resolve({ action: 'dismissed' });
      }
      // Find the agent name for this session to mark its tool calls finished
      const session = get().sessions.get(sessionId);
      const agentName = session?.name;
      set((state) => ({
        approvalQueue:
          sessionApprovals.length > 0
            ? state.approvalQueue.filter((a) => a.sessionId !== sessionId)
            : state.approvalQueue,
        pendingApproval:
          pendingApproval?.sessionId === sessionId
            ? null
            : state.pendingApproval,
        questionQueue:
          sessionQuestions.length > 0
            ? remainingQuestions
            : state.questionQueue,
        pendingQuestion:
          pendingQuestion?.sessionId === sessionId
            ? (remainingQuestions[0] ?? null)
            : state.pendingQuestion,
        messages: agentName
          ? state.messages.map((msg) =>
              msg.role === MessageRole.ToolUse &&
              msg.agentName === agentName &&
              !msg.isFinished
                ? { ...msg, isFinished: true }
                : msg
            )
          : state.messages,
      }));
    },

    terminateAllCrewSessions: async () => {
      const { sessions, kiro } = get();
      const sessionIds = Array.from(sessions.keys());
      // Terminate each session on the backend, but keep data in store
      await Promise.all(
        sessionIds.map((id) => kiro?.terminateSession(id).catch(() => {}))
      );
      // Mark all sessions as terminated instead of clearing
      set((state) => {
        const newSessions = new Map(state.sessions);
        for (const [id, session] of newSessions) {
          if (session.status !== 'terminated') {
            newSessions.set(id, { ...session, status: 'terminated' as const });
          }
        }
        return { sessions: newSessions };
      });
    },

    setActiveSession: (id) => set({ activeSessionId: id }),

    setSelectedSession: (id) => set({ selectedSessionId: id }),

    toggleCrewMonitor: () =>
      set((state) => ({ crewMonitorVisible: !state.crewMonitorVisible })),

    incrementExitSequence: () => {
      set((state) => {
        if (state.exitTimer) {
          clearTimeout(state.exitTimer);
        }
        if (state.suspendTimer) {
          clearTimeout(state.suspendTimer);
        }

        const newSequence = state.exitSequence + 1;

        if (newSequence >= 2) {
          // Clean up kiro and renderer before exiting
          state.kiro.close();
          state.onExit?.();
          process.exit(0);
        }

        const timer = setTimeout(() => {
          set({ exitSequence: 0, exitTimer: null });
        }, 2000);

        return {
          exitSequence: newSequence,
          exitTimer: timer,
          suspendArmed: false,
          suspendTimer: null,
        };
      });
    },

    resetExitSequence: () => {
      set((state) => {
        if (state.exitTimer) {
          clearTimeout(state.exitTimer);
        }
        return { exitSequence: 0, exitTimer: null };
      });
    },

    armSuspend: () => {
      set((state) => {
        if (state.suspendTimer) {
          clearTimeout(state.suspendTimer);
        }
        if (state.exitTimer) {
          clearTimeout(state.exitTimer);
        }

        const timer = setTimeout(() => {
          set({ suspendArmed: false, suspendTimer: null });
        }, 2000);

        return {
          suspendArmed: true,
          suspendTimer: timer,
          exitSequence: 0,
          exitTimer: null,
        };
      });
    },

    disarmSuspend: () => {
      set((state) => {
        if (state.suspendTimer) {
          clearTimeout(state.suspendTimer);
        }
        return { suspendArmed: false, suspendTimer: null };
      });
    },

    showTransientAlert: (alert) => {
      set({ transientAlert: alert });
    },

    dismissTransientAlert: () => {
      set({ transientAlert: null });
    },

    setRetryStatus: (status) => {
      set({ retryStatus: status });
    },

    setLoadingMessage: (message) => {
      const prev = get().loadingMessage;
      set({ loadingMessage: message });
      // The "loading window" (e.g. /agent swap, /chat resume, /<cmd> options
      // fetch) accepts queued slash commands via handleUserInput while it's
      // open — but no other path drains the queue when it closes. Without
      // this drain, anything the user queued during the swap stays stuck
      // until the next sendMessage/cancelMessage cycle. Fire-and-forget the
      // drain on the close transition so the queued command runs immediately
      // after the swap settles. processQueue() no-ops if isProcessing is
      // still true, so this is safe even when an agent stream is interleaved
      // with the loading window.
      if (prev != null && message == null) {
        void get().processQueue();
      }
    },

    // Context usage actions
    applySessionRosterDelta: (delta) => {
      const roster = mergeRosterDelta(get().sessionRoster, delta);
      const active = deriveActiveSessionStatus(roster, get().sessionId);
      set({
        sessionRoster: roster,
        cloudSessionStatus: active?.status ?? null,
        cloudProvisioningFailure: active?.provisioningFailure ?? null,
      });
    },
    setCloudRepo: (cloudRepo) => set({ cloudRepo }),
    applyRepoFooter: (repos, branch) =>
      set({
        cloudRepo: repos[0] ?? null,
        cloudExtraRepos: Math.max(0, repos.length - 1),
        attachedRepos: [...repos],
        ...(repos.length === 0
          ? { cloudBranch: null }
          : branch !== undefined
            ? { cloudBranch: branch }
            : {}),
      }),
    applySessionRepositories: (repositories) => {
      if (!get().cloudSessionActive) return;
      get().applyRepoFooter(
        repositories.map((r) => r.name),
        repositories[0]?.branch ?? null
      );
    },
    resetCloudSessionScope: () =>
      set({
        cloudRepo: null,
        cloudBranch: null,
        cloudExtraRepos: 0,
        attachedRepos: [],
      }),
    stashCloudSessionScope: (sessionId) => {
      if (!sessionId) return;
      const { cloudRepo, cloudBranch, cloudExtraRepos, attachedRepos } = get();
      // Nothing bound → drop any stale stash so a restore can't resurrect it.
      const next = new Map(get().cloudScopeBySession);
      next.delete(sessionId);
      if (cloudRepo || attachedRepos.length > 0) {
        next.set(sessionId, {
          cloudRepo,
          cloudBranch,
          cloudExtraRepos,
          attachedRepos: [...attachedRepos],
        });
      }
      // Bound the LRU (Map preserves insertion order) so a long-lived process
      // switching through many sessions can't grow it without limit.
      const MAX_STASH = 20;
      while (next.size > MAX_STASH) {
        next.delete(next.keys().next().value as string);
      }
      set({ cloudScopeBySession: next });
    },
    restoreCloudSessionScope: (sessionId) => {
      const stashed = sessionId
        ? get().cloudScopeBySession.get(sessionId)
        : undefined;
      if (!stashed) return false;
      set({
        cloudRepo: stashed.cloudRepo,
        cloudBranch: stashed.cloudBranch,
        cloudExtraRepos: stashed.cloudExtraRepos,
        attachedRepos: [...stashed.attachedRepos],
      });
      return true;
    },
    setCloudBranch: (cloudBranch) => set({ cloudBranch }),
    setCloudProvider: (cloudProvider) => set({ cloudProvider }),
    setCloudRepoCount: (cloudRepoCount) => set({ cloudRepoCount }),
    setCloudNewSessionChecklist: (cloudNewSessionChecklist) =>
      set({ cloudNewSessionChecklist }),
    setCloudExtraRepos: (cloudExtraRepos) => set({ cloudExtraRepos }),
    setContextUsage: (percent) => {
      set((state) => {
        const lastUserIdx = state.messages.findLastIndex(
          (m) => m.role === MessageRole.User
        );
        if (lastUserIdx >= 0) {
          const msg = state.messages[lastUserIdx]!;
          if (msg.role === MessageRole.User) {
            const messages = [...state.messages];
            messages[lastUserIdx] = { ...msg, contextPercent: percent };
            return { contextUsagePercent: percent, messages };
          }
        }
        return { contextUsagePercent: percent };
      });
    },

    setKasMessageId: (kasMessageId) => {
      set((state) => {
        const lastUserIdx = state.messages.findLastIndex(
          (m) => m.role === MessageRole.User
        );
        if (lastUserIdx >= 0) {
          const msg = state.messages[lastUserIdx]!;
          if (msg.role === MessageRole.User) {
            const messages = [...state.messages];
            messages[lastUserIdx] = { ...msg, kasMessageId };
            return { messages };
          }
        }
        return {};
      });
    },

    setLastTurnTokens: (tokens) => {
      set({ lastTurnTokens: tokens });
    },

    toggleContextBreakdown: () => {
      set((state) => ({ showContextBreakdown: !state.showContextBreakdown }));
    },

    setShowContextBreakdown: (show, breakdown) => {
      set({ showContextBreakdown: show, contextBreakdown: breakdown ?? null });
    },

    setShowTuiPanel: (show) => {
      set({ showTuiPanel: show });
    },

    setShowChangelogPanel: (show) => {
      set({ showChangelogPanel: show });
    },

    setShowHelpPanel: (show, commands = []) => {
      set({ showHelpPanel: show, helpCommands: commands });
    },

    setShowUsagePanel: (show, data) => {
      set({ showUsagePanel: show, usageData: data ?? null });
    },

    setShowRewindExplorer: (show, rows) => {
      set({ showRewindExplorer: show, rewindRows: rows ?? [] });
    },
    setShowTangentExplorer: (show, rows) => {
      set({ showTangentExplorer: show, tangentRows: rows ?? [] });
    },
    setTangentName: (name) => {
      set({ tangentName: name });
    },

    setUpgradeDiagnostics: (rows, description) => {
      set({
        upgradeAnalysisRows: rows,
        upgradeAnalysisDescription: description,
      });
    },

    setUpgradeRunPreview: (preview) => {
      set({ upgradeRunPreview: preview });
    },

    setShowMcpPanel: (
      show,
      servers = [],
      mode = 'list',
      registryServers = []
    ) => {
      set({
        showMcpPanel: show,
        mcpServers: servers,
        mcpMode: mode,
        mcpRegistryServers: registryServers,
      });
    },
    resetClientDisplayCaches: () => {
      set({
        contextBreakdownCache: null,
        mcpServerCache: [],
        mcpRegistryCache: [],
        toolsList: [],
        hooksList: [],
      });
    },

    setShowToolsPanel: (show, tools) => {
      // Only replace the cached list when tools are explicitly provided.
      // Closing the panel (no `tools` arg) must NOT wipe `toolsList`: under
      // KAS the cache is the source of truth between `_kiro/tools/didChange`
      // pushes, and KAS won't re-push an unchanged set (diff-before-emit), so
      // clearing here would leave `/tools` empty until the set next changes.
      set(
        tools !== undefined
          ? { showToolsPanel: show, toolsList: tools }
          : { showToolsPanel: show }
      );
    },
    setToolsList: (tools) => {
      set({ toolsList: tools });
    },
    updateMcpServerStatuses: (servers) => {
      const { mcpServers, showMcpPanel } = get();
      if (!showMcpPanel || mcpServers.length === 0) return;
      const updated = mcpServers.map((existing) => {
        const live = servers.find((s) => s.name === existing.name);
        if (!live) return existing;
        return {
          ...existing,
          status: live.status as McpServerInfo['status'],
          toolCount: live.toolCount,
        };
      });
      set({ mcpServers: updated });
    },
    setShowGoalPanel: (show) => {
      set({ showGoalPanel: show });
    },
    setShowStatsPanel: (show, stats = [], summary = null) => {
      set({ showStatsPanel: show, statsList: stats, statsSummary: summary });
    },

    setShowHooksPanel: (show, hooks) => {
      set(
        hooks !== undefined
          ? { showHooksPanel: show, hooksList: hooks }
          : { showHooksPanel: show }
      );
    },

    setShowRepoPicker: (show, resources = []) => {
      // Providers can list the same repo name more than once; selection is
      // name-keyed, so duplicates would check/toggle together. First wins.
      const deduped = dedupeRepoResources(resources);
      set({ showRepoPicker: show, repoPickerResources: deduped });
      if (show) {
        recordTuiCloudRepoAttach({ event: 'opened' });
        // Zero-cost branch resolution: the picker already fetched these
        // resources (each carries an optional defaultBranch), so if the
        // session-bound repo is among them, light up the footer's branch
        // segment without any extra RPC.
        const boundRepo = get().cloudRepo?.trim();
        if (boundRepo && !get().cloudBranch) {
          const match = deduped.find((r) => r.name === boundRepo);
          if (match?.defaultBranch) set({ cloudBranch: match.defaultBranch });
        }
      }
    },

    submitRepoPicker: async (selected) => {
      const previous = get().attachedRepos;
      // Snapshot the pre-attach footer so a failed clone turn can roll it
      // back — the footer must reflect what is actually in the sandbox.
      const rollback: Partial<AppState> = {
        cloudRepo: get().cloudRepo,
        cloudBranch: get().cloudBranch,
        cloudExtraRepos: get().cloudExtraRepos,
        attachedRepos: previous,
      };
      const firstBranch = selected[0]
        ? get().repoPickerResources.find((r) => r.name === selected[0])
            ?.defaultBranch
        : undefined;
      // The resources are cleared below; keep a branch lookup in case a
      // partial failure promotes a different repo to the footer's primary.
      const branchByName = new Map(
        get().repoPickerResources.map((r) => [r.name, r.defaultBranch])
      );
      set({ showRepoPicker: false, repoPickerResources: [] });
      get().applyRepoFooter(selected);
      if (firstBranch) set({ cloudBranch: firstBranch });
      recordTuiCloudRepoAttach({
        event: 'submitted',
        repoCount: selected.length,
      });
      // One turn settles the workspace to the selected set: clone the newly
      // checked, remove the unchecked. Unchanged selection fires no turn.
      const added = selected.filter((repo) => !previous.includes(repo));
      const removed = previous.filter((repo) => !selected.includes(repo));
      const instruction = formatRepoChangeInstruction(added, removed);
      if (!instruction) return;
      const preTurnMessageCount = get().messages.length;
      // sendMessage silently queues (early-returns) instead of firing when a
      // turn is already in flight; a queued instruction leaves the inspected
      // slice empty, so a stale error flag could force a spurious rollback of a
      // footer whose clone is still pending. Only judge the outcome when the
      // turn actually fired (mirrors sendMessage's own fire condition).
      const turnFired =
        get().isInitialized &&
        !get().isProcessing &&
        !get().isCompacting &&
        !get().loadingMessage;
      await get().sendMessage(instruction);
      if (!turnFired) return;
      // Roll back only on evidence the work didn't happen: no tool in the
      // turn succeeded AND something failed. Error flags alone are unreliable
      // — a phantom session error can land during a turn whose clones all
      // succeeded.
      const turnTools = get()
        .messages.slice(preTurnMessageCount)
        .filter(
          (m) => m.role === MessageRole.ToolUse && !m.isSubagentTool
        ) as Extract<MessageType, { role: MessageRole.ToolUse }>[];
      const anyToolSucceeded = turnTools.some(
        (m) => m.result?.status === 'success'
      );
      const anyToolFailed = turnTools.some((m) => m.result?.status === 'error');
      if (!anyToolSucceeded && (get().lastTurnErrored || anyToolFailed)) {
        logger.warn('[repo-attach] rolling back footer', {
          lastTurnErrored: get().lastTurnErrored,
          toolCount: turnTools.length,
          anyToolFailed,
        });
        set(rollback);
        get().showTransientAlert({
          message:
            "Repository change didn't complete — the footer reflects the previous state. Check the turn output and retry /repo.",
          status: 'warning',
          autoHideMs: 8000,
        });
        return;
      }
      if (!anyToolFailed) return;
      // Mixed outcome: some tools succeeded, some failed. Settle each
      // requested repo against the tools that mention it so a failed clone
      // never stays in the footer behind an unrelated success.
      const reconciled = reconcileRepoSelection(
        selected,
        added,
        removed,
        turnTools.map((m) => ({
          content: m.content,
          status:
            m.result?.status === 'error'
              ? ('error' as const)
              : m.result?.status === 'success'
                ? ('success' as const)
                : undefined,
        }))
      );
      if (reconciled === selected) return;
      logger.warn('[repo-attach] reconciling footer after partial failure', {
        selected,
        reconciled,
      });
      get().applyRepoFooter(reconciled);
      const primary = reconciled[0];
      if (primary && primary !== selected[0]) {
        set({
          cloudBranch:
            primary === rollback.cloudRepo
              ? (rollback.cloudBranch ?? null)
              : (branchByName.get(primary) ?? null),
        });
      }
      get().showTransientAlert({
        message:
          "Some repository changes didn't complete — the footer reflects what's in the sandbox. Check the turn output and retry /repo.",
        status: 'warning',
        autoHideMs: 8000,
      });
    },

    setCloudProviderChecked: (cloudProviderChecked) =>
      set({ cloudProviderChecked }),
    setShowSourceProviderGate: (show, setupUrl = null) => {
      set({
        showSourceProviderGate: show,
        sourceProviderSetupUrl: show ? setupUrl : null,
      });
    },

    retrySourceProviderConnection: async () => {
      const kiro = get().kiro;
      if (!kiro) return false;
      try {
        const list = await kiro.getRepoProviderSource().listSourceProviders();
        const conn = resolveSourceProviderConnection(list);
        if (conn.connected) {
          set({ showSourceProviderGate: false, sourceProviderSetupUrl: null });
          return true;
        }
        // Still not connected: refresh the setup URL and keep the gate up.
        set({ sourceProviderSetupUrl: conn.setupUrl ?? null });
        return false;
      } catch {
        return false;
      }
    },

    setShowSessionPicker: (show, rows = [], invokedAs) => {
      set({
        showSessionPicker: show,
        sessionPickerRows: show ? rows : [],
        sessionPickerTitle: show ? (invokedAs ?? '/sessions') : '/sessions',
      });
    },

    setShowKeybindingsPanel: (show) => {
      set({ showKeybindingsPanel: show });
    },

    setShowDisplaySettingsPanel: (show) => {
      set({ showDisplaySettingsPanel: show });
    },

    setShowThemePanel: (show) => {
      set({ showThemePanel: show });
    },
    setShowCloudQuitPrompt: (show) => {
      set({ showCloudQuitPrompt: show });
    },

    setShowSettingsPanel: (show) => {
      set({ showSettingsPanel: show });
    },

    setCloudSessionActive: (cloudSessionActive) => {
      set({ cloudSessionActive });
    },

    setTerminalTitleEnabled: (enabled) => {
      set({ terminalTitleEnabled: enabled });
    },

    setSettingsReturnOnEscape: (value) => {
      set({ settingsReturnOnEscape: value });
    },

    setVerboseReturnOnEscape: (route) => {
      set({ verboseReturnOnEscape: route });
    },

    setThemeReturnOnEscape: (route) => {
      set({ themeReturnOnEscape: route });
    },

    /**
     * Re-open the top-level /settings panel. Used by ESC handlers when a
     * /settings-derived overlay is dismissed: we go back one level rather
     * than close everything.
     */
    reopenSettingsMenu: () => {
      // Both modes reopen the shared SettingsPanel — an ESC-back from a
      // /settings-derived sub-overlay (display/theme/keybindings/verbosity)
      // returns to the same panel the user opened. Lite renders the panel via
      // <BackendPanels>; its lite-only `verbosity` row is gated inside the
      // panel's model on uiMode, so it reappears with the same visibility.
      set({ showSettingsPanel: true });
    },

    setShowKnowledgePanel: (show, entries = [], status) => {
      set({
        showKnowledgePanel: show,
        knowledgeEntries: entries,
        knowledgeStatus: status ?? null,
      });
    },

    setShowCodePanel: (show, data) => {
      set({
        showCodePanel: show,
        codeData: data ?? null,
        ...(data?.status === 'initialized'
          ? { codeIntelligenceActive: true }
          : {}),
      });
    },

    // ── Spec artifact view actions ───────────────────────────
    notifyArtifactGenerationWrite: ({ path, featureName, artifact }) => {
      const now = Date.now();
      // We render at most one generation card at a time. When a new
      // write comes in for a different path, drop any prior entry —
      // the user only ever sees the most-recently written artifact
      // until the agent moves on.
      //
      // Same-path writes refresh the existing entry (which preserves
      // the last good summary while parsing continues).
      set((state) => {
        const existing =
          state.artifactGenerating?.absolutePath === path
            ? state.artifactGenerating
            : null;
        return {
          artifactGenerating: existing
            ? {
                ...existing,
                lastWriteTs: now,
                // A new write resets `complete` so the card returns to
                // its live state if the agent issues another write
                // after the 2 s idle timeout.
                complete: false,
              }
            : {
                absolutePath: path,
                featureName,
                artifact,
                summary: null,
                lastWriteTs: now,
                complete: false,
                parseError: null,
              },
        };
      });

      // Background load — never blocks the caller. Errors are surfaced
      // as `parseError` on the entry so the card can show a non-blocking
      // indicator while retaining the last good summary.
      void loadArtifactSummary(process.cwd(), featureName, artifact)
        .then((res) => {
          set((state) => {
            const entry = state.artifactGenerating;
            // Entry could have been cleared by clearArtifactViewOnEngineSwitch
            // while the load was in flight, or replaced by a write to a
            // different artifact. Drop the result silently.
            if (!entry || entry.absolutePath !== path) return state;

            if (res.ok) {
              return {
                artifactGenerating: {
                  ...entry,
                  summary: res.summary,
                  parseError: null,
                },
              };
            }

            const message = describeLoadError(res.error);
            return {
              artifactGenerating: { ...entry, parseError: message },
            };
          });
        })
        .catch((err: unknown) => {
          // The loader is contracted to never throw; this is a
          // last-resort guard so a stray rejection doesn't crash
          // the agent stream listener.
          logger.error('[artifact-view] load threw', {
            path,
            err: err instanceof Error ? err.message : String(err),
          });
        });
    },

    markArtifactGenerationComplete: (path: string) => {
      set((state) => {
        const entry = state.artifactGenerating;
        if (!entry || entry.absolutePath !== path || entry.complete) {
          return state;
        }
        return {
          artifactGenerating: { ...entry, complete: true },
        };
      });
    },

    reparseArtifactGeneration: (path: string) => {
      // Read the entry once to recover the (featureName, artifact) tuple
      // without making the caller pass them in. If the active entry is
      // for a different path (the agent moved on) or was cleared
      // entirely (engine switch, panel teardown), bail out.
      const entry = get().artifactGenerating;
      if (!entry || entry.absolutePath !== path) return;
      const { featureName, artifact } = entry;

      void loadArtifactSummary(process.cwd(), featureName, artifact)
        .then((res) => {
          set((state) => {
            const current = state.artifactGenerating;
            // Re-check: another action could have cleared or replaced
            // the entry while we were waiting on the read.
            if (!current || current.absolutePath !== path) return state;

            if (res.ok) {
              return {
                artifactGenerating: {
                  ...current,
                  summary: res.summary,
                  // Clear any mid-stream parse error: the post-flush
                  // parse is authoritative.
                  parseError: null,
                },
              };
            }

            // Failed parse on a fully-flushed file — surface the error
            // but keep whatever last-good summary the entry already has.
            return {
              artifactGenerating: {
                ...current,
                parseError: describeLoadError(res.error),
              },
            };
          });
        })
        .catch((err: unknown) => {
          logger.error('[artifact-view] reparse threw', {
            path,
            err: err instanceof Error ? err.message : String(err),
          });
        });
    },

    openArtifactView: async (featureName: string, artifact: ArtifactKind) => {
      const workspaceRoot = process.cwd();
      const workflow = loadSpecConfig(workspaceRoot, featureName);
      const result = await loadArtifactSummary(
        workspaceRoot,
        featureName,
        artifact
      );
      if (!result.ok) {
        // Surface a transient alert + open the panel in error mode so
        // the user can read the message and dismiss with `q`.
        set({
          artifactViewOpen: {
            featureName,
            artifact,
            summary: emptySummaryFor(artifact),
            mode: 'summary',
            cursor: 0,
            expanded: {},
            error: { message: describeLoadError(result.error) },
            workflow,
          },
        });
        return;
      }
      set({
        artifactViewOpen: {
          featureName,
          artifact,
          summary: result.summary,
          mode: 'summary',
          cursor: 0,
          expanded: {},
          error: null,
          workflow,
        },
      });
    },

    closeArtifactView: () => {
      set({ artifactViewOpen: null });
    },

    moveArtifactCursor: (direction: 'prev' | 'next') => {
      set((state) => {
        const open = state.artifactViewOpen;
        if (!open || open.mode !== 'summary') return state;
        const count = countArtifactItems(open.summary);
        if (count === 0) return state;
        const cursor =
          direction === 'next'
            ? (open.cursor + 1) % count
            : // Wrap around: -1 mod n becomes n-1
              (open.cursor - 1 + count) % count;
        return { artifactViewOpen: { ...open, cursor } };
      });
    },

    toggleArtifactExpand: (index: number) => {
      set((state) => {
        const open = state.artifactViewOpen;
        if (!open) return state;
        const next = { ...open.expanded };
        next[index] = !next[index];
        return { artifactViewOpen: { ...open, expanded: next } };
      });
    },

    enterArtifactDetail: () => {
      set((state) => {
        const open = state.artifactViewOpen;
        if (!open || open.mode !== 'summary') return state;
        // Guard: don't enter detail if there are no items to drill into.
        if (countArtifactItems(open.summary) === 0) return state;
        return {
          artifactViewOpen: { ...open, mode: 'detail' },
        };
      });
    },

    leaveArtifactDetail: () => {
      set((state) => {
        const open = state.artifactViewOpen;
        if (!open || open.mode !== 'detail') return state;
        // Spec: pressing Escape returns cursor to the item that was open
        // in detail. If the item is no longer valid (eg. the file shrank
        // externally), clamp to first.
        const count = countArtifactItems(open.summary);
        const cursor = count === 0 ? 0 : Math.min(open.cursor, count - 1);
        return {
          artifactViewOpen: { ...open, mode: 'summary', cursor },
        };
      });
    },

    clearArtifactViewOnEngineSwitch: () => {
      set({ artifactGenerating: null, artifactViewOpen: null });
    },
    // ── End spec artifact view actions ───────────────────────

    // File attachment actions
    attachFile: (path) => {
      set((state) => ({
        attachedFiles: state.attachedFiles.includes(path)
          ? state.attachedFiles
          : [...state.attachedFiles, path],
      }));
    },

    removeAttachedFile: (path) => {
      set((state) => ({
        attachedFiles: state.attachedFiles.filter((f) => f !== path),
      }));
    },

    clearAttachedFiles: () => {
      set({ attachedFiles: [] });
    },

    registerUserColorsSetter: (setter) => {
      set({ _userColorsSetter: setter });
    },

    registerBaseThemeSetter: (setter) => {
      set({ _baseThemeSetter: setter });
    },

    registerThemeDiffHexGetter: (getter) => {
      set({ _themeDiffHexGetter: getter });
    },

    registerAutoPreviewGetter: (getter) => {
      set({ _autoPreviewGetter: getter });
    },

    setThemePreview: (preview) => {
      set({ themePreview: preview });
    },

    setPendingFileAttachment: (path, triggerPosition = 0) => {
      set({ pendingFileAttachment: path ? { path, triggerPosition } : null });
    },

    consumePendingFileAttachment: () => {
      const pending = get().pendingFileAttachment;
      set({ pendingFileAttachment: null });
      return pending;
    },

    addPendingImage: (image) => {
      set((state) => ({ pendingImages: [...state.pendingImages, image] }));
    },

    removePendingImage: (index) => {
      set((state) => ({
        pendingImages: state.pendingImages.filter((_, i) => i !== index),
      }));
    },

    clearPendingImages: () => {
      set({ pendingImages: [] });
    },

    toggleToolOutputsExpanded: () => {
      set((state) => ({ toolOutputsExpanded: !state.toolOutputsExpanded }));
    },

    setTasks: (tasks: TaskItem[]) => {
      const prevTasks = get().tasks;
      set({ tasks });

      // Trigger implement-plan survey when all tasks are done.
      // Detection: either all tasks have status 'completed', OR tasks were
      // cleared (set to empty) after previously having pending items — the
      // agent removes tasks from the list once they're done.
      const hadPending =
        prevTasks.length > 0 && prevTasks.some((t) => t.status !== 'completed');
      const allDone =
        (tasks.length > 0 && tasks.every((t) => t.status === 'completed')) ||
        (tasks.length === 0 && prevTasks.length > 0);

      if (allDone && hadPending) {
        queueMicrotask(() => get().triggerImplementPlanSurvey());
      }
    },

    toggleActivityTray: () => {
      set((state) => ({
        activityTrayExpanded: !state.activityTrayExpanded,
        // Clear editing state when collapsing
        editingQueueIndex: !state.activityTrayExpanded
          ? state.editingQueueIndex
          : null,
      }));
    },

    // Dual-mode interrupt behavior toggle
    toggleInterruptMode: () => {
      const switchingToQueue =
        get().activeInterruptMode === InterruptMode.STEER;
      const newMode = switchingToQueue
        ? InterruptMode.QUEUE
        : InterruptMode.STEER;
      set({ activeInterruptMode: newMode });
      get().showTransientAlert({
        message: switchingToQueue
          ? 'Switched to Queue mode'
          : 'Switched to Steer mode',
        status: 'info',
        autoHideMs: 3000,
      });
    },

    setActiveInterruptMode: (mode: InterruptMode) => {
      set({ activeInterruptMode: mode });
    },

    setAnnouncement: (msg) => {
      set({ announcement: msg });
    },

    toggleAnnouncementExpanded: () => {
      set((state) => ({ announcementExpanded: !state.announcementExpanded }));
    },

    setHasExpandableToolOutputs: (has: boolean) => {
      set({ hasExpandableToolOutputs: has });
    },

    confirmTrustAllTools: () => {
      set({ trustAllToolsConfirmed: true });
    },

    recordCompletedTurn: () => {
      const state = get();
      const nextCount = state.completedTurnCount + 1;
      set({ completedTurnCount: nextCount });

      // Only evaluate the session-feedback survey trigger once we reach the
      // threshold. shouldShowSurvey() checks eligibility + cooldown.
      const surveyState = state.surveyState;
      if (
        nextCount < DEFAULT_TURN_THRESHOLD ||
        state.showSurveyPanel ||
        !shouldShowSurvey(SESSION_FEEDBACK_SURVEY, surveyState)
      ) {
        return;
      }

      // Don't interrupt in-flight work or other attention-grabbing UI.
      if (
        state.isProcessing ||
        state.isCompacting ||
        state.pendingApproval ||
        state.agentError ||
        state.transientAlert ||
        state.surveyPrompt ||
        state.queuedMessages.length > 0 ||
        state.pendingSteerContent != null ||
        state.tasks.some((t) => t.status === 'pending')
      ) {
        return;
      }

      // Record cooldown immediately when showing the prompt — even if the
      // user quits without responding, the cooldown clock starts now.
      markSurveyShown(SESSION_FEEDBACK_SURVEY.id);
      set({
        surveyPrompt: {
          message: SESSION_FEEDBACK_SURVEY.notificationMessage,
          survey: SESSION_FEEDBACK_SURVEY,
        },
      });
    },

    openSurveyPanel: (survey?: SurveyDefinition) => {
      const target = survey ?? get().activeSurvey ?? SESSION_FEEDBACK_SURVEY;
      markSurveyShown(target.id);
      set((s) => ({
        showSurveyPanel: true,
        activeSurvey: target,
        surveyPrompt: null,
        transientAlert: null,
        surveyState: { ...s.surveyState, lastShownAt: Date.now() },
      }));
    },

    closeSurveyPanel: () => {
      set({ showSurveyPanel: false, activeSurvey: null });
    },

    submitSurvey: (answers) => {
      const survey = get().activeSurvey ?? SESSION_FEEDBACK_SURVEY;

      // Cooldown semantics:
      //   - session-feedback uses its own 30-day cooldown (independent).
      //   - plan-quality and implement-plan share a 90-day cooldown with each
      //     other (they are a pair: implement is gated on plan having shown).
      if (survey.id === SESSION_FEEDBACK_SURVEY.id) {
        markSurveyCompleted(SESSION_FEEDBACK_SURVEY.id);
      } else if (
        survey.id === PLAN_QUALITY_SURVEY.id ||
        survey.id === IMPLEMENT_PLAN_SURVEY.id
      ) {
        markSurveyCompleted(PLAN_QUALITY_SURVEY.id);
        markSurveyCompleted(IMPLEMENT_PLAN_SURVEY.id);
      }
      // unknown survey id is a noop — sharing is opt-in per id

      logger.info('[survey] submitted', {
        surveyId: survey.id,
        answerCount: Object.keys(answers).length,
      });

      // Track plan survey shown for gating implement-plan survey.
      const planShown =
        survey.id === PLAN_QUALITY_SURVEY.id
          ? true
          : get().planSurveyShownThisSession;

      set((s) => ({
        showSurveyPanel: false,
        activeSurvey: null,
        planSurveyShownThisSession: planShown,
        surveyState: {
          ...s.surveyState,
          lastShownAt: Date.now(),
          lastCompletedAt: Date.now(),
        },
        transientAlert: {
          message: 'Thanks for your feedback',
          status: 'success' as const,
          autoHideMs: 3000,
        },
      }));

      // Fire-and-forget ingestion.
      const metadata = {
        sessionId: get().sessionId ?? undefined,
        isInternal: features.isInternalUser,
        agentEngine: get().agentEngine,
      };
      submitFormToAperture(survey, answers, { metadata })
        .then((outcome) => {
          if (outcome.rateLimited) {
            get().showTransientAlert({
              message:
                outcome.message ??
                'Feedback submission is temporarily unavailable.',
              status: 'warning',
              autoHideMs: 5000,
            });
          }
        })
        .catch((err) => {
          logger.warn('[survey] submit promise rejected', err);
        });
    },

    dismissSurveyPrompt: () => {
      // Cooldown semantics:
      //   - session-feedback uses its own 30-day cooldown (independent).
      //   - plan-quality and implement-plan share a 90-day cooldown with each
      //     other.
      const dismissed =
        get().surveyPrompt?.survey ?? get().activeSurvey ?? null;
      // Stray invocation with no active survey: noop. Do not re-couple
      // session-feedback to the plan/implement pair by marking everything.
      if (!dismissed) return;
      if (dismissed.id === SESSION_FEEDBACK_SURVEY.id) {
        markSurveyDismissed(SESSION_FEEDBACK_SURVEY.id);
      } else if (
        dismissed.id === PLAN_QUALITY_SURVEY.id ||
        dismissed.id === IMPLEMENT_PLAN_SURVEY.id
      ) {
        markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
        markSurveyDismissed(IMPLEMENT_PLAN_SURVEY.id);
      }
      // unknown survey id is a noop — sharing is opt-in per id

      set((s) => ({
        surveyPrompt: null,
        transientAlert: null,
        activeSurvey: null,
        surveyState: {
          ...s.surveyState,
          lastShownAt: Date.now(),
          dismissCount: s.surveyState.dismissCount + 1,
        },
      }));
    },

    triggerPlanSurvey: () => {
      const state = get();
      if (state.showSurveyPanel || state.surveyPrompt || state.transientAlert)
        return;

      // Resolve eligibility for plan survey (10% sampling, 30-day cooldown)
      const planState = loadSurveyState(PLAN_QUALITY_SURVEY.id);
      const { eligible, state: resolved } = resolveEligibility(
        PLAN_QUALITY_SURVEY,
        planState
      );
      if (!eligible || !shouldShowSurvey(PLAN_QUALITY_SURVEY, resolved)) return;

      markSurveyShown(PLAN_QUALITY_SURVEY.id);
      set({
        planSurveyShownThisSession: true,
        surveyPrompt: {
          message: PLAN_QUALITY_SURVEY.notificationMessage,
          survey: PLAN_QUALITY_SURVEY,
        },
      });
    },

    triggerImplementPlanSurvey: () => {
      const state = get();
      // Only shown if the plan-quality survey was shown this session.
      if (!state.planSurveyShownThisSession) return;
      if (state.showSurveyPanel) return;

      // If the plan survey prompt is still showing (user never responded),
      // replace it with the more relevant implementation survey.
      if (state.surveyPrompt?.survey.id === PLAN_QUALITY_SURVEY.id) {
        markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
      }

      markSurveyShown(IMPLEMENT_PLAN_SURVEY.id);
      set({
        surveyPrompt: {
          message: IMPLEMENT_PLAN_SURVEY.notificationMessage,
          survey: IMPLEMENT_PLAN_SURVEY,
        },
      });
    },

    dispatchSlashCommand: async (execCmd: string, recordAs?: string) => {
      CommandHistory.getInstance().add(recordAs ?? execCmd);
      const ctx: CommandContext = buildCommandContext(get(), set, get);
      await executeCommand(execCmd, ctx);
    },

    // Main orchestrator
    handleUserInput: async (input: string) => {
      const trimmed = input.trim();
      const hasPendingImages = get().pendingImages.length > 0;
      if (!trimmed && !hasPendingImages) return;

      const state = get();
      state.resetExitSequence();

      // Queue if processing, loading a session, or not yet initialized — but
      // always allow /quit and /exit through. loadingMessage covers the
      // /chat <id> resume window in lite: the input row stays visible so the
      // user can keep typing, but the agent isn't ready to receive input yet.
      if (
        state.isProcessing ||
        state.isCompacting ||
        !state.isInitialized ||
        state.loadingMessage
      ) {
        const lower = trimmed.toLowerCase();
        // Collapse internal whitespace so e.g. "/goal  clear" matches "/goal clear".
        const normalized = lower.replace(/\s+/g, ' ');

        if (lower === '/quit' || lower === '/exit') {
          state.clearInput();
          if (state.kiro.isCloudSessionActive?.()) {
            // Cloud: the keep-running/turn-off prompt must ALWAYS appear, even
            // mid-turn — same flow as the idle-path quit effect. The prompt's
            // choices already handle an in-flight turn (keep-running detaches,
            // turn-off cancels first).
            get().setShowCloudQuitPrompt(true);
            return;
          }
          state.kiro.close();
          state.onExit?.();
          process.exit(0);
        }

        // /disconnect only detaches the client — the sandbox keeps running
        // the in-flight turn — so it must work mid-turn instead of queueing
        // behind a turn it doesn't interrupt.
        if (lower === '/disconnect' && state.kiro.isCloudSessionActive?.()) {
          state.clearInput();
          emitCloudDetachNoticeOnce(state.kiro.sessionId);
          state.kiro.close();
          state.onExit?.();
          process.exit(0);
        }

        // Whitelist for non-interactive backend commands that are safe during processing.
        // These don't queue input on the agent — they execute immediately on the backend
        // because they're either read-only or required as escape hatches (e.g. /goal clear
        // to stop a runaway goal loop).
        const runWhileProcessing = async (
          backendArg: string,
          errorContext: string,
          optimisticUiUpdate?: () => void
        ) => {
          state.clearInput();
          optimisticUiUpdate?.();
          try {
            await state.kiro.executeCommand({
              command: 'goal',
              args: { subcommand: backendArg },
            });
          } catch (err) {
            const reason = err instanceof Error ? err.message : String(err);
            state.showTransientAlert({
              message: `${errorContext}: ${reason}`,
              status: 'warning',
              autoHideMs: 4000,
            });
          }
        };

        if (normalized === '/goal clear') {
          // Required escape hatch: user must be able to abort runaway goal loops.
          // Update TUI optimistically; backend notification will reconcile on next event.
          await runWhileProcessing('clear', 'Failed to clear goal', () =>
            state.setGoalStatus(null)
          );
          return;
        }
        if (normalized === '/goal status') {
          // Read-only; no UI side effects.
          await runWhileProcessing('status', 'Failed to read goal status');
          return;
        }
        // TODO: support queuing non-interactive slash commands (e.g. /clear, /compact)
        //       that don't require UI interaction to complete
        if (trimmed.startsWith('/')) {
          // Lite mode queues slash commands so they fire at turn-end. Unknown
          // tokens (e.g. "/foozle", pasted "/some/file/path") fall through to
          // chat-message queueing — the lite contract is "first token must
          // exactly match a known command, otherwise it's a message".
          const allCommands = liteGateCommands(state);
          const isLite = state.uiMode === 'lite';
          const isKnown = isKnownSlashCommandToken(trimmed, allCommands);
          if (isLite && isKnown) {
            // The queue strip above the divider already shows the new entry
            // (and "(N queued)") the moment queueMessage returns, so no
            // transient alert is needed.
            state.queueMessage(trimmed);
            // clearInput resets only the lower-level state.input buffer
            // (lines, cursor). PromptInput renders from `commandInputValue`,
            // which is a separate slot that PromptInput's syncToStore
            // writes to on every keystroke. Without clearCommandInput the
            // visible row keeps showing the just-queued command — and the
            // user's next keystroke appends to it: typing `/model` again
            // produces `/model/model`, which fails isKnownSlashCommandToken
            // and falls through to chat-message queueing below. The two
            // clears together mirror what InlineLayout does at its own
            // submit site (clearInput() + clearCommandInput()) and what
            // the main-path `set` block below does inline.
            state.clearInput();
            state.clearCommandInput();
            return;
          }
          if (!isLite) {
            state.showTransientAlert({
              message:
                "Slash commands can't be queued — wait for the current task to finish",
              status: 'warning',
              autoHideMs: 4000,
            });
            return;
          }
          // Lite + unknown token: fall through to chat-message queueing below.
        }
        if (trimmed.startsWith('!')) {
          state.showTransientAlert({
            message:
              "Shell escape commands can't be queued — wait for the current task to finish",
            status: 'warning',
            autoHideMs: 4000,
          });
          return;
        }
        state.queueMessage(trimmed);
        // Same incomplete-clear reason as the slash branch above:
        // clearInput leaves commandInputValue populated, so the visible
        // input keeps the just-queued chat message and the user's next
        // keystroke appends to it.
        state.clearInput();
        state.clearCommandInput();
        return;
      }

      // /spec new description-collection: the next submitted line is the
      // feature description, not a chat message. Real commands still run —
      // only leaving spec mode (or esc) voids the step, so informational
      // commands don't cost the user their setup.
      const pendingSpec = state.pendingSpecDescription;
      const isCommandInput = isKnownSlashCommandToken(
        trimmed,
        liteGateCommands(state)
      );
      const isSpecDescription = !!pendingSpec && !isCommandInput;
      if (isSpecDescription && !trimmed) {
        // Images-only submit: a spec can't start from an empty description.
        // No buffer clear needed — the input row clears itself pre-submit.
        state.showTransientAlert({
          message: 'Describe the spec in words first — images stay attached',
          status: 'warning',
          autoHideMs: 4000,
        });
        return;
      }

      // Clear all UI state before processing any input
      const hadSurveyPrompt = !!state.surveyPrompt;
      const dismissedSurveyId = state.surveyPrompt?.survey.id ?? null;
      set({
        activeCommand: null,
        showContextBreakdown: false,
        showHelpPanel: false,
        showUsagePanel: false,
        showRewindExplorer: false,
        showTangentExplorer: false,
        commandInputValue: '',
        activeTrigger: null,
        promptHint: null,
        commandShadowText: null,
        surveyPrompt: null,
      });
      state.clearInput();

      // If the survey prompt was showing and the user chose to type instead
      // of accepting it, count that as a dismissal toward the cooldown.
      if (hadSurveyPrompt) {
        // Cooldown semantics:
        //   - session-feedback uses its own 30-day cooldown (independent).
        //   - plan-quality and implement-plan share a 90-day cooldown with
        //     each other.
        if (dismissedSurveyId === SESSION_FEEDBACK_SURVEY.id) {
          markSurveyDismissed(SESSION_FEEDBACK_SURVEY.id);
        } else if (
          dismissedSurveyId === PLAN_QUALITY_SURVEY.id ||
          dismissedSurveyId === IMPLEMENT_PLAN_SURVEY.id
        ) {
          markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
          markSurveyDismissed(IMPLEMENT_PLAN_SURVEY.id);
        } else {
          // Defensive fallback for unknown survey ids; current code paths
          // only set surveyPrompt with known ids.
          markSurveyDismissed(SESSION_FEEDBACK_SURVEY.id);
          markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
          markSurveyDismissed(IMPLEMENT_PLAN_SURVEY.id);
        }
        set((s) => ({
          surveyState: {
            ...s.surveyState,
            lastShownAt: Date.now(),
            dismissCount: s.surveyState.dismissCount + 1,
          },
        }));
      }

      // Clear announcement on first user interaction
      if (state.announcement) {
        set({ announcement: null, announcementExpanded: false });
      }

      // Spec description: send the kickoff. Placed after the shared cleanup
      // (so surveys/panels behave as on any submit) and before the slash and
      // shell branches (a description may start with '/' or '!').
      if (pendingSpec && isSpecDescription) {
        set({ pendingSpecDescription: null });
        await state.sendMessage(
          composeSpecKickoffPrompt(pendingSpec.featureName, trimmed),
          undefined,
          trimmed
        );
        return;
      }

      // A typed `@name` that exactly matches a known prompt is routed as its
      // slash form. Interception must not depend on the @ menu being open:
      // menu state is async (debounced search, late MCP prompt advertisement)
      // and pasted input never opens it.
      const routed = normalizeAtPrompt(
        trimmed,
        selectVisibleSlashCommands(state)
      );

      // Handle slash commands via command registry
      if (routed.startsWith('/')) {
        // Rewritten @prompts always end in a message send, which records
        // history itself; recording here too would double-add since the
        // history dedupe only collapses consecutive identical entries.
        if (routed === trimmed) CommandHistory.getInstance().add(routed);
        const ctx: CommandContext = buildCommandContext(state, set, get);

        applyLiteAlertRouting(ctx, state, set);

        // Lite mode: only dispatch when the first whitespace-separated token
        // is an EXACT command-registry match. Typos like "/foozle" and pasted
        // paths like "/some/file/path" go straight to chat. Subcommand errors
        // (e.g. "/verbose foozle") still flow through the dispatcher so the
        // command's own handler can show the proper error.
        if (state.uiMode === 'lite') {
          const allCommands = liteGateCommands(state);
          if (isKnownSlashCommandToken(routed, allCommands)) {
            await executeCommand(routed, ctx);
            return;
          }
          await state.sendMessage(routed, undefined, trimmed);
          return;
        }

        const handled = await executeCommand(routed, ctx);
        if (handled) return;
        // Dispatch declined a rewritten @prompt (the command parser can
        // reject names its heuristics read as file paths); send the typed
        // text untouched rather than letting the fallthrough mangle it.
        if (routed !== trimmed) {
          await state.sendMessage(trimmed);
          return;
        }
        // Not a recognized command — could be a file path like /Users/...
        // Strip the leading "/" only for file paths to match V1 behavior
        // (leaving it confuses the LLM's path extraction for tool calls).
        // For other inputs like "// hello world", send as-is.
        const afterSlash = routed.slice(1);
        const isFilePath =
          afterSlash.length > 0 &&
          afterSlash[0] !== '/' &&
          afterSlash[0] !== ' ';

        const messageText = isFilePath ? afterSlash : routed;
        await state.sendMessage(messageText, undefined, trimmed);
        return;
      }

      // Handle shell escape commands
      if (trimmed.startsWith('!')) {
        const command = trimmed.slice(1).trim();
        if (!command) return;

        // Cloud sessions: `!` runs on the LOCAL machine, not the sandbox,
        // which is misleading (and some paths error outright) — refuse
        // before the terminal is touched. Strictly cloud-gated (dark-ship):
        // local sessions never reach this branch.
        if (state.cloudSessionActive) {
          state.showTransientAlert({
            message:
              'Shell commands are not available for a cloud session yet.',
            status: 'error',
            autoHideMs: 5000,
          });
          return;
        }

        const {
          needsTTY,
          isClearCommand,
          executeClearCommand,
          executeShellEscapeTTY,
          executeShellEscapeStreaming,
        } = await import('../utils/shell-escape.js');

        // Clear/reset: clear messages and terminal, no user message needed
        if (isClearCommand(command)) {
          set({ messages: [] });
          executeClearCommand();
          return;
        }

        // Add user message showing the command
        const userMsgId = generateMessageId();
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: userMsgId,
              role: MessageRole.User,
              content: chalk.hex('#C19AFF')('!') + command,
              agentName: state.currentAgent?.name,
            },
          ],
        }));

        if (needsTTY(command)) {
          // Full-screen TTY mode: alternate screen + direct terminal access
          const result = executeShellEscapeTTY(command);
          if (result.exitCode !== 0) {
            const msg = result.error || `Exited with status ${result.exitCode}`;
            set((state) => ({
              messages: [
                ...state.messages,
                {
                  id: generateMessageId(),
                  role: MessageRole.System,
                  content: msg,
                  success: false,
                },
              ],
            }));
          }
        } else {
          // Streaming mode: pipe output into conversation.
          // Stdin is inherited so interactive commands
          // can prompt the user for input.
          const outputMsgId = generateMessageId();
          let accumulated = '';

          // Add initial empty model message and set processing
          set((state) => ({
            isProcessing: true,
            isShellEscape: true,
            messages: [
              ...state.messages,
              {
                id: outputMsgId,
                role: MessageRole.Model,
                content: '',
                agentName: state.currentAgent?.name,
                shellOutput: true,
              },
            ],
          }));

          const { promise, kill, write } = executeShellEscapeStreaming(
            command,
            (chunk) => {
              accumulated += chunk;
              // Update the model message with accumulated output
              set((state) => ({
                messages: state.messages.map((msg) =>
                  msg.id === outputMsgId
                    ? { ...msg, content: accumulated }
                    : msg
                ),
              }));
            }
          );

          // Store kill function for Ctrl+C cancellation
          const abortController = new AbortController();
          const origKill = kill;
          abortController.signal.addEventListener('abort', () => origKill());
          set({
            currentAbortController: abortController,
            _shellEscapeWriter: write,
          });

          try {
            const result = await promise;

            // Finalize. Three cases:
            //  - buffer non-empty, exit 0  → keep buffer as-is
            //  - buffer non-empty, exit ≠0 → buffer + "\n\n[exit code: N]"
            //  - buffer empty             → leave content '' and let
            //    the lite-mode empty-Model skip drop the row entirely.
            //    Previously inserted '(no output)' here, but that flipped
            //    the message from ineligible→eligible AFTER cancelMessage
            //    had already set isProcessing=false and the cancelArmedRef
            //    effect had appended a 'user interrupted' System row —
            //    causing the lite static-flush delta walk to push the
            //    System row a second time and trigger React's
            //    "Encountered two children with the same key" warning
            //    forever (the duplicate id stays in the append-only
            //    static items array). For empty-output shell commands
            //    there's nothing useful to show in scrollback anyway;
            //    if it failed, the exit-code suffix below provides the
            //    minimum signal the user needs.
            const exitSuffix =
              result.exitCode !== 0
                ? `\n\n[exit code: ${result.exitCode}]`
                : '';
            const finalContent = accumulated
              ? accumulated + exitSuffix
              : exitSuffix.trimStart();
            // Only push the content update when there's something to
            // commit — `accumulated || exitSuffix` covers all the cases
            // where the row should land in scrollback. Empty buffer +
            // exit 0 → finalContent is '', and we skip the setState so
            // the row stays empty and gets dropped by the empty-Model
            // filter. Avoids re-introducing the same race the comment
            // above describes.
            if (finalContent) {
              set((state) => ({
                messages: state.messages.map((msg) =>
                  msg.id === outputMsgId
                    ? { ...msg, content: finalContent }
                    : msg
                ),
              }));
            }
          } finally {
            set({
              isProcessing: false,
              isShellEscape: false,
              _shellEscapeWriter: null,
              currentAbortController: null,
            });
          }
          // Shell command finished — drain any queued messages.
          await get().processQueue();
        }
        return;
      }

      // Handle regular prompts
      await state.sendMessage(trimmed);
    },
  }));

  // Track the last progress state we wrote to the terminal so we only
  // emit an OSC 9;4 escape when the derived indicator actually changes.
  let lastProgressKey: string | null = null;

  store.subscribe((state) => {
    // Suppress OSC 9;4 progress on alternate screen — it pollutes the
    // tab-bar indicator and causes unnecessary escape-sequence writes.
    const onAltScreen =
      state.mode === 'crew-monitor' || state.mode === 'session-view';
    // Derive a cache key from the fields that affect the progress indicator
    const key = `${onAltScreen}|${state.agentError ?? ''}|${state.pendingApproval != null}|${state.pendingQuestion != null}|${state.isProcessing}|${state.isCompacting}|${state.contextUsagePercent}`;
    if (key !== lastProgressKey) {
      lastProgressKey = key;
      // Defer so the OSC 9;4 escape lands after twinki's nextTick render frame.
      setImmediate(() => {
        if (onAltScreen) {
          clearTerminalProgress();
        } else {
          syncTerminalProgress(state);
        }
      });
    }
  });

  // Terminal notifications (bell / OSC 9) on turn-end and required input.
  let prevProcessing = false;
  let prevApproval: unknown = null;
  let prevQuestion: unknown = null;

  store.subscribe((state) => {
    const enabled = state.settings?.[Settings.CHAT_ENABLE_NOTIFICATIONS];
    const wasProcessing = prevProcessing;
    const hadApproval = prevApproval;
    const hadQuestion = prevQuestion;
    prevProcessing = state.isProcessing;
    prevApproval = state.pendingApproval;
    prevQuestion = state.pendingQuestion;

    // Turn completed cleanly — bump our survey turn counter regardless of
    // whether the terminal bell is enabled. The store action decides whether
    // to trigger the survey prompt.
    const turnJustEnded =
      wasProcessing &&
      !state.isProcessing &&
      !state.agentError &&
      !state.wasCancelled;
    if (turnJustEnded) {
      // Defer so we run after the state that set isProcessing=false commits,
      // avoiding a re-entrant set() inside this subscriber.
      queueMicrotask(() => {
        store.getState().recordCompletedTurn();
      });
    }

    if (!enabled) return;

    const method = resolveNotificationMethod(
      state.settings?.[Settings.CHAT_NOTIFICATION_METHOD] as string | undefined
    );
    if (!method) return;

    if (turnJustEnded) {
      playNotification(method, 'Response complete');
    }

    // Tool approval requested
    if (!hadApproval && state.pendingApproval) {
      playNotification(method, 'Permission required');
    }
    if (!hadQuestion && state.pendingQuestion) {
      playNotification(method, 'Input required');
    }
  });

  // Never resets: an in-session /chat new re-empties `messages`, and the
  // cold-boot connect screen must not re-open for it.
  store.subscribe((state) => {
    if (!state.hasEnteredConversation && state.messages.length > 0) {
      store.setState({ hasEnteredConversation: true });
    }
  });

  return store;
};
