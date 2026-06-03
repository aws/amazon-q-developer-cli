/**
 * Slash command system public API.
 */

import { parseCommand } from '../types/commands.js';
import type { AvailableCommand } from '../types/commands.js';
import { dispatch } from './dispatcher.js';
import type { CommandContext } from './types.js';

export type { CommandContext } from './types.js';

/** Find command by exact or prefix match (alphabetical order for prefix) */
function findCommand<T extends AvailableCommand>(
  commands: readonly T[],
  name: string
): T | undefined {
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

  const cmd =
    findCommand(ctx.kasCommands, name) ?? findCommand(ctx.slashCommands, name);
  if (!cmd) {
    // Not a known command — let the caller handle it as a regular message
    // (e.g. pasted file paths like "/Users/me/file.txt")
    return false;
  }

  // /goal with a description (set case) must flow through sendMessage so the
  // TUI enters streaming mode. The server's slash router handles it like /skills.
  // Only /goal (bare) and /goal clear go through the command system.
  if (cmd.name.toLowerCase() === '/goal') {
    const trimmedArgs = args?.trim() ?? '';
    const isSubcommand = !trimmedArgs || trimmedArgs === 'clear';
    if (!isSubcommand) {
      // Send as a regular prompt — server slash router intercepts /goal text
      await ctx.sendMessage(input);
      return true;
    }
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
  const cmd =
    ctx.kasCommands.find((c) => c.name === `/${commandName}`) ??
    ctx.slashCommands.find((c) => c.name === `/${commandName}`);
  if (!cmd) {
    ctx.showAlert(`Unknown command: /${commandName}`, 'error', 3000);
    return;
  }

  // For /agent selection, prefix with "swap" to avoid collisions with subcommands
  // (e.g. an agent named "create" would otherwise trigger the create subcommand)
  const effectiveArg =
    commandName === 'agent' && argValue ? `swap ${argValue}` : argValue;

  await dispatch(cmd, effectiveArg, ctx, { argIsSynthetic: true });
}
