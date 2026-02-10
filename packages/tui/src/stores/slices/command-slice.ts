/**
 * Command slice - manages slash command UI state
 */
import type { StateCreator } from 'zustand';
import type { CommandOption } from '../../types/commands';

export interface SlashCommand {
  name: string;
  description: string;
  source: 'local' | 'backend';
  meta?: {
    optionsMethod?: string;
    inputType?: 'text' | 'selection' | 'multiselect' | 'panel';
    subcommands?: string[];
    hint?: string;
    local?: boolean;
  };
}

export interface ActiveCommand {
  command: SlashCommand;
  options: CommandOption[];
}

export interface CommandState {
  slashCommands: SlashCommand[];
  activeCommand: ActiveCommand | null;
  commandInputValue: string;
  activeTrigger: { key: string; position: number; type: 'start' | 'inline' } | null;
  filePickerHasResults: boolean;
}

export interface CommandActions {
  setSlashCommands: (commands: SlashCommand[]) => void;
  setActiveCommand: (command: ActiveCommand | null) => void;
  setCommandInput: (value: string) => void;
  setActiveTrigger: (trigger: { key: string; position: number; type: 'start' | 'inline' } | null) => void;
  setFilePickerHasResults: (hasResults: boolean) => void;
  clearCommandInput: () => void;
}

export type CommandSlice = CommandState & CommandActions;

export const createCommandSlice: StateCreator<CommandSlice> = (set) => ({
  // State
  slashCommands: [],
  activeCommand: null,
  commandInputValue: '',
  activeTrigger: null,
  filePickerHasResults: false,

  // Actions
  setSlashCommands: (commands) => {
    set((state) => {
      const localCommands = state.slashCommands.filter((cmd) => cmd.source === 'local');
      return { slashCommands: [...localCommands, ...commands] };
    });
  },
  setActiveCommand: (command) => set({ activeCommand: command }),
  setCommandInput: (value) => set({ commandInputValue: value }),
  setActiveTrigger: (trigger) => set({ activeTrigger: trigger }),
  setFilePickerHasResults: (hasResults) => set({ filePickerHasResults: hasResults }),
  clearCommandInput: () => set({ commandInputValue: '', activeTrigger: null, filePickerHasResults: false }),
});
