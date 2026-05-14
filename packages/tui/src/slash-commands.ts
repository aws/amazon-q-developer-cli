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

/** TUI-owned slash commands. A command is available when all its
 *  required extension methods are advertised by the agent. */
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
    name: '/spec',
    description: 'List specs, switch to spec mode, or run spec tasks',
    meta: {
      local: true,
      subcommands: ['new', 'run'],
      subcommandHints: { new: '<feature-name>', run: '<feature-name>' },
    },
  },
];

/**
 * Returns true when the KAS agent engine is active.
 *
 * Detection is based on the `KIRO_AGENT_ENGINE` environment variable which
 * is set by the Rust launcher when `--agent-engine=kas` is passed. This is
 * the same check used by `createAcpClient()` to decide whether to
 * instantiate `KasAcpClient` vs `RustAcpClient`.
 *
 * NOTE: The engine cannot change mid-session. It is determined at process
 * startup by the Rust launcher and the `createAcpClient()` factory is
 * called exactly once during `Kiro.initialize()`. There is no reconnect or
 * engine-switch flow. Therefore, if the engine is not KAS at startup,
 * `SLASH_COMMANDS` (including `/spec`) will never be broadcast — and if it
 * IS KAS, the commands remain valid for the entire session lifetime. No
 * explicit removal logic is needed when the engine "changes away from KAS"
 * because that transition cannot occur.
 */
export function isKasEngine(): boolean {
  return process.env.KIRO_AGENT_ENGINE === 'kas';
}
