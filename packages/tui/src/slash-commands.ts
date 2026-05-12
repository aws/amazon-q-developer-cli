// Note that this is only for KAS.
// This is needed because KAS is a harness to be used by multiple clients.
// Instead of exposing higher abstraction level extension methods,
// it exposes more basic primitives that are needed by every client.
// It is then up to the client to compose these primitives to fulfill
// their own needs.
import type { CommandMeta } from './types/commands';

export interface SlashCommand {
  name: string;
  description: string;
  meta?: CommandMeta;
}

/** TUI-owned slash commands for KAS mode. */
export const SLASH_COMMANDS: SlashCommand[] = [
  {
    name: '/help',
    description: 'Show available commands',
    meta: { inputType: 'panel' },
  },
  {
    name: '/agent',
    description: 'List or switch agents',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['create', 'edit', 'swap'],
      subcommandHints: { create: '<name>', edit: '[name]', swap: '<name>' },
    },
  },
  {
    name: '/clear',
    description: 'Clear the conversation and start a fresh session',
  },
  {
    name: '/model',
    description: 'List or switch models',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: '/reply',
    description: 'Reply to the last assistant message in $EDITOR',
  },
  {
    name: '/paste',
    description: 'Paste image from clipboard',
  },
  {
    name: '/prompts',
    description: 'Select or list available prompts',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: '/usage',
    description: 'Show plan usage and billing information',
    meta: { inputType: 'panel' },
  },
  {
    name: '/knowledge',
    description: 'Manage knowledge bases',
    meta: { inputType: 'panel' },
    requiredMethods: ['_kiro/knowledge'],
  },
];
