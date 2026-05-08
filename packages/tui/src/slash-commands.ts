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
      hint: '',
      subcommands: ['create', 'edit', 'swap'],
      subcommandHints: { create: '<name>', edit: '[name]', swap: '<name>' },
    },
    // Composed from ACP's standard session modes (`availableModes`) — no
    // custom extension method required. See the review discussion at
    // https://github.com/kiro-team/kiro-agent/pull/568#discussion_r3192594213
    // for why `_kiro/agent/list` was dropped in favor of session modes.
    requiredMethods: [],
  },
  {
    name: '/clear',
    description: 'Clear the conversation and start a fresh session',
    // No required extension methods: composed from ACP-standard session/new.
    requiredMethods: [],
  },
  {
    name: '/model',
    description: 'List or switch models',
    // Composed from ACP-standard session/set_config_option with
    // configOptions[category='model']. No extension method required;
    // the option list itself may still be empty if KAS has no
    // ModelConfigProvider registered — in that case the dispatcher's
    // selection flow will surface "No options available".
    meta: {
      inputType: 'selection',
      hint: '',
    },
    requiredMethods: [],
  },
  {
    name: '/reply',
    description: 'Reply to the last assistant message in $EDITOR',
    requiredMethods: [],
  },
];
