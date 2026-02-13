/**
 * Types for the slash command handler system.
 */

import type { Kiro } from '../kiro.js';
import type { SlashCommand, ActiveCommand } from '../stores/app-store.js';

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
  /** Show/hide context breakdown panel */
  setShowContextBreakdown: (show: boolean) => void;
  /** Show/hide help panel */
  setShowHelpPanel: (
    show: boolean,
    commands?: Array<{ name: string; description: string; usage: string }>
  ) => void;
  /** Clear conversation messages */
  clearMessages: () => void;
  /** Clear all command UI state (menus, panels) */
  clearUIState: () => void;
}
