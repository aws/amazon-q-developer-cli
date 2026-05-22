import { createStore, useStore, type StoreApi } from 'zustand';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { Kiro } from '../kiro';
import chalk from 'chalk';
import { kiroSafe } from '../theme/kiroSafe';
import type { TerminalColor } from '../types/themeTypes';
import { createContext, useContext } from 'react';
import { KAS_COMMANDS, type KasCommand } from '../kas-commands';
import { type AgentEngine, resolveAgentEngine } from '../agent-engine';
import {
  AgentEventType,
  ApprovalOptionId,
  TASK_TOOL_NAMES,
  SESSION_TOOL_NAMES,
  type AgentStreamEvent,
  type ApprovalRequestInfo,
  type ToolKind,
} from '../types/agent-events';
import type {
  InputBufferState,
  InputBufferActions,
  MoveCursorDir,
} from '../types/input-buffer';
import type { AvailableCommand, CommandOption } from '../types/commands';
import type { StatusType } from '../types/componentTypes';
import type { SubagentInfo, SubagentStatus } from '../types/subagent.js';
import type { AgentSession, InboxMessage } from '../types/multi-session.js';
import type { TaskItem, RawTask } from '../types/tasks';

/** A selectable turn in the `/rewind` Explorer. Shape is defined by the
 *  backend `/rewind` execute handler in `CommandResult.data.turns`. */
export interface RewindTurn {
  logIndex: number;
  label: string;
  group: string;
  responseSnippet: string;
}

export interface ContextBreakdownData {
  contextFiles: {
    percent: number;
    tokens: number;
    items?: Array<{
      name: string;
      tokens: number;
      matched: boolean;
      percent: number;
    }>;
  };
  tools: { percent: number; tokens: number };
  kiroResponses: { percent: number; tokens: number };
  yourPrompts: { percent: number; tokens: number };
  sessionFiles?: { percent: number; tokens: number };
  /** UI-specific: initially show context breakdown in expanded mode */
  initialExpanded?: boolean;
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
}

export interface BonusCredit {
  name: string;
  used: number;
  total: number;
  daysUntilExpiry: number;
}

export interface UsageData {
  planName: string;
  billingCycleReset: string;
  overagesEnabled: boolean;
  isEnterprise: boolean;
  usageBreakdowns: UsageBreakdownItem[];
  bonusCredits: BonusCredit[];
}

export interface McpServerInfo {
  name: string;
  status: 'running' | 'loading' | 'failed' | 'disabled' | 'auth-required';
  toolCount: number;
  // Registry fields (present in /mcp list response)
  version?: string;
  description?: string;
  enabled?: boolean;
}

export interface ToolInfo {
  name: string;
  source: string;
  description: string;
  status: 'allowed' | 'requires-approval' | 'denied';
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
import { buildSettingsActiveCommand } from '../commands/settings-subcommands.js';
import { formatImageLabel } from '../utils/image-label.js';
import { expandFileReferences, readFileContent } from '../utils/file-search.js';
import { logger } from '../utils/logger.js';
import {
  setTerminalProgressWarning,
  setTerminalProgressIndeterminate,
  setTerminalProgressError,
  clearTerminalProgress,
} from '../utils/terminal-capabilities.js';
import {
  getAuthErrorGuidance,
  getSessionErrorGuidance,
  getErrorGuidance,
  simplifyErrorMessage,
  detectErrorCategory,
} from '../utils/error-guidance.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { CommandHistory } from '../utils/command-history.js';
import { Settings } from '../constants/settings.js';
import { readStringSetting } from '../utils/cli-settings.js';
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
  | { status: 'error'; error: string }
  | { status: 'cancelled' };

export type MessageType =
  | { id: string; role: MessageRole.User; content: string; agentName?: string }
  | {
      id: string;
      role: MessageRole.Model;
      content: string;
      thinking?: string;
      agentName?: string;
      shellOutput?: boolean;
      standalone?: boolean;
    }
  | {
      id: string;
      role: MessageRole.ToolUse;
      name: string;
      kind?: ToolKind;
      content: string;
      isFinished?: boolean;
      status?: ToolUseStatus;
      result?: ToolResult;
      locations?: Array<{ path: string; line?: number }>;
      agentName?: string;
      liveOutput?: string[];
    }
  | { id: string; role: MessageRole.System; content: string; success: boolean };

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

export interface SlashCommand extends AvailableCommand {
  source: 'local' | 'backend';
}

export interface ActiveCommand {
  command: AvailableCommand;
  options: CommandOption[];
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
  | { type: 'mcp_governance_disabled'; apiFailure: boolean };

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

/** Compute a summary message from accumulated init errors. */
export function summarizeInitErrors(errors: InitError[]): string | null {
  if (errors.length === 0) return null;

  const mcpFailures = errors.filter((e) => e.type === 'mcp_failure');
  const agentNotFound = errors.filter((e) => e.type === 'agent_not_found');
  const configErrors = errors.filter((e) => e.type === 'agent_config_error');
  const mcpGovernance = errors.filter(
    (e) => e.type === 'mcp_governance_disabled'
  );
  const parts: string[] = [];

  // MCP governance disabled (show first — important admin notice)
  if (mcpGovernance.length > 0) {
    const e = mcpGovernance[0]!;
    parts.push(
      e.apiFailure
        ? 'failed to retrieve MCP settings — MCP disabled'
        : 'MCP disabled by your administrator'
    );
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
  const hasHardError = errors.some((e) => e.type !== 'mcp_governance_disabled');
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
}

export type AppActions = BaseAppActions & InputBufferActions;

interface BaseAppActions {
  // Kiro actions
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>,
    displayContent?: string
  ) => Promise<void>;
  createStreamEventHandler: () => (event: AgentStreamEvent) => void;
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
  setApprovalMode: (mode: 'dropdown' | 'drill-in') => void;
  setAutoApproveCrewTools: (value: boolean) => void;
  setCurrentModel: (model: { id: string; name: string } | null) => void;
  setCurrentEffort: (effort: string | null) => void;
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
  queueMessage: (content: string) => void;
  processQueue: () => Promise<void>;
  clearQueue: () => void;
  removeQueuedMessage: (index: number) => void;
  replaceQueuedMessage: (index: number, content: string) => void;
  startEditingQueue: (index: number) => void;
  cancelEditingQueue: () => void;
  setSlashCommands: (commands: SlashCommand[]) => void;
  setKasCommands: (commands: KasCommand[]) => void;
  setPrompts: (
    prompts: Array<{
      name: string;
      description?: string;
      arguments: Array<{
        name: string;
        description?: string;
        required?: boolean;
      }>;
      serverName: string;
    }>
  ) => void;

  // Command UI actions
  setActiveCommand: (command: ActiveCommand | null) => void;
  executeCommandWithArg: (arg: string) => Promise<void>;
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
  addMessage: (sessionId: string, message: InboxMessage) => void;
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
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string,
    registryServers?: McpServerInfo[]
  ) => void;
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  setShowStatsPanel: (
    show: boolean,
    stats?: RequestStat[],
    summary?: StatsSummary | null
  ) => void;
  setShowHooksPanel: (show: boolean, hooks?: HookInfo[]) => void;
  setShowKeybindingsPanel: (show: boolean) => void;
  setShowDisplaySettingsPanel: (show: boolean) => void;
  setSettingsReturnOnEscape: (value: boolean) => void;
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

