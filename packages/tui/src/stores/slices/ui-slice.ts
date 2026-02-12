/**
 * UI slice - manages general UI state (mode, exit sequence, tool outputs, context)
 */
import type { StateCreator } from 'zustand';

export interface LastTurnTokens {
  input: number;
  output: number;
  cached: number;
}

export interface UIState {
  mode: 'inline' | 'expanded';
  exitSequence: number;
  exitTimer: NodeJS.Timeout | null;
  toolOutputsExpanded: boolean;
  hasExpandableToolOutputs: boolean;
  contextUsagePercent: number | null;
  lastTurnTokens: LastTurnTokens | null;
  showContextBreakdown: boolean;
  showHelpPanel: boolean;
  helpCommands: Array<{ name: string; description: string; usage: string }>;
}

export interface UIActions {
  setMode: (mode: 'inline' | 'expanded') => void;
  incrementExitSequence: () => void;
  resetExitSequence: () => void;
  toggleToolOutputsExpanded: () => void;
  setHasExpandableToolOutputs: (has: boolean) => void;
  setContextUsage: (percent: number) => void;
  setLastTurnTokens: (tokens: LastTurnTokens) => void;
  toggleContextBreakdown: () => void;
  setShowContextBreakdown: (show: boolean) => void;
  setShowHelpPanel: (show: boolean, commands?: Array<{ name: string; description: string; usage: string }>) => void;
}

export type UISlice = UIState & UIActions;

export const createUISlice: StateCreator<UISlice> = (set) => ({
  // State
  mode: 'inline',
  exitSequence: 0,
  exitTimer: null,
  toolOutputsExpanded: false,
  hasExpandableToolOutputs: false,
  contextUsagePercent: null,
  lastTurnTokens: null,
  showContextBreakdown: false,
  showHelpPanel: false,
  helpCommands: [],

  // Actions
  setMode: (mode) => set({ mode }),

  incrementExitSequence: () => {
    set((state) => {
      if (state.exitTimer) clearTimeout(state.exitTimer);
      const newSequence = state.exitSequence + 1;
      if (newSequence >= 2) process.exit(0);
      const timer = setTimeout(() => set({ exitSequence: 0, exitTimer: null }), 2000);
      return { exitSequence: newSequence, exitTimer: timer };
    });
  },

  resetExitSequence: () => {
    set((state) => {
      if (state.exitTimer) clearTimeout(state.exitTimer);
      return { exitSequence: 0, exitTimer: null };
    });
  },

  toggleToolOutputsExpanded: () => set((state) => ({ toolOutputsExpanded: !state.toolOutputsExpanded })),
  setHasExpandableToolOutputs: (has) => set({ hasExpandableToolOutputs: has }),
  setContextUsage: (percent) => set({ contextUsagePercent: percent }),
  setLastTurnTokens: (tokens) => set({ lastTurnTokens: tokens }),
  toggleContextBreakdown: () => set((state) => ({ showContextBreakdown: !state.showContextBreakdown })),
  setShowContextBreakdown: (show) => set({ showContextBreakdown: show }),
  setShowHelpPanel: (show, commands = []) => set({ showHelpPanel: show, helpCommands: commands }),
});
