/**
 * Types for the slash command handler system.
 */

import type { TerminalColor } from '../types/themeTypes.js';
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
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import type {
  AgentEntry,
  EffortEntry,
  ModelEntry,
} from '../utils/kas-config-options.js';
import type {
  ActiveCommand,
  HookInfo,
  KnowledgeEntry,
  McpServerInfo,
  ToolInfo,
  CodePanelData,
  RequestStat,
  StatsSummary,
  UpgradeAnalysisRow,
} from '../stores/app-store.js';

/** Context passed to command handlers */
export interface CommandContext {
  /** Kiro client for backend communication */
  kiro: Kiro;
  /** Active agent backend */
  agentEngine: AgentEngine;
  /** Whether the current session is a cloud session. Gates cloud-only commands like `/repo`. */
  cloudSessionActive: boolean;
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
  /** KAS available models cache. Empty in V2. */
  kasAvailableModels: readonly ModelEntry[];
  /** KAS available efforts cache. Empty in V2. */
  kasAvailableEfforts: readonly EffortEntry[];
  /** KAS available agents cache. Empty in V2. */
  kasAvailableAgents: readonly AgentEntry[];
  /** Show transient alert */
  showAlert: (
    message: string,
    status: 'success' | 'warning' | 'error',
    autoHideMs?: number
  ) => void;
  /**
   * Announce a state change the user should see persistently.
   * In lite mode this writes to the chat scrollback as a system message;
   * in TUI mode it falls back to a transient alert (since classic has a
   * NotificationBar but no equivalent always-visible scrollback target).
   */
  announceSystem: (
    message: string,
    success?: boolean,
    autoHideMs?: number
  ) => void;
  /** Set loading message (shows shimmer) */
  setLoadingMessage: (message: string | null) => void;
  /** Set active command (for selection menus) */
  setActiveCommand: (cmd: ActiveCommand | null) => void;
  /** Update current model in store */
  setCurrentModel: (model: { id: string; name: string }) => void;
  /**
   * Begin tracking a KAS session (new vs resumed) and reset the model-change
   * baseline. Called before a session-start RPC; returns a restore function to
   * revert the prior tracking state if that RPC fails.
   */
  beginKasSession: (origin: 'new' | 'resumed') => () => void;
  /** Read current model from store (used by /effort to detect model=auto). */
  getCurrentModel?: () => { id: string; name: string } | null;
  /** Update current reasoning effort level in store (KAS /effort). */
  setCurrentEffort: (effort: string | null) => void;
  /** Read current effort from store (used by /effort to validate a switch). */
  getCurrentEffort?: () => string | null;
  /** Read current agent from store (used by /agent to validate a switch). */
  getCurrentAgent?: () => { name: string } | null;
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
  setUpgradeDiagnostics: (
    rows: UpgradeAnalysisRow[],
    description: string
  ) => void;
  /** Stash bucket value → agent names for the /upgrade-agent run preview panel. */
  setUpgradeRunPreview: (preview: Record<string, string[]>) => void;
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
  /** Show/hide the `/repo` picker with the fetched repositories (cloud-only). */
  setShowRepoPicker: (
    show: boolean,
    resources?: SourceProviderResource[]
  ) => void;
  setShowKeybindingsPanel: (show: boolean) => void;
  setShowDisplaySettingsPanel: (show: boolean) => void;
  setShowThemePanel: (show: boolean) => void;
  /** Open/close the /quit "keep running vs turn off" prompt (cloud sessions only). */
  setShowCloudQuitPrompt: (show: boolean) => void;
  setShowSettingsPanel: (show: boolean) => void;
  setSettingsReturnOnEscape: (value: boolean) => void;
  /** Stash the parent route consumed by the /verbosity menu's ESC handler. */
  setVerboseReturnOnEscape: (route: string | null) => void;
  /** Stash the parent route consumed by the /theme menu's ESC handler. */
  setThemeReturnOnEscape: (route: string | null) => void;
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
  /** Lite-only: signal LiteLayout to wipe scrollback + render cache.
   *  Used on /chat <id> and /rewind to drop stale flushed rows so the
   *  resumed history isn't stacked under the previous session. No-op in TUI. */
  bumpLiteScrollbackClear: () => void;
  /** Send message to chat. If displayContent is provided, it's shown in UI instead of content. */
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>,
    displayContent?: string
  ) => Promise<void>;
  /** Create a stream event handler for processing agent events into messages. */
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
  /** Update user theme colors (prompt text+bg combo and/or response text and/or diff colors).
   *  Pass null to clear an override, undefined to leave unchanged. */
  setUserColors: (
    prompt?: { text: any; bg: any } | null,
    response?: any | null,
    diff?: any | null
  ) => void;
  /** Switch the base theme at runtime. Pass null to reset to auto-detected. */
  setBaseTheme: (theme: any) => void;
  /** Set theme preview string (rendered below menu during /theme flow) */
  setThemePreview: (preview: string | null) => void;
  /** Get the base theme's diff colors (for preview fallback when user preset is 'default') */
  getThemeDiffHex: () => {
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
  };
  /** Get a preview string showing the auto-detected theme with no user overrides */
  getAutoPreview: () => string;
  /** Switch between TUI and lite mode */
  setUiMode?: (mode: 'tui' | 'lite') => void;
  /** Get current UI mode */
  getUiMode?: () => 'tui' | 'lite';
  /**
   * Set the index into `messages` at which lite's <Static> begins emitting.
   * Called by `switchToLite` (tui→lite) so prior messages already on the
   * user's screen via the modern TUI aren't re-emitted as duplicates in
   * lite style below them.
   */
  setLiteStaticSkipBefore?: (idx: number) => void;
  /** Drain any messages the user typed while a load (or other gating state)
   * was in flight. Effects that gate input on `loadingMessage` should call
   * this once they've cleared it, so queued lines don't sit forever. */
  processQueue: () => Promise<void>;
}
