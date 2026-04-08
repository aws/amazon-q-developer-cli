import { createStore, useStore } from 'zustand';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { Kiro } from '../kiro';
import chalk from 'chalk';
import { kiroSafe } from '../theme/kiroSafe';
import { createContext, useContext } from 'react';
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

export interface HookInfo {
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

import {
  executeCommand,
  executeCommandWithArg,
  type CommandContext,
} from '../commands/index.js';
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
import { CommandHistory } from '../utils/command-history.js';
import { Settings } from '../constants/settings.js';
import {
  resolveNotificationMethod,
  playNotification,
} from '../utils/notification.js';

export enum MessageRole {
  User = 'user',
  Model = 'model',
  ToolUse = 'tool_use',
  System = 'system',
}

// Helper to generate unique message IDs
const generateMessageId = () => crypto.randomUUID();

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
      liveOutput?: string;
    }
  | { id: string; role: MessageRole.System; content: string; success: boolean };

export interface SlashCommand extends AvailableCommand {
  source: 'local' | 'backend';
}

export interface ActiveCommand {
  command: SlashCommand;
  options: CommandOption[];
}

export interface TransientAlert {
  message: string;
  status: StatusType;
  autoHideMs?: number;
  /** Optional keyboard shortcut action shown in the alert */
  action?: { label: string; key: string; onAction: () => void };
}

