/**
 * Types for the slash command handler system.
 */

import type { AgentStreamEvent } from '../types/agent-events.js';
import type { Kiro } from '../kiro.js';
import type {
  SlashCommand,
  ActiveCommand,
  HookInfo,
  KnowledgeEntry,
  McpServerInfo,
  ToolInfo,
  CodePanelData,
} from '../stores/app-store.js';

/** Context passed to command handlers */
export interface CommandContext {
  /** Kiro client for backend communication */
  kiro: Kiro;
  /** Available slash commands from backend */
  slashCommands: SlashCommand[];
  /** Show transient alert */
  showAlert: (
    message: string,
    status: 'success' | 'error',
    autoHideMs?: number
  ) => void;
  /** Set loading message (shows shimmer) */
  setLoadingMessage: (message: string | null) => void;
  /** Set active command (for selection menus) */
  setActiveCommand: (cmd: ActiveCommand | null) => void;
  /** Update current model in store */
  setCurrentModel: (model: { id: string; name: string }) => void;
  /** Update current agent in store */
  setCurrentAgent: (agent: { name: string }) => void;
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
  /** Show/hide usage panel */
  setShowUsagePanel: (show: boolean, data?: any) => void;
  /** Show/hide MCP servers panel */
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string
  ) => void;
  /** Show/hide tools panel */
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  /** Show/hide hooks panel */
  setShowHooksPanel: (show: boolean, hooks?: HookInfo[]) => void;
  /** Show/hide knowledge panel */
  setShowKnowledgePanel: (
    show: boolean,
    entries?: KnowledgeEntry[],
    status?: string
  ) => void;
  /** Show/hide code panel */
  setShowCodePanel: (show: boolean, data?: CodePanelData) => void;
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
  createStreamEventHandler: () => (event: AgentStreamEvent) => void;
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
  /** Update user theme colors (prompt text+bg combo and/or response text and/or diff colors).
   *  Pass null to clear an override, undefined to leave unchanged. */
  setUserColors: (
    prompt?: { text: any; bg: any } | null,
    response?: any | null,
    diff?: any | null
  ) => void;
  /** Set theme preview string (rendered below menu during /theme flow) */
  setThemePreview: (preview: string | null) => void;
  /** Get the base theme's diff hex colors (for preview fallback when user preset is 'default') */
  getThemeDiffHex: () => {
    added: { background: string; bar: string; highlight: string };
    removed: { background: string; bar: string; highlight: string };
  };
  /** Get a preview string showing the auto-detected theme with no user overrides */
  getAutoPreview: () => string;
}
