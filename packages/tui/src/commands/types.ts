/**
 * Types for the slash command handler system.
 */

import type { StreamEventHandler } from '../stores/app-store.js';
import type { Kiro } from '../kiro.js';
import type { AgentEngine } from '../agent-engine.js';
import type { KasCommand } from '../kas-commands.js';
import type { InterruptMode } from '../constants/interrupt-mode.js';
import type {
  AvailableCommand,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from '../types/commands.js';
import type {
  ActiveCommand,
  HookInfo,
  KnowledgeEntry,
  McpServerInfo,
  ToolInfo,
  CodePanelData,
  RequestStat,
  StatsSummary,
} from '../stores/app-store.js';

/** Context passed to command handlers */
export interface CommandContext {
  /** Kiro client for backend communication */
  kiro: Kiro;
  /** Active agent backend */
  agentEngine: AgentEngine;
  /**
   * Slash commands visible for the current engine. This is the merged
   * list from `selectVisibleSlashCommands` -- host commands + V2/KAS
   * backend commands + projected prompts / skills / steering. The
   * dispatcher uses this list to look commands up by name.
   */
  slashCommands: readonly AvailableCommand[];
  /**
   * Static, TUI-owned KAS commands. Always empty in V2 mode. The dispatcher
   * checks this list first in KAS mode so KAS-side handlers take precedence
   * over the V2 dispatcher pipeline for the same command name.
   */
  kasCommands: readonly KasCommand[];
  /** Prompts available for invocation (populated by the active engine). */
  prompts: readonly PromptEntry[];
  /** Skills available for invocation (populated by the active engine). */
  skills: readonly SkillEntry[];
  /** Steering documents available for invocation (KAS only). */
  steering: readonly SteeringEntry[];
  /** Show transient alert */
  showAlert: (
    message: string,
    status: 'success' | 'warning' | 'error',
    autoHideMs?: number
  ) => void;
  /** Set loading message (shows shimmer) */
  setLoadingMessage: (message: string | null) => void;
  /** Set active command (for selection menus) */
  setActiveCommand: (cmd: ActiveCommand | null) => void;
  /** Update current model in store */
  setCurrentModel: (model: { id: string; name: string }) => void;
  /** Update current reasoning effort level in store (KAS /effort). */
  setCurrentEffort: (effort: string | null) => void;
  /** Update current agent in store */
  setCurrentAgent: (
    agent: { name: string; welcomeMessage?: string },
    options?: { suppressWelcome?: boolean }
  ) => void;
  /** Current agent (read-only snapshot at the moment the context was built). Used by effects
   *  that need to know the previous agent before swapping (e.g. modeChanged telemetry). */
  currentAgent: { name: string } | null;
  /** Update context usage percentage */
  setContextUsage: (percent: number) => void;
  /** Show/hide context breakdown panel */
  setShowContextBreakdown: (show: boolean, breakdown?: any) => void;
  /** Show/hide help panel */
  setShowHelpPanel: (
    show: boolean,
    commands?: Array<{
      name: string;
      description: string;
      usage: string;
      subcommands?: string[];
    }>
  ) => void;
  /** Show/hide TUI info panel */
  setShowTuiPanel: (show: boolean) => void;
  /** Show/hide changelog panel */
  setShowChangelogPanel: (show: boolean) => void;
  /** Show/hide usage panel */
  setShowUsagePanel: (show: boolean, data?: any) => void;
  setShowRewindExplorer: (show: boolean, rows?: any[]) => void;
  /** Show/hide MCP servers panel */
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string,
    registryServers?: McpServerInfo[]
  ) => void;
  /** Show/hide tools panel */
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  /**
   * Read-only snapshot of the cached session tool listing at the moment the
   * context was built. Populated by KAS via `_kiro/tools/didChange`; used by
   * the KAS `/tools` handler to open the panel without clearing the cache.
   */
  toolsList: readonly ToolInfo[];
  /** Show/hide goal panel */
  setShowGoalPanel: (show: boolean) => void;
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
  /** Show/hide stats panel */
  setShowStatsPanel: (
    show: boolean,
    stats?: RequestStat[],
    summary?: StatsSummary | null
  ) => void;
  /** Show/hide hooks panel */
  setShowHooksPanel: (show: boolean, hooks?: HookInfo[]) => void;
  setShowKeybindingsPanel: (show: boolean) => void;
  setShowDisplaySettingsPanel: (show: boolean) => void;
  setShowThemePanel: (show: boolean) => void;
  setShowSettingsPanel: (show: boolean) => void;
  setSettingsReturnOnEscape: (value: boolean) => void;
  /** Set the active interrupt mode (steer or queue) — takes effect immediately */
  setActiveInterruptMode: (mode: InterruptMode) => void;
  /**
   * Snapshot of `settingsReturnOnEscape`. Effect handlers read this to
   * decide whether to bounce the user back into the /settings picker on
   * completion (true when the flow was launched from /settings) or just
   * close the overlay (true for direct /theme, /keybindings, etc.).
   */
  settingsReturnOnEscape: boolean;
  /**
   * Re-open the /settings top-level menu. Used by command flows that finish
   * a sub-action and should return the user to the /settings picker rather
   * than dismissing the overlay (per the /settings UX spec).
   */
  reopenSettingsMenu: () => void;
  /** Show/hide knowledge panel */
  setShowKnowledgePanel: (
    show: boolean,
    entries?: KnowledgeEntry[],
    status?: string
  ) => void;
  /** Show/hide code panel */
  setShowCodePanel: (show: boolean, data?: CodePanelData) => void;
  /**
   * Open the structured spec artifact view panel.
   *
   * Resolves once the summary has been loaded (or once the load fails
   * and the panel is opened in error mode). The store handles all
   * state — the caller doesn't have to manage cursor / mode.
   */
  openArtifactView: (
    featureName: string,
    artifact: 'requirements' | 'design' | 'tasks'
  ) => Promise<void>;
  /** Clear conversation messages (keeps last turn for /clear) */
  clearMessages: () => void;
  /** Reset all messages (full wipe for /chat new) */
  resetMessages: () => void;
  /** Clear all command UI state (menus, panels) */
  clearUIState: () => void;
  /** Send message to chat. If displayContent is provided, it's shown in UI instead of content. */
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>,
    displayContent?: string
  ) => Promise<void>;
  /** Create a stream event handler for processing agent events into messages */
  createStreamEventHandler: () => StreamEventHandler;
  /** Update the session ID in the store */
  setSessionId: (id: string | null) => void;
  /** Add a system message to the conversation */
  addSystemMessage: (content: string, success: boolean) => void;
  /** Add session to store */
  addSession: (session: any) => void;
  /** Set active session */
  setActiveSession: (id: string) => void;
  /** Current sessions map */
  sessions: Map<string, any>;
  /** Set app mode */
  setMode: (
    mode: 'inline' | 'expanded' | 'crew-monitor' | 'session-view'
  ) => void;
  /** Get current conversation messages */
  getMessages: () => Array<{
    id: string;
    role: string;
    content: string;
  }>;
  /** Set voice stop callback */
  setVoiceStop: (fn: (() => void) | null) => void;
  /** Set voice cancel callback */
  setVoiceCancel: (fn: (() => void) | null) => void;
  /** Set voice level */
  setVoiceLevel: (level: number | null) => void;
  /** Set partial transcription text for ghost text display during recording */
  setVoicePartialText: (text: string | null) => void;
  /** Whether voice auto-submit is enabled */
  voiceAutoSubmit: boolean;
  /** Toggle voice auto-submit */
  toggleVoiceAutoSubmit: () => void;
  /** Current voice hint index */
  voiceHintIndex: number;
  /** Increment voice hint index */
  incrementVoiceHint: () => void;
  /** Set pending voice text for insertion into input */
  setPendingVoiceText: (text: string | null) => void;
}