  // Theme preview string (rendered below menu during /theme flow)
  themePreview: string | null;
  setThemePreview: (preview: string | null) => void;

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

  // Task management actions
  setTasks: (tasks: TaskItem[]) => void;
  toggleActivityTray: () => void;

  // Announcement actions
  setAnnouncement: (msg: { id: string; maxLines: number } | null) => void;
  toggleAnnouncementExpanded: () => void;

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

export interface AppState {
  // Chat state
  messages: MessageType[];
  liveOutputs: Map<string, string[][]>;
  queuedMessages: string[];
  editingQueueIndex: number | null;
  /**
   * Slash commands sourced from the active backend's
   * `available_commands_update` broadcast.
   *
   * - V2 mode: populated by `setSlashCommands` from V2's broadcast; prompts
   *   and skills get appended directly here via `onPromptsUpdate`.
   * - KAS mode: populated by `setSlashCommands` from KAS's broadcast, which
   *   already includes prompts/skills/steering as commands tagged with
   *   `_meta.kiro.type`.
   */
  slashCommands: SlashCommand[];
  /**
   * Static, TUI-owned KAS commands. Seeded from `KAS_COMMANDS` at boot
   * when `agentEngine === 'kas'`; empty otherwise. The dispatcher checks
   * this list first in KAS mode so KAS-side handlers take precedence
   * over the V2 dispatcher pipeline for the same command name.
   */
  kasCommands: KasCommand[];
  /** Frozen at boot from props.agentEngine ?? process.env.KIRO_AGENT_ENGINE. */
  agentEngine: AgentEngine;
  prompts: Array<{
    name: string;
    description?: string;
    arguments: Array<{
      name: string;
      description?: string;
      required?: boolean;
    }>;
    serverName: string;
  }>;

  // Kiro/Agent state
  kiro: Kiro;
  onExit?: () => void;
  sessionId: string | null;
  isProcessing: boolean;
  isCompacting: boolean;
  wasCancelled: boolean;
  agentError: string | null;
  agentErrorGuidance: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  approvalQueue: ApprovalRequestInfo[];
  approvalMode: 'dropdown' | 'drill-in';
  autoApproveCrewTools: boolean;
  focusedCrewIndex: number;
  setFocusedCrewIndex: (index: number) => void;
  currentModel: { id: string; name: string } | null;
  currentEffort: string | null;
  currentAgent: { name: string } | null;
  previousAgentName: string | null;
  settings: Record<string, unknown> | null;

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
  sessionMessages: Map<string, InboxMessage[]>;
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
  loadingMessage: string | null;
  toolOutputsExpanded: boolean; // Global toggle for all tool outputs
  hasExpandableToolOutputs: boolean; // Whether there are any tool outputs that can be expanded

  // Context usage state
  contextUsagePercent: number | null;
  lastTurnTokens: LastTurnTokens | null;
  turnSummaries: Map<string, string>; // turnId (user message id) → formatted summary text

  // Usage panel state
  showUsagePanel: boolean;
  usageData: UsageData | null;

  // Rewind explorer state
  showRewindExplorer: boolean;
  rewindRows: RewindTurn[];

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
  pendingOAuthServers: Map<string, string>; // serverName → oauthUrl
  initErrors: InitError[];
  mcpMode: string;
  showToolsPanel: boolean;
  toolsList: ToolInfo[];
  showStatsPanel: boolean;
  statsList: RequestStat[];
  statsSummary: StatsSummary | null;
  showHooksPanel: boolean;
  hooksList: HookInfo[];
  showKeybindingsPanel: boolean;
  showDisplaySettingsPanel: boolean;
  /**
   * When true, closing the currently open overlay re-opens the /settings
   * top-level menu instead of fully dismissing. Set by /settings subcommand
   * handlers before they hand off to showThemeMenu / setShowKeybindingsPanel,
   * consumed by the ESC handlers (CommandMenu and handleCloseKeybindingsPanel),
   * and reset whenever consumed.
   */
  settingsReturnOnEscape: boolean;
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

  // Initialization state — true once the ACP session is ready
  isInitialized: boolean;

  // Non-interactive mode
  noInteractive: boolean;

