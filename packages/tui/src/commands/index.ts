/**
 * Slash command system public API.
 */

import { parseCommand } from '../types/commands.js';
import { dispatch } from './dispatcher.js';
import type { CommandContext } from './types.js';
import type { SlashCommand } from '../stores/app-store.js';

export type { CommandContext } from './types.js';

/** Find command by exact or prefix match (alphabetical order for prefix) */
function findCommand(
  commands: SlashCommand[],
  name: string
): SlashCommand | undefined {
  const lower = name.toLowerCase();

  // Exact match first
  const exact = commands.find((c) => c.name.toLowerCase() === `/${lower}`);
  if (exact) return exact;

  // Prefix match - sort alphabetically so /clear < /compact < /context
  const sorted = [...commands].sort((a, b) => a.name.localeCompare(b.name));
  return sorted.find((c) => c.name.toLowerCase().startsWith(`/${lower}`));
}

/**
 * Execute a slash command.
 */
export async function executeCommand(
  input: string,
  ctx: CommandContext
): Promise<boolean> {
  const { isCommand, name, args } = parseCommand(input);
  if (!isCommand) {
    return false;
  }

  const cmd = findCommand(ctx.slashCommands, name);
  if (!cmd) {
    ctx.showAlert(`Unknown command: /${name}`, 'error', 3000);
    return true;
  }

  await dispatch(cmd, args, ctx);
  return true;
}

/**
 * Execute a command with a specific argument value.
 * Used by selection menus when user picks an option.
 */
export async function executeCommandWithArg(
  commandName: string,
  argValue: string,
  ctx: CommandContext
): Promise<void> {
  const cmd = ctx.slashCommands.find((c) => c.name === `/${commandName}`);
  if (!cmd) {
    ctx.showAlert(`Unknown command: /${commandName}`, 'error', 3000);
    return;
  }

  await dispatch(cmd, argValue, ctx);
}