export type InitError =
  | { type: 'mcp_failure'; serverName: string; error: string }
  | { type: 'agent_not_found'; requestedAgent: string; fallbackAgent: string }
  | { type: 'agent_config_error'; path?: string; error: string }
  | { type: 'model_not_found'; requestedModel: string; fallbackModel: string };

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
  const modelNotFound = errors.filter((e) => e.type === 'model_not_found');

  const parts: string[] = [];

  // Agent not found
  if (agentNotFound.length > 0) {
    const e = agentNotFound[0]!;
    parts.push(
      `agent "${e.requestedAgent}" not found, using "${e.fallbackAgent}"`
    );
  }

  // Model not found
  if (modelNotFound.length > 0) {
    const e = modelNotFound[0]!;
    parts.push(
      `model "${e.requestedModel}" not found, using "${e.fallbackModel}"`
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
  setCurrentAgent: (
    agent: { name: string; welcomeMessage?: string } | null
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
  showTransientAlert: (alert: TransientAlert) => void;
  dismissTransientAlert: () => void;
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
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string
  ) => void;
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  setShowHooksPanel: (show: boolean, hooks?: HookInfo[]) => void;
  setShowKnowledgePanel: (
    show: boolean,
    entries?: KnowledgeEntry[],
    status?: string
  ) => void;
  setShowCodePanel: (show: boolean, data?: CodePanelData) => void;

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

  // Theme diff hex getter (set by ThemeProvider bridge)
  _themeDiffHexGetter:
    | (() => {
        added: { background: string; bar: string; highlight: string };
        removed: { background: string; bar: string; highlight: string };
      })
    | null;
  registerThemeDiffHexGetter: (
    getter: () => {
      added: { background: string; bar: string; highlight: string };
      removed: { background: string; bar: string; highlight: string };
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

  // Main orchestrator
  handleUserInput: (input: string) => Promise<void>;
}

export const AppStoreContext = createContext<AppStoreApi | null>(null);

export type AppStoreApi = ReturnType<typeof createAppStore>;

export interface AppState {
  // Chat state
  messages: MessageType[];
  queuedMessages: string[];
  editingQueueIndex: number | null;
  slashCommands: SlashCommand[];
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
  agentError: string | null;
  agentErrorGuidance: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  approvalQueue: ApprovalRequestInfo[];
  approvalMode: 'dropdown' | 'drill-in';
  autoApproveCrewTools: boolean;
  focusedCrewIndex: number;
  setFocusedCrewIndex: (index: number) => void;
  currentModel: { id: string; name: string } | null;
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
  transientAlert: TransientAlert | null;
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
  showHelpPanel: boolean;
  helpCommands: Array<{
    name: string;
    description: string;
    usage: string;
    subcommands?: string[];
  }>;
  showMcpPanel: boolean;
  mcpServers: McpServerInfo[];
  pendingOAuthServers: Map<string, string>; // serverName → oauthUrl
  initErrors: InitError[];
  mcpMode: string;
  showToolsPanel: boolean;
  toolsList: ToolInfo[];
  showHooksPanel: boolean;
  hooksList: HookInfo[];
  showKnowledgePanel: boolean;
  knowledgeEntries: KnowledgeEntry[];
  knowledgeStatus: string | null;
  showCodePanel: boolean;
  codeData: CodePanelData | null;
  codeIntelligenceActive: boolean;

  // Task management state
  tasks: TaskItem[];
  activityTrayExpanded: boolean;

  // Abort controller for current stream
  currentAbortController: AbortController | null;
  cancelInProgress: Promise<void> | null;
  isShellEscape: boolean;

  // Initialization state — true once the ACP session is ready
  isInitialized: boolean;

  // Non-interactive mode
  noInteractive: boolean;

  // Streaming buffer control (typed properly instead of `any`)
  streamingBuffer: {
    startBuffering: (() => void) | null;
    stopBuffering: (() => void) | null;
  };
}

interface AppStoreProps {
  kiro: Kiro;
  noInteractive?: boolean;
  initialInput?: string;
}

interface AppStoreProps {
  kiro: Kiro;
  noInteractive?: boolean;
  initialInput?: string;
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

export const createAppStore = (props: AppStoreProps) => {
  const store = createStore<AppState & AppActions>((set, get) => ({
    // Initial state
    messages: [],
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
        name: '/theme',
        description: 'Select a theme that looks best for your terminal',
        source: 'local' as const,
        meta: { local: true },
      },
    ], // Backend sends all commands via CommandsUpdate
    prompts: [],
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    isCompacting: false,
    agentError: null,
    agentErrorGuidance: null,
    pendingApproval: null,
    approvalQueue: [],
    approvalMode: 'dropdown',
    autoApproveCrewTools: false,
    focusedCrewIndex: 0,
    currentModel: null,
    currentAgent: null,
    previousAgentName: null,
    settings: null,

    activeCommand: null,
    commandInputValue: '',
    activeTrigger: null,
    filePickerHasResults: false,
    promptHint: null,
    commandShadowText: null,

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
    transientAlert: null,
    loadingMessage: null as string | null,
    toolOutputsExpanded: false,
    hasExpandableToolOutputs: false,

    contextUsagePercent: null,
    lastTurnTokens: null,
    turnSummaries: new Map(),
    showContextBreakdown: false,
    contextBreakdown: null,
    showHelpPanel: false,
    helpCommands: [],
    showUsagePanel: false,
    usageData: null,
    showMcpPanel: false,
    mcpServers: [],
    pendingOAuthServers: new Map(),
    initErrors: [],
    mcpMode: 'list',
    showToolsPanel: false,
    toolsList: [],
    showHooksPanel: false,
    hooksList: [],
    showKnowledgePanel: false,
    knowledgeEntries: [],
    knowledgeStatus: null,
    showCodePanel: false,
    codeData: null,
    codeIntelligenceActive: existsSync(
      join(process.cwd(), '.kiro', 'settings', 'lsp.json')
    ),
    attachedFiles: [],
    _userColorsSetter: null,
    _themeDiffHexGetter: null,
    _autoPreviewGetter: null,
    themePreview: null,
    pendingFileAttachment: null,
    pendingImages: [],
    currentAbortController: null,
    cancelInProgress: null,
    isShellEscape: false,
    streamingBuffer: { startBuffering: null, stopBuffering: null },

    // Task management
    tasks: [],
    activityTrayExpanded: false,

    isInitialized: false,
    noInteractive: props.noInteractive ?? false,

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
          autoApproveCrewTools: false,
          messages: [...state.messages, userMessage],
          attachedFiles: [], // Clear attachments after sending
          pendingImages: [], // Clear pending images after sending
          // Reset expandable content flag for new turn (expanded state persists)
          hasExpandableToolOutputs: false,
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
        // Extract error message
        let errorMessage = 'Unknown error';
        if (error instanceof Error) {
          errorMessage = error.message || error.name || 'Unknown error';
        } else if (typeof error === 'string') {
          errorMessage = error;
        } else if (typeof error === 'object' && error !== null) {
          const errObj = error as Record<string, unknown>;
          if (typeof errObj.message === 'string' && errObj.message) {
            errorMessage = errObj.message;
          } else if (
            typeof errObj.error === 'object' &&
            errObj.error !== null
          ) {
            const innerErr = errObj.error as Record<string, unknown>;
            if (typeof innerErr.message === 'string' && innerErr.message) {
              errorMessage = innerErr.message;
            }
          }
        }

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
        const displayMessage = simplifyErrorMessage(errorMessage);

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

      // Batching: accumulate content chunks and flush to the store
      // on a timer so Ink's render loop isn't starved by rapid-fire
      // synchronous set() calls from the ACP notification handler.
      let pendingContentFlush: ReturnType<typeof setTimeout> | null = null;
      let lastContentEventId: string | null = null;

      const startBuffering = () => {
        isBuffering = true;
      };

      const commitBufferedContent = () => {
        if (!bufferedContent) return;
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
              };
              return { messages };
            }
          }
          return {};
        });
      };

      const flushContentToStore = () => {
        pendingContentFlush = null;
        if (!bufferedContent) return;

        set((state) => {
          const lastMsg = state.messages[state.messages.length - 1];
          if (lastMsg?.role === MessageRole.Model) {
            // Update existing model message: single array copy, direct index write
            const messages = [...state.messages];
            messages[messages.length - 1] = {
              id: lastMsg.id,
              role: MessageRole.Model,
              content: bufferedContent,
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
              set((state) => {
                const toolMsgIndex = state.messages.findIndex(
                  (msg) =>
                    msg.role === MessageRole.ToolUse && msg.id === event.id
                );
                if (toolMsgIndex !== -1) {
                  const toolMsg = state.messages[toolMsgIndex];
                  if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
                    const messages = [...state.messages];
                    messages[toolMsgIndex] = {
                      id: toolMsg.id,
                      name: toolMsg.name,
                      kind: toolMsg.kind,
                      role: MessageRole.ToolUse,
                      content: toolMsg.content + text,
                      locations: toolMsg.locations,
                      agentName: toolMsg.agentName,
                    };
                    return { messages };
                  }
                }
                return state;
              });
            }
            break;
          case AgentEventType.ToolCallFinished:
            set((state) => {
              const messages = [...state.messages];
              const toolMsgIndex = messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );
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
              return { messages };
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
                  status: 'error',
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
          case AgentEventType.AgentSwitched:
            get().setCurrentAgent({
              name: event.agentName,
              welcomeMessage: event.welcomeMessage,
            });
            if (event.previousAgentName) {
              set({ previousAgentName: event.previousAgentName });
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
                  status: 'error',
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
                  status: 'error',
                  autoHideMs: 8000,
                });
              }
            }
            break;
          case AgentEventType.TurnSummary:
            // Handled by global handleTurnSummaryEvent, not here
            break;
          case AgentEventType.ModelNotFound:
            {
              const updated = [
                ...get().initErrors,
                {
                  type: 'model_not_found' as const,
                  requestedModel: event.requestedModel,
                  fallbackModel: event.fallbackModel,
                },
              ];
              set({ initErrors: updated });
              const message = summarizeInitErrors(updated);
              if (message) {
                get().showTransientAlert({
                  message,
                  status: 'error',
                  autoHideMs: 8000,
                });
              }
            }
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
      set({ cancelInProgress: cancelPromise });

      try {
        // Abort local stream first
        if (currentAbortController) {
          currentAbortController.abort();
          set({ currentAbortController: null });
        }

        // Cancel any pending approval
        get().cancelApproval();

        // Terminate any active crew sessions so subagents don't keep running
        if (get().sessions.size > 0) {
          await get().terminateAllCrewSessions();
        }

        // Then notify backend — this must complete before a new prompt
        // can be sent, otherwise the backend rejects with
        // "Prompt already in progress".
        await kiro.cancel();

        // Mark any unfinished tool uses as finished with cancelled status.
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
        set({ isProcessing: false, currentAbortController: null });
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
    setCurrentAgent: (agent) => {
      set({ currentAgent: agent ? { name: agent.name } : null });
      if (agent?.welcomeMessage) {
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
        logger.info(
          '[context-usage] ContextUsage event in compactionHandler, percent=',
          event.percent
        );
        get().setContextUsage(event.percent);
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
          return { isCompacting: false, isProcessing: false, messages };
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
      const { pendingApproval, approvalQueue } = get();
      const approval = target ?? pendingApproval;
      if (approval) {
        const toolCallId = approval.toolCall.toolCallId;
        const isRejected =
          optionId === ApprovalOptionId.RejectOnce ||
          optionId === ApprovalOptionId.RejectAlways;

        // Update the tool call status based on user response
        const remainingQueue = approvalQueue.filter((a) => a !== approval);
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
            return msg;
          }),
          approvalQueue: remainingQueue,
          pendingApproval:
            state.pendingApproval === approval
              ? nextApproval
              : state.pendingApproval,
          approvalMode: 'dropdown',
        }));

        approval.resolve({
          outcome: 'selected',
          optionId,
          _meta,
        });
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

        // Mark the tool call as cancelled
        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && msg.id === toolCallId) {
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
      const ctx: CommandContext = {
        kiro: state.kiro,
        slashCommands: state.slashCommands,
        showAlert: (message, status, autoHideMs = 3000) =>
          state.showTransientAlert({ message, status, autoHideMs }),
        setLoadingMessage: state.setLoadingMessage,
        setActiveCommand: state.setActiveCommand,
        setCurrentModel: state.setCurrentModel,
        setCurrentAgent: state.setCurrentAgent,
        setContextUsage: state.setContextUsage,
        setShowContextBreakdown: state.setShowContextBreakdown,
        setShowHelpPanel: state.setShowHelpPanel,
        setShowUsagePanel: state.setShowUsagePanel,
        setShowMcpPanel: state.setShowMcpPanel,
        setShowToolsPanel: state.setShowToolsPanel,
        setShowHooksPanel: state.setShowHooksPanel,
        setShowKnowledgePanel: state.setShowKnowledgePanel,
        setShowCodePanel: state.setShowCodePanel,
        clearMessages: state.clearMessages,
        resetMessages: state.resetMessages,
        sendMessage: state.sendMessage,
        createStreamEventHandler: state.createStreamEventHandler,
        setSessionId: (id: string | null) =>
          set({ sessionId: id, initErrors: [] }),
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
            showMcpPanel: false,
            showToolsPanel: false,
            showHooksPanel: false,
            showKnowledgePanel: false,
            showCodePanel: false,
            contextBreakdown: null,
            usageData: null,
            codeData: null,
          }),
        getMessages: () => get().messages,
        setUserColors: (prompt?: any, response?: any, diff?: any) => {
          const setter = get()._userColorsSetter;
          if (setter) setter(prompt, response, diff);
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
              background: d.added.background.truecolor ?? '',
              bar: d.added.bar.truecolor ?? '',
              highlight: d.added.highlight.truecolor ?? '',
            },
            removed: {
              background: d.removed.background.truecolor ?? '',
              bar: d.removed.bar.truecolor ?? '',
              highlight: d.removed.highlight.truecolor ?? '',
            },
          };
        },
        getAutoPreview: () => {
          const getter = get()._autoPreviewGetter;
          return getter ? getter() : '';
        },
      };

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

      const { queuedMessages, editingQueueIndex, tasks } = get();
      const nextMessage = queuedMessages[0];
      if (!nextMessage) return;

      // Don't drain the queue while there are pending tasks — let the agent
      // finish its task list first.  Queued messages will be sent once all
      // tasks are completed (or if there are no tasks at all).
      const hasPendingTasks = tasks.some((t) => t.status === 'pending');
      if (hasPendingTasks) return;

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
    setMode: (mode) => set({ mode }),

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

        return { exitSequence: newSequence, exitTimer: timer };
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

    showTransientAlert: (alert) => {
      set({ transientAlert: alert });
    },

    dismissTransientAlert: () => {
      set({ transientAlert: null });
    },

    setLoadingMessage: (message) => {
      set({ loadingMessage: message });
    },

    // Context usage actions
    setContextUsage: (percent) => {
      logger.info('[context-usage] setContextUsage called, percent=', percent);
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

    setShowHelpPanel: (show, commands = []) => {
      set({ showHelpPanel: show, helpCommands: commands });
    },

    setShowUsagePanel: (show, data) => {
      set({ showUsagePanel: show, usageData: data ?? null });
    },

    setShowMcpPanel: (show, servers = [], mode = 'list') => {
      set({ showMcpPanel: show, mcpServers: servers, mcpMode: mode });
    },

    setShowToolsPanel: (show, tools = []) => {
      set({ showToolsPanel: show, toolsList: tools });
    },

    setShowHooksPanel: (show, hooks = []) => {
      set({ showHooksPanel: show, hooksList: hooks });
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
      set({ tasks });
      // If all tasks are now completed, drain any queued messages
      const allDone =
        tasks.length > 0 && tasks.every((t) => t.status === 'completed');
      if (allDone && !get().isProcessing) {
        get().processQueue();
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

    setHasExpandableToolOutputs: (has: boolean) => {
      set({ hasExpandableToolOutputs: has });
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
        state.queueMessage(trimmed);
        state.clearInput();
        return;
      }

      // Clear all UI state before processing any input
      set({
        activeCommand: null,
        showContextBreakdown: false,
        showHelpPanel: false,
        showUsagePanel: false,
        commandInputValue: '',
        activeTrigger: null,
        promptHint: null,
        commandShadowText: null,
      });
      state.clearInput();

      // Handle slash commands via command registry
      if (trimmed.startsWith('/')) {
        CommandHistory.getInstance().add(trimmed);
        const ctx: CommandContext = {
          kiro: state.kiro,
          slashCommands: state.slashCommands,
          showAlert: (message, status, autoHideMs = 3000) =>
            state.showTransientAlert({ message, status, autoHideMs }),
          setLoadingMessage: state.setLoadingMessage,
          setActiveCommand: state.setActiveCommand,
          setCurrentModel: state.setCurrentModel,
          setCurrentAgent: state.setCurrentAgent,
          setContextUsage: state.setContextUsage,
          setShowContextBreakdown: state.setShowContextBreakdown,
          setShowHelpPanel: state.setShowHelpPanel,
          setShowUsagePanel: state.setShowUsagePanel,
          setShowMcpPanel: state.setShowMcpPanel,
          setShowToolsPanel: state.setShowToolsPanel,
          setShowHooksPanel: state.setShowHooksPanel,
          setShowKnowledgePanel: state.setShowKnowledgePanel,
          setShowCodePanel: state.setShowCodePanel,
          clearMessages: state.clearMessages,
          resetMessages: state.resetMessages,
          sendMessage: state.sendMessage,
          createStreamEventHandler: state.createStreamEventHandler,
          setSessionId: (id: string | null) =>
            set({ sessionId: id, initErrors: [] }),
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
              showMcpPanel: false,
              showToolsPanel: false,
              showHooksPanel: false,
              showKnowledgePanel: false,
              contextBreakdown: null,
              usageData: null,
            }),
          getMessages: () => get().messages,
          setUserColors: (prompt?: any, response?: any, diff?: any) => {
            const setter = get()._userColorsSetter;
            if (setter) setter(prompt, response, diff);
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
                background: d.added.background.truecolor ?? '',
                bar: d.added.bar.truecolor ?? '',
                highlight: d.added.highlight.truecolor ?? '',
              },
              removed: {
                background: d.removed.background.truecolor ?? '',
                bar: d.removed.bar.truecolor ?? '',
                highlight: d.removed.highlight.truecolor ?? '',
              },
            };
          },
          getAutoPreview: () => {
            const getter = get()._autoPreviewGetter;
            return getter ? getter() : '';
          },
        };
        const handled = await executeCommand(trimmed, ctx);
        if (handled) return;
        // Not a command (e.g. file path like /Users/...) — fall through to send as message
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
          // TTY mode: direct terminal access
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
          // Streaming mode: pipe output into conversation
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

          const { promise, kill } = executeShellEscapeStreaming(
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
          set({ currentAbortController: abortController });

          const result = await promise;

          // Finalize
          const finalContent = accumulated || '(no output)';
          const exitSuffix =
            result.exitCode !== 0 ? `\n\n[exit code: ${result.exitCode}]` : '';
          set((state) => ({
            isProcessing: false,
            isShellEscape: false,
            currentAbortController: null,
            messages: state.messages.map((msg) =>
              msg.id === outputMsgId
                ? { ...msg, content: finalContent + exitSuffix }
                : msg
            ),
          }));
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

    if (!enabled) return;

    const method = resolveNotificationMethod(
      state.settings?.[Settings.CHAT_NOTIFICATION_METHOD] as string | undefined
    );
    if (!method) return;

    // Turn completed cleanly (no error)
    if (wasProcessing && !state.isProcessing && !state.agentError) {
      playNotification(method, 'Response complete');
    }

    // Tool approval requested
    if (!hadApproval && state.pendingApproval) {
      playNotification(method, 'Permission required');
    }
  });

  return store;
};