  // Trust all tools mode
  trustAllToolsRequested: boolean;
  trustAllToolsConfirmed: boolean;

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

const CONTEXT_WARNING_THRESHOLD = 60;

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
    | 'isProcessing'
    | 'isCompacting'
    | 'contextUsagePercent'
  >
): void {
  if (state.isProcessing || state.isCompacting) {
    // Active — always spinning unless paused for approval
    if (state.agentError) {
      setTerminalProgressError(); // pulsing red
    } else if (state.pendingApproval) {
      setTerminalProgressWarning(100); // static yellow at 100%
    } else {
      setTerminalProgressIndeterminate(); // spinning green
    }
  } else {
    // Idle — static bar or hidden
    if (state.agentError) {
      setTerminalProgressError(); // pulsing red
    } else if (
      state.contextUsagePercent != null &&
      state.contextUsagePercent >= CONTEXT_WARNING_THRESHOLD
    ) {
      setTerminalProgressWarning(state.contextUsagePercent); // static yellow with %
    } else {
      clearTerminalProgress(); // hidden
    }
  }
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

/** Build a CommandContext from the current AppState + setter. */
function buildCommandContext(
  state: AppState & AppActions,
  set: StoreApi<AppState & AppActions>['setState'],
  get: StoreApi<AppState & AppActions>['getState'],
  extraClearState?: Partial<AppState>
): CommandContext {
  return {
    kiro: state.kiro,
    agentEngine: state.agentEngine,
    slashCommands: state.slashCommands,
    kasCommands: state.kasCommands,
    showAlert: (message, status, autoHideMs = 3000) =>
      state.showTransientAlert({ message, status, autoHideMs }),
    setLoadingMessage: state.setLoadingMessage,
    setActiveCommand: state.setActiveCommand,
    setCurrentModel: state.setCurrentModel,
    setCurrentAgent: state.setCurrentAgent,
    setContextUsage: state.setContextUsage,
    setShowContextBreakdown: state.setShowContextBreakdown,
    setShowHelpPanel: state.setShowHelpPanel,
    setShowTuiPanel: state.setShowTuiPanel,
    setShowChangelogPanel: state.setShowChangelogPanel,
    setShowUsagePanel: state.setShowUsagePanel,
    setShowRewindExplorer: state.setShowRewindExplorer,
    setShowMcpPanel: state.setShowMcpPanel,
    setShowToolsPanel: state.setShowToolsPanel,
    setShowStatsPanel: state.setShowStatsPanel,
    setShowHooksPanel: state.setShowHooksPanel,
    setShowKeybindingsPanel: state.setShowKeybindingsPanel,
    setShowDisplaySettingsPanel: state.setShowDisplaySettingsPanel,
    setSettingsReturnOnEscape: state.setSettingsReturnOnEscape,
    setShowKnowledgePanel: state.setShowKnowledgePanel,
    setShowCodePanel: state.setShowCodePanel,
    openArtifactView: state.openArtifactView,
    clearMessages: state.clearMessages,
    resetMessages: state.resetMessages,
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
    addSystemMessage: (content: string, success: boolean) =>
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
      })),
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
        showMcpPanel: false,
        showToolsPanel: false,
        showStatsPanel: false,
        showHooksPanel: false,
        showKeybindingsPanel: false,
        settingsReturnOnEscape: false,
        showKnowledgePanel: false,
        contextBreakdown: null,
        usageData: null,
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
        name: '/tui',
        description: "What's new in the TUI experience",
        source: 'local' as const,
        meta: { local: true, inputType: 'panel' as const },
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
    ], // Backend sends all commands via CommandsUpdate
    kasCommands: agentEngine === 'kas' ? [...KAS_COMMANDS] : [],
    agentEngine,
    prompts: [],
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    isCompacting: false,
    wasCancelled: false,
    agentError: null,
    agentErrorGuidance: null,
    pendingApproval: null,
    approvalQueue: [],
    approvalMode: 'dropdown',
    autoApproveCrewTools: false,
    focusedCrewIndex: 0,
    currentModel: null,
    currentEffort: null,
    currentAgent: null,
    previousAgentName: null,
    settings: null,

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
    sessionMessages: new Map(),
    sessionEventBuffer: {},

    exitSequence: 0,
    exitTimer: null,
    suspendArmed: false,
    suspendTimer: null,
    transientAlert: null,
    retryStatus: null,
    loadingMessage: null as string | null,
    toolOutputsExpanded: false,
    hasExpandableToolOutputs: false,

    contextUsagePercent: null,
    lastTurnTokens: null,
    turnSummaries: new Map(),
    showContextBreakdown: false,
    contextBreakdown: null,
    showTuiPanel: false,
    showChangelogPanel: false,
    showHelpPanel: false,
    helpCommands: [],
    showUsagePanel: false,
    usageData: null,
    showRewindExplorer: false,
    rewindRows: [],
    showMcpPanel: false,
    mcpServers: [],
    mcpRegistryServers: [],
    pendingOAuthServers: new Map(),
    initErrors: [],
    mcpMode: 'list',
    showToolsPanel: false,
    toolsList: [],
    showStatsPanel: false,
    statsList: [],
    statsSummary: null,
    showHooksPanel: false,
    hooksList: [],
    showKeybindingsPanel: false,
    showDisplaySettingsPanel: false,
    settingsReturnOnEscape: false,
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
    themePreview: null,
    pendingFileAttachment: null,
    pendingImages: [],
    currentAbortController: null,
    cancelInProgress: null,
    isShellEscape: false,
    _shellEscapeWriter: null,
    streamingBuffer: { startBuffering: null, stopBuffering: null },

    // Task management
    tasks: [],
    activityTrayExpanded: false,

    // Announcement
    announcement: null,
    announcementExpanded: false,

