// Note that this is only for KAS.
// This is needed because KAS is a harness to be used by multiple clients.
// Instead of exposing higher abstraction level extension methods,
// it exposes more basic primitives that are needed by every client.
// It is then up to the client to compose these primitives to fulfill
// their own needs.
import type { AvailableCommand, CommandMeta } from './types/commands';

export enum KasCommandName {
  Help = '/help',
  Agent = '/agent',
  Chat = '/chat',
  Clear = '/clear',
  Model = '/model',
  Effort = '/effort',
  Reply = '/reply',
  Paste = '/paste',
  Prompts = '/prompts',
  Usage = '/usage',
  Spec = '/spec',
  Knowledge = '/knowledge',
  Compact = '/compact',
  Context = '/context',
  Code = '/code',
  Hooks = '/hooks',
  Mcp = '/mcp',
  Tools = '/tools',
  Plan = '/plan',
  Feedback = '/feedback',
  Rewind = '/rewind',
  UpgradeAgent = '/upgrade-agent',
}

const KAS_COMMAND_NAME_VALUES: ReadonlySet<string> = new Set(
  Object.values(KasCommandName)
);

export function isKasCommandName(name: string): name is KasCommandName {
  return KAS_COMMAND_NAME_VALUES.has(name);
}

export function isKasCommand(cmd: AvailableCommand): cmd is KasCommand {
  return isKasCommandName(cmd.name);
}

/**
 * TODO: KasCommand currently mirrors `AvailableCommand` shape with a
 * narrowed `name`. As KAS-side handlers grow we'll likely diverge with
 * KAS-specific fields (typed subcommand schemas, input validators, etc.)
 * and stop sharing the V2 backend command shape entirely.
 */
export interface KasCommand extends AvailableCommand {
  name: KasCommandName;
  meta?: CommandMeta;
}

/** TUI-owned slash commands for KAS mode. */
export const KAS_COMMANDS: readonly KasCommand[] = [
  {
    name: KasCommandName.Help,
    description: 'Show available commands',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Agent,
    description: 'List or switch agents',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['create', 'edit', 'swap'],
      subcommandHints: { create: '<name>', edit: '[name]', swap: '<name>' },
    },
  },
  {
    name: KasCommandName.Chat,
    description: 'Load a previous session, save, or start a new one',
    meta: {
      inputType: 'selection',
      local: true,
      subcommands: ['new', 'save', 'load'],
      subcommandHints: {
        new: '[prompt]',
        save: '[--force] <path>',
        load: '<path>',
      },
    },
  },
  {
    name: KasCommandName.Clear,
    description: 'Clear the conversation and start a fresh session',
  },
  {
    name: KasCommandName.Model,
    description: 'List or switch models',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: KasCommandName.Effort,
    description: 'List or set the reasoning effort level',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: KasCommandName.Reply,
    description: 'Reply to the last assistant message in $EDITOR',
  },
  {
    name: KasCommandName.Paste,
    description: 'Paste image from clipboard',
  },
  {
    name: KasCommandName.Prompts,
    description: 'Select or list available prompts',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: KasCommandName.Usage,
    description: 'Show plan usage and billing information',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Spec,
    description: 'List specs, switch to spec mode, or run spec tasks',
    meta: {
      local: true,
      subcommands: ['new', 'run', 'view', 'analyze_requirements'],
      subcommandHints: {
        new: '<feature-name>',
        run: '<feature-name>',
        view: '<feature-name> [requirements|design|tasks]',
        analyze_requirements: '↵ to select a spec',
      },
    },
  },
  {
    name: KasCommandName.Knowledge,
    description: 'Manage knowledge bases',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Compact,
    description: 'Compact conversation history to reduce context usage',
  },
  {
    name: KasCommandName.Context,
    description: 'Show or manage context files',
    meta: {
      inputType: 'panel',
      hint: 'add <path>, remove <path>, clear',
      subcommands: ['show', 'add', 'remove', 'clear'],
      subcommandHints: { add: '[--force] <path>...', remove: '<path>...' },
    },
  },
  {
    name: KasCommandName.Code,
    description:
      'Code intelligence status, initialization, and codebase overview',
    meta: {
      inputType: 'panel',
      subcommands: ['status', 'init', 'overview'],
    },
  },
  {
    name: KasCommandName.Hooks,
    description: 'View configured hooks',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Mcp,
    description: 'Show MCP server status',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Tools,
    description: 'List available tools',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Plan,
    description:
      'Switch to plan mode to break ideas into an implementation plan',
  },
  {
    name: KasCommandName.Feedback,
    description: 'Submit feedback, request features, or report issues',
    meta: {
      inputType: 'selection',
      searchable: false,
      hint: '',
    },
  },
  {
    name: KasCommandName.Rewind,
    description: 'Fork the session at an earlier turn',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.UpgradeAgent,
    description: 'Upgrade V2 agent configs to universal (V2 + V3) format',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['run', 'diagnostics'],
      subcommandHints: {
        run: '(upgrade agents by group)',
        diagnostics: '(review upgraded agents)',
      },
    },
  },
];
