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
  requiredMethods: string[];
}

/** TUI-owned slash commands. A command is available when all its
 *  required extension methods are advertised by the agent. */
export const SLASH_COMMANDS: SlashCommand[] = [
  {
    name: '/help',
    description: 'Show available commands',
    meta: { inputType: 'panel' },
    requiredMethods: [],
  },
  {
    name: '/agent',
    description: 'List or switch agents',
    meta: {
      inputType: 'selection',
      optionsMethod: '_kiro.dev/commands/agent/options',
      hint: '',
      subcommands: ['create', 'edit', 'swap'],
      subcommandHints: { create: '<name>', edit: '[name]', swap: '<name>' },
    },
    requiredMethods: ['_kiro/agent/list'],
  },
  {
    name: '/clear',
    description: 'Clear the conversation and start a fresh session',
    // No required extension methods: composed from ACP-standard session/new.
    requiredMethods: [],
  },
];