    isInitialized: false,
    noInteractive: props.noInteractive ?? false,
    trustAllToolsRequested: props.trustAllTools ?? false,
    trustAllToolsConfirmed: false,

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
        isInitialized,
        attachedFiles,
        pendingImages,
      } = get();
      if (!isInitialized || isProcessing) {
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
        return {
          isProcessing: true,
          agentError: null,
          agentErrorGuidance: null,
          wasCancelled: false,
          autoApproveCrewTools: false,
          messages: [...state.messages, userMessage],
          attachedFiles: [], // Clear attachments after sending
          pendingImages: [], // Clear pending images after sending
          // Reset expandable content flag for new turn (expanded state persists)
          hasExpandableToolOutputs: false,
          // A fresh request starts — any retry banner from a previous request is stale.
          retryStatus: null,
        };
      });

      try {
        const eventHandler = get().createStreamEventHandler();
        await kiro.streamMessage(
          expandedContent,
          abortController.signal,
          eventHandler,
          allImages.length > 0 ? allImages : undefined
        );
        (eventHandler as any).flush?.();

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
        await get().processQueue();
      } catch (error) {
        set({ currentAbortController: null });
        logger.error('[store] sendMessage: caught error', error);
        if (error instanceof DOMException && error.name === 'AbortError') {
          set({ isProcessing: false });
          await get().processQueue();
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
          });
        } else {
          // All other errors are non-blocking (transient alerts)
          get().showTransientAlert({
            message: displayMessage,
            status: 'error',
            autoHideMs: 5000,
          });
          set({ isProcessing: false });
          await get().processQueue();
        }
      }
    },

    /**
     * Creates a synchronous event handler callback for stream events.
     * This is the core event-processing logic, used by streamMessage.
     * Returns a cleanup function via the returned handler's `.flush` property.
     */
    createStreamEventHandler: () => {
      let isBuffering = false;
      let bufferedContent = '';
      let bufferedThinking = '';

      // Batching: accumulate content chunks and flush to the store
      // on a timer so Ink's render loop isn't starved by rapid-fire
      // synchronous set() calls from the ACP notification handler.
      let pendingContentFlush: ReturnType<typeof setTimeout> | null = null;
      let lastContentEventId: string | null = null;

      // Per-tool-call live output buffering. ToolCallUpdate events for
      // verbose commands (e.g. a Gradle build streaming thousands of lines)
      // would otherwise trigger a React re-render per line. We batch per
      // tool_call_id and flush on a timer, mirroring the assistant content
      // batching above.
      const toolOutputBuffers = new Map<string, string>();
      let pendingToolOutputFlush: ReturnType<typeof setTimeout> | null = null;

      const flushToolOutputs = () => {
        pendingToolOutputFlush = null;
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

      const commitBufferedContent = () => {
        if (!bufferedContent && !bufferedThinking) return;
        set((state) => {
          const lastModelMsgIndex = state.messages.findLastIndex(
            (msg) => msg.role === MessageRole.Model
          );
          if (lastModelMsgIndex !== -1) {
            const msg = state.messages[lastModelMsgIndex];
            if (msg && msg.role === MessageRole.Model) {
              const messages = [...state.messages];
              messages[lastModelMsgIndex] = {
                ...msg,
                content: bufferedContent,
                thinking: bufferedThinking || msg.thinking,
              };
              return { messages };
            }
          }
          return {};
        });
      };

      const flushContentToStore = () => {
        pendingContentFlush = null;
        if (!bufferedContent && !bufferedThinking) return;

        set((state) => {
          const lastMsg = state.messages[state.messages.length - 1];
          if (lastMsg?.role === MessageRole.Model) {
            // Update existing model message: single array copy, direct index write
            const messages = [...state.messages];
            messages[messages.length - 1] = {
              id: lastMsg.id,
              role: MessageRole.Model,
              content: bufferedContent,
              thinking: bufferedThinking || lastMsg.thinking,
              agentName: lastMsg.agentName ?? state.currentAgent?.name,
            };
            return { messages };
          } else {
            // First content chunk — append a new model message
            return {
              messages: [
                ...state.messages,
                {
                  id: lastContentEventId ?? crypto.randomUUID(),
                  role: MessageRole.Model,
                  content: bufferedContent,
                  thinking: bufferedThinking || undefined,
                  agentName: state.currentAgent?.name,
                },
              ],
            };
          }
        });
      };

      const stopBuffering = () => {
        if (isBuffering && bufferedContent) {
          commitBufferedContent();
          isBuffering = false;
        }
      };

      set({ streamingBuffer: { startBuffering, stopBuffering } });

      const handler = (event: AgentStreamEvent) => {
        // The retry banner reflects the wait between the SDK's HTTP attempts. Once any
        // other stream event arrives (a new message, content chunk, error, etc.) the
        // retry window is over — clear it so the "Thinking..." line reverts. We leave
        // the banner untouched for RetryWarning itself (that's what's being rendered).
        if (event.type !== AgentEventType.RetryWarning && get().retryStatus) {
          get().setRetryStatus(null);
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
            // Reset buffer for the next assistant turn
            bufferedContent = '';
            bufferedThinking = '';
            lastContentEventId = null;

            if (event.content.type === 'text') {
              const text = event.content.text;
              const id = event.id;
              set((state) => ({
                messages: [
                  ...state.messages,
                  {
                    id,
                    role: MessageRole.User,
                    content: text,
                    agentName: state.currentAgent?.name,
                  },
                ],
              }));
            }
            break;
          case AgentEventType.Content:
            if (event.content.type === 'text') {
              const text = event.content.text;
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
            if (isBuffering && bufferedContent) {
              commitBufferedContent();
              isBuffering = false;
            }
            // Flush any pending batched content before adding tool message
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
              flushContentToStore();
            }
            // Reset buffer so the next Model message after this tool
            // doesn't repeat text from before the tool call.
            bufferedContent = '';
            bufferedThinking = '';
            lastContentEventId = null;

            set((state) => {
              const existingIndex = state.messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );

              let content: string;
              const diff = event.toolContent?.[0];
              if (diff) {
                const args = event.args as Record<string, unknown>;
                let command = 'create';
                if (args.oldStr !== undefined) {
                  command = 'strReplace';
                } else if (args.insertLine !== undefined || args.append) {
                  command = 'insert';
                }
                content = JSON.stringify({
                  command,
                  path: diff.path,
                  content: diff.newText,
                  oldStr: diff.oldText,
                  newStr: diff.newText,
                  insertLine: args.insertLine,
                });
              } else if (event.kind === 'edit') {
                const args = event.args as Record<string, unknown>;
                let command = 'create';
                if (args.oldStr !== undefined) {
                  command = 'strReplace';
                } else if (args.insertLine !== undefined || args.append) {
                  command = 'insert';
                }
                content = JSON.stringify({
                  command,
                  path: args.path,
                  content: args.text || args.content || '',
                  oldStr: args.oldStr,
                  newStr: args.newStr,
                  insertLine: args.insertLine,
                });
              } else {
                content = JSON.stringify(event.args);
              }

              if (existingIndex !== -1) {
                const existingMsg = state.messages[existingIndex];
                if (existingMsg && existingMsg.role === MessageRole.ToolUse) {
                  const hasNewContent =
                    Object.keys(event.args).length > 0 || event.toolContent;
                  if (hasNewContent) {
                    const messages = [...state.messages];
                    messages[existingIndex] = {
                      ...existingMsg,
                      content,
                      kind: event.kind || existingMsg.kind,
                      locations: event.locations || existingMsg.locations,
                    };
                    return { messages };
                  }
                }
                return state;
              }

              const isNotReady = NOT_READY_TOOLS.has(event.name);
              // Wipe previous subagent state when a new crew invocation starts
              let clearedMessages = state.messages;
              let clearedSessions = state.sessions;
              let clearedSessionMessages = state.sessionMessages;
              let clearedEventBuffer = state.sessionEventBuffer;
              if (SESSION_TOOL_NAMES.has(event.name)) {
                const staleNames = new Set<string>();
                const newSessions = new Map<string, AgentSession>();
                for (const [id, s] of state.sessions) {
                  if (s.type === 'ephemeral' && id !== state.sessionId) {
                    staleNames.add(s.name);
                  } else {
                    newSessions.set(id, s);
                  }
                }
                if (staleNames.size > 0) {
                  clearedSessions = newSessions;
                  clearedMessages = state.messages.filter(
                    (msg) =>
                      msg.role !== MessageRole.ToolUse ||
                      !msg.agentName ||
                      !staleNames.has(msg.agentName)
                  );
                  clearedSessionMessages = new Map(state.sessionMessages);
                  clearedEventBuffer = { ...state.sessionEventBuffer };
                  for (const [id, s] of state.sessions) {
                    if (s.type === 'ephemeral' && id !== state.sessionId) {
                      clearedSessionMessages.delete(id);
                      delete clearedEventBuffer[id];
                    }
                  }
                }
              }
              // Resolve agent name: use subagent session name if tool call is from a subagent
              const agentName = event.sessionId
                ? (state.sessions.get(event.sessionId)?.name ??
                  state.currentAgent?.name)
                : state.currentAgent?.name;
              return {
                sessions: clearedSessions,
                sessionMessages: clearedSessionMessages,
                sessionEventBuffer: clearedEventBuffer,
                messages: [
                  ...clearedMessages,
                  {
                    id: event.id,
                    role: MessageRole.ToolUse,
                    name: event.name,
                    kind: event.kind,
                    content,
                    locations: event.locations,
                    agentName,
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
                  messages[toolMsgIndex] = {
                    id: toolMsg.id,
                    role: MessageRole.ToolUse,
                    name: toolMsg.name,
                    kind: toolMsg.kind,
                    content: toolMsg.content,
                    isFinished: true,
                    status: toolMsg.status,
                    result: event.result,
                    locations: toolMsg.locations,
                    agentName: toolMsg.agentName,
                  };
                }
              }
              return { messages, liveOutputs: newLiveOutputs };
            });

            // Extract task state from task tool results
            extractTaskState(event, get);
            break;
          case AgentEventType.ApprovalRequest: {
            const { autoApproveCrewTools, sessionId: mainSessionId } = get();
            const isCrewApproval = !!(
              event.value.sessionId &&
              mainSessionId &&
              event.value.sessionId !== mainSessionId
            );
            if (autoApproveCrewTools && isCrewApproval) {
              const opt = event.value.permissionOptions.find(
                (o: { optionId: string }) => o.optionId === 'allow_once'
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
          case AgentEventType.ContextUsage:
            get().setContextUsage(event.percent);
            break;
          case AgentEventType.EffortUpdate:
            get().setCurrentEffort(event.effort);
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
            if (event.status === 'started') {
              set({ isCompacting: true, loadingMessage: null });
            } else if (event.status === 'completed') {
              set({ isCompacting: false, loadingMessage: null });
            } else if (event.status === 'failed') {
              set({ isCompacting: false, loadingMessage: null });
              get().showTransientAlert({
                message: `Compaction failed: ${event.error ?? 'unknown error'}`,
                status: 'error',
                autoHideMs: 5000,
              });
            }
            break;
          case AgentEventType.AuthError:
            {
              const guidance = getAuthErrorGuidance(event.errorType);
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
              });
            }
            break;
          case AgentEventType.SessionError:
            {
              const guidance = getSessionErrorGuidance(
                event.errorType,
                event.pid
              );
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
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
                if (!state.pendingOAuthServers.has(event.serverName))
                  return state;
                const updated = new Map(state.pendingOAuthServers);
                updated.delete(event.serverName);
                return { pendingOAuthServers: updated };
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
          case AgentEventType.HooksUpdate:
            // Update cached hooks list. If the panel is open, it will
            // re-render with the new data automatically.
            set({ hooksList: event.hooks });
            break;
        }
      };

      // TODO: Refactor createStreamEventHandler to return { handle, flush } instead of
      // monkey-patching flush onto the handler function and casting to any.
      // Attach flush for callers to commit remaining buffered content
      (handler as any).flush = () => {
        // Cancel any pending batched flush and commit immediately
        if (pendingContentFlush) {
          clearTimeout(pendingContentFlush);
          pendingContentFlush = null;
        }
        // flushContentToStore handles both creating new and updating
        // existing Model messages — no need to also call commitBufferedContent
        flushContentToStore();
        set({ streamingBuffer: { startBuffering: null, stopBuffering: null } });
      };

      return handler;
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
      (handler as any).flush?.();
    },

    cancelMessage: async () => {
      const { kiro, currentAbortController } = get();
      if (!kiro) return;
      let resolveCancelPromise: () => void;
      const cancelPromise = new Promise<void>((resolve) => {
        resolveCancelPromise = resolve;
      });
      set({ cancelInProgress: cancelPromise, wasCancelled: true });

      try {
        // Abort local stream first
        if (currentAbortController) {
          currentAbortController.abort();
          set({ currentAbortController: null });
        }

        // Cancel any pending approval
        get().cancelApproval();

        // Mark any unfinished tool uses as finished with cancelled status
        // immediately — before async calls. This stops spinners and prevents
        // a leak if kiro.cancel() is slow or throws.
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );

          if (hasUnfinishedToolCalls) {
            return {
              messages: state.messages.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? {
                      ...msg,
                      isFinished: true,
                      result: { status: 'cancelled' },
                    }
                  : msg
              ),
            };
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

        // Only show the alert when the queue is empty — if there are
        // queued messages the next one will start immediately and the
        // transient alert would just flash confusingly.
        if (get().queuedMessages.length === 0) {
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
        // Drain any queued messages now that isProcessing is cleared.
        await get().processQueue();
      }
    },

    setProcessing: (isProcessing) => set({ isProcessing }),
    setAgentError: (agentError, guidance) =>
      set({ agentError, agentErrorGuidance: guidance ?? null }),
    setCurrentModel: (currentModel) => set({ currentModel }),
    setCurrentEffort: (currentEffort) => set({ currentEffort }),
    setCurrentAgent: (agent, options) => {
      const prevAgent = get().currentAgent;
      // The artifact-generation card belongs to the spec workflow's
      // active agent. Switching agents (e.g. spec → kiro_planner →
      // anything else) means any in-flight card is stale. Clear it.
      // We only update the field when there's actually an agent change
      // and an entry to clear, so no-op rerenders are avoided.
      const isAgentChanging = prevAgent?.name !== agent?.name;
      const hasGenerating = get().artifactGenerating !== null;
      set({
        currentAgent: agent ? { name: agent.name } : null,
        ...(isAgentChanging && hasGenerating
          ? { artifactGenerating: null }
          : {}),
      });

      // Trigger plan quality survey when switching away from planner
      // (the handoff moment — plan was presented and user approved it).
      if (
        prevAgent?.name === 'kiro_planner' &&
        agent?.name &&
        agent.name !== 'kiro_planner'
      ) {
        queueMicrotask(() => get().triggerPlanSurvey());
      }

      if (agent?.welcomeMessage && !options?.suppressWelcome) {
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: generateMessageId(),
              role: MessageRole.Model,
              content: agent.welcomeMessage!,
              agentName: agent.name,
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
      if (event.type === AgentEventType.EffortUpdate) {
        get().setCurrentEffort(event.effort);
        return;
      }
      if (event.type !== AgentEventType.CompactionStatus) return;
      if (event.status === 'started') {
        set((state) => ({
          isCompacting: true,
          isProcessing: true,
          messages: [
            ...state.messages,
            { id: crypto.randomUUID(), role: MessageRole.User, content: '' },
          ],
        }));
      } else if (event.status === 'completed') {
        const summary = event.summary;
        set((state) => {
          const messages = [...state.messages];
          if (summary) {
            messages.push({
              id: crypto.randomUUID(),
              role: MessageRole.Model,
              content: summary,
            });
          }
          return {
            isCompacting: false,
            isProcessing: false,
            transientAlert: null,
            messages,
          };
        });
        await get().processQueue();
      } else if (event.status === 'failed') {
        set({ isCompacting: false, isProcessing: false });
        get().showTransientAlert({
          message: `Compaction failed: ${event.error ?? 'unknown error'}`,
          status: 'error',
          autoHideMs: 5000,
        });
        await get().processQueue();
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
        const isRejected =
          optionId === ApprovalOptionId.RejectOnce ||
          optionId === ApprovalOptionId.RejectAlways;
        const isTrust =
          optionId === ApprovalOptionId.AllowAlways && !_meta?.trustOption;

        // When trusting a tool, cascade to all pending approvals of the same tool
        let cascadeApprovals: ApprovalRequestInfo[] = [];
        if (isTrust) {
          const toolMsg = messages.find(
            (m) => m.role === MessageRole.ToolUse && m.id === toolCallId
          );
          if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
            const trustedName = toolMsg.name;
            cascadeApprovals = approvalQueue.filter((a) => {
              if (a === approval) return false;
              const msg = messages.find(
                (m) =>
                  m.role === MessageRole.ToolUse &&
                  m.id === a.toolCall.toolCallId
              );
              return (
                msg &&
                msg.role === MessageRole.ToolUse &&
                msg.name === trustedName
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

        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && msg.id === toolCallId) {
              return {
                ...msg,
                status: isRejected
                  ? ToolUseStatus.Rejected
                  : ToolUseStatus.Approved,
                isFinished: isRejected ? true : msg.isFinished,
              };
            }
            if (msg.role === MessageRole.ToolUse && cascadeIds.has(msg.id)) {
              return { ...msg, status: ToolUseStatus.Approved };
            }
            return msg;
          }),
          approvalQueue: remainingQueue,
          pendingApproval:
            state.pendingApproval === approval ||
            cascadeIds.has(state.pendingApproval?.toolCall.toolCallId ?? '')
              ? nextApproval
              : state.pendingApproval,
          approvalMode: 'dropdown',
        }));

        approval.resolve({
          outcome: 'selected',
          optionId,
          _meta,
        });

        // Auto-resolve cascaded approvals with allow_once (trust is already applied)
        for (const cascaded of cascadeApprovals) {
          cascaded.resolve({
            outcome: 'selected',
            optionId: ApprovalOptionId.AllowOnce,
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

        // Mark all cancelled tool calls as finished
        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && cancelIds.has(msg.id)) {
              return {
                ...msg,
                isFinished: true,
                result: { status: 'cancelled' as const },
              };
            }
            return msg;
          }),
        }));

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

    setApprovalMode: (mode) => set({ approvalMode: mode }),

    setAutoApproveCrewTools: (value) => set({ autoApproveCrewTools: value }),
    setFocusedCrewIndex: (index) => set({ focusedCrewIndex: index }),

    // Keeps last turn visible for /clear
    clearMessages: () => {
      const msgs = get().messages;
      if (msgs.length < 2) return;

      // Find the last user message to keep the entire last turn
      let lastUserIndex = -1;
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i]?.role === MessageRole.User) {
          lastUserIndex = i;
          break;
        }
      }

      if (lastUserIndex === -1) return;
      set({ messages: msgs.slice(lastUserIndex) });
    },

    resetMessages: () => {
      set({ messages: [] });
    },

    setSlashCommands: (commands: SlashCommand[]) => {
      set((state) => {
        const localCommands = state.slashCommands.filter(
          (cmd) => cmd.source === 'local' || cmd.meta?.type === 'prompt'
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

      await executeCommandWithArg(cmdName, arg, ctx);
    },

    queueMessage: (content: string) => {
      const trimmed = content.trim();
      if (!trimmed) return;
      set((state) => ({ queuedMessages: [...state.queuedMessages, trimmed] }));
    },

    processQueue: async () => {
      const { cancelInProgress, isProcessing } = get();
      if (cancelInProgress) {
        await cancelInProgress;
      }

      // Don't drain if already processing (prevents double-send races)
      if (isProcessing) return;

      const { queuedMessages, editingQueueIndex } = get();
      const nextMessage = queuedMessages[0];
      if (!nextMessage) return;

      // Adjust editing index since we're removing index 0
      let newEditingIndex = editingQueueIndex;
      if (newEditingIndex != null) {
        if (newEditingIndex === 0) {
          newEditingIndex = null;
        } else {
          newEditingIndex = newEditingIndex - 1;
        }
      }
      const stoppedEditing =
        editingQueueIndex != null && newEditingIndex == null;

      set((state) => ({
        queuedMessages: state.queuedMessages.slice(1),
        editingQueueIndex: newEditingIndex,
        commandInputValue: stoppedEditing ? '' : state.commandInputValue,
      }));
      await get().sendMessage(nextMessage);
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
      set((state) => {
        const newMessages = state.queuedMessages.filter((_, i) => i !== index);
        // Adjust editing index: clear if the edited item was removed, shift down
        // if an earlier item was removed
        let newEditingIndex = state.editingQueueIndex;
        if (newEditingIndex != null) {
          if (newEditingIndex === index) {
            newEditingIndex = null;
          } else if (newEditingIndex > index) {
            newEditingIndex = newEditingIndex - 1;
          }
        }
        // Clear the input field if we just exited editing mode
        const wasEditing = state.editingQueueIndex != null;
        const stoppedEditing = wasEditing && newEditingIndex == null;
        return {
          queuedMessages: newMessages,
          editingQueueIndex: newEditingIndex,
          commandInputValue: stoppedEditing ? '' : state.commandInputValue,
        };
      });
    },

    replaceQueuedMessage: (index: number, content: string) => {
      set((state) => {
        if (index < 0 || index >= state.queuedMessages.length) {
          return { editingQueueIndex: null };
        }
        const updated = [...state.queuedMessages];
        updated[index] = content;
        return { queuedMessages: updated, editingQueueIndex: null };
      });
    },

    startEditingQueue: (index: number) => {
      const msg = get().queuedMessages[index];
      if (msg == null) return;
      // Load the message text into the command input so PromptInput picks it up
      set({ editingQueueIndex: index, commandInputValue: msg });
    },

    cancelEditingQueue: () => {
      set({ editingQueueIndex: null, commandInputValue: '' });
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
      // generation state after switching to vibe mode (or away from
      // spec mode in general). The open artifact-view panel is left
      // alone — the user explicitly opened it and dismisses with Q.
      set((state) =>
        state.artifactGenerating === null
          ? { mode }
          : { mode, artifactGenerating: null }
      );
    },

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
        if (staleIds.length === 0) return { sessions: newSessions };
        // Also clear stale messages, event buffers, and inbox messages
        const staleNames = new Set(
          staleIds.map((id) => state.sessions.get(id)?.name).filter(Boolean)
        );
        const newMessages = new Map(state.sessionMessages);
        const newBuffer = { ...state.sessionEventBuffer };
        for (const id of staleIds) {
          newMessages.delete(id);
          delete newBuffer[id];
        }
        return {
          sessions: newSessions,
          sessionMessages: newMessages,
          sessionEventBuffer: newBuffer,
          messages: state.messages.filter(
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
        const newMessages = new Map(state.sessionMessages);
        newSessions.delete(id);
        newMessages.delete(id);
        // Clean up event buffer for terminated session
        const newBuffer = { ...state.sessionEventBuffer };
        delete newBuffer[id];
        return {
          sessions: newSessions,
          sessionMessages: newMessages,
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
      const { approvalQueue, pendingApproval } = get();
      // Cancel pending approvals for this session
      const sessionApprovals = approvalQueue.filter(
        (a) => a.sessionId === sessionId
      );
      for (const a of sessionApprovals) {
        a.resolve({ outcome: 'cancelled' });
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

    addMessage: (sessionId, message) =>
      set((state) => {
        const newMessages = new Map(state.sessionMessages);
        const existing = newMessages.get(sessionId) || [];
        newMessages.set(sessionId, [...existing, message]);
        return { sessionMessages: newMessages };
      }),

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
      set({ loadingMessage: message });
    },

    // Context usage actions
    setContextUsage: (percent) => {
      logger.debug('[context-usage] setContextUsage called, percent=', percent);
      set({ contextUsagePercent: percent });
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

    setShowToolsPanel: (show, tools = []) => {
      set({ showToolsPanel: show, toolsList: tools });
    },
    setShowStatsPanel: (show, stats = [], summary = null) => {
      set({ showStatsPanel: show, statsList: stats, statsSummary: summary });
    },

    setShowHooksPanel: (show, hooks = []) => {
      set({ showHooksPanel: show, hooksList: hooks });
    },

    setShowKeybindingsPanel: (show) => {
      set({ showKeybindingsPanel: show });
    },

    setShowDisplaySettingsPanel: (show) => {
      set({ showDisplaySettingsPanel: show });
    },

    setSettingsReturnOnEscape: (value) => {
      set({ settingsReturnOnEscape: value });
    },

    /**
     * Re-open the top-level /settings menu. Used by ESC handlers when a
     * /settings-derived overlay is dismissed: we go back one level rather
     * than close everything. Bypasses handleUserInput() so we don't go
     * through the whole input-reset pipeline (which can race with the
     * concurrent overlay-close side-effects).
     */
    reopenSettingsMenu: () => {
      const settingsCmd = get().slashCommands.find(
        (c) => c.name === '/settings'
      );
      if (!settingsCmd) return;
      set({ activeCommand: buildSettingsActiveCommand(settingsCmd) });
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
        editingQueueIndex: state.activityTrayExpanded
          ? null
          : state.editingQueueIndex,
      }));
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

      // Answering any survey sets the cooldown for all (shared 90-day cooldown).
      markSurveyCompleted(SESSION_FEEDBACK_SURVEY.id);
      markSurveyCompleted(PLAN_QUALITY_SURVEY.id);
      markSurveyCompleted(IMPLEMENT_PLAN_SURVEY.id);

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
        isInternal: !!process.env.KIRO_INTERNAL,
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
      // Dismissing any survey sets the cooldown for all (shared 90-day cooldown).
      markSurveyDismissed(SESSION_FEEDBACK_SURVEY.id);
      markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
      markSurveyDismissed(IMPLEMENT_PLAN_SURVEY.id);

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
      if (state.showSurveyPanel || state.transientAlert) return;

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

    // Main orchestrator
    handleUserInput: async (input: string) => {
      const trimmed = input.trim();
      const hasPendingImages = get().pendingImages.length > 0;
      if (!trimmed && !hasPendingImages) return;

      const state = get();
      state.resetExitSequence();

      // Queue if processing or not yet initialized — but always allow /quit and /exit through
      if (state.isProcessing || !state.isInitialized) {
        const lower = trimmed.toLowerCase();
        if (lower === '/quit' || lower === '/exit') {
          state.clearInput();
          state.kiro.close();
          state.onExit?.();
          process.exit(0);
        }
        // TODO: support queuing non-interactive slash commands (e.g. /clear, /compact)
        //       that don't require UI interaction to complete
        if (trimmed.startsWith('/')) {
          state.showTransientAlert({
            message:
              "Slash commands can't be queued — wait for the current task to finish",
            status: 'warning',
            autoHideMs: 4000,
          });
          state.clearInput();
          return;
        }
        if (trimmed.startsWith('!')) {
          state.showTransientAlert({
            message:
              "Shell escape commands can't be queued — wait for the current task to finish",
            status: 'warning',
            autoHideMs: 4000,
          });
          state.clearInput();
          return;
        }
        state.queueMessage(trimmed);
        state.clearInput();
        return;
      }

      // Clear all UI state before processing any input
      const hadSurveyPrompt = !!state.surveyPrompt;
      set({
        activeCommand: null,
        showContextBreakdown: false,
        showHelpPanel: false,
        showUsagePanel: false,
        showRewindExplorer: false,
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
        // Shared cooldown: dismissing any survey sets cooldown for all.
        markSurveyDismissed(SESSION_FEEDBACK_SURVEY.id);
        markSurveyDismissed(PLAN_QUALITY_SURVEY.id);
        markSurveyDismissed(IMPLEMENT_PLAN_SURVEY.id);
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

      // Handle slash commands via command registry
      if (trimmed.startsWith('/')) {
        CommandHistory.getInstance().add(trimmed);
        const ctx: CommandContext = buildCommandContext(state, set, get);
        const handled = await executeCommand(trimmed, ctx);
        if (handled) return;
        // Not a recognized command — could be a file path like /Users/...
        // Strip the leading "/" only for file paths to match V1 behavior
        // (leaving it confuses the LLM's path extraction for tool calls).
        // For other inputs like "// hello world", send as-is.
        const afterSlash = trimmed.slice(1);
        const isFilePath =
          afterSlash.length > 0 &&
          afterSlash[0] !== '/' &&
          afterSlash[0] !== ' ';
        const messageText = isFilePath ? afterSlash : trimmed;
        await state.sendMessage(messageText, undefined, trimmed);
        return;
      }

      // Handle shell escape commands
      if (trimmed.startsWith('!')) {
        const command = trimmed.slice(1).trim();
        if (!command) return;

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

            // Finalize
            const finalContent = accumulated || '(no output)';
            const exitSuffix =
              result.exitCode !== 0
                ? `\n\n[exit code: ${result.exitCode}]`
                : '';
            set((state) => ({
              messages: state.messages.map((msg) =>
                msg.id === outputMsgId
                  ? { ...msg, content: finalContent + exitSuffix }
                  : msg
              ),
            }));
          } finally {
            set({
              isProcessing: false,
              isShellEscape: false,
              _shellEscapeWriter: null,
              currentAbortController: null,
            });
          }
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
    const key = `${onAltScreen}|${state.agentError ?? ''}|${state.pendingApproval != null}|${state.isProcessing}|${state.isCompacting}|${state.contextUsagePercent}`;
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

  // Terminal notifications (bell / OSC 9) on turn-end and tool approval.
  let prevProcessing = false;
  let prevApproval: unknown = null;

  store.subscribe((state) => {
    const enabled = state.settings?.[Settings.CHAT_ENABLE_NOTIFICATIONS];
    const wasProcessing = prevProcessing;
    const hadApproval = prevApproval;
    prevProcessing = state.isProcessing;
    prevApproval = state.pendingApproval;

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
  });

  return store;
};
