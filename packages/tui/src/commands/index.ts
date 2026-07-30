/**
 * Slash command system public API.
 */

import { parseCommand } from '../types/commands.js';
import type { AvailableCommand } from '../types/commands.js';
import {
  getKasWorkflowAliasSubcommand,
  KasCommandName,
} from '../kas-commands.js';
import { dispatch } from './dispatcher.js';
import type { CommandContext } from './types.js';
import { commandMetricName } from '../utils/slash-command-telemetry.js';

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
  return sorted.find(
    (c) =>
      c.meta?.hidden !== true && c.name.toLowerCase().startsWith(`/${lower}`)
  );
}

/**
 * Whether the input's first whitespace-separated token names a known slash
 * command (exact match, no prefix). Used by lite mode to decide between
 * "dispatch a command" and "send as a chat message" — the latter is the
 * lite contract for typos like /foozle and pasted paths like /some/path.
 */
export function isKnownSlashCommandToken(
  input: string,
  commands: readonly AvailableCommand[]
): boolean {
  const { isCommand, name } = parseCommand(input);
  if (!isCommand) return false;
  const lower = name.toLowerCase();
  return commands.some((c) => c.name.toLowerCase() === `/${lower}`);
}

export function recordSlashCommandInvocation(
  command: AvailableCommand,
  ctx: Pick<CommandContext, 'kiro'>
): void {
  ctx.kiro.recordSlashCommandInvocation(commandMetricName(command));
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

  // Voice command: always handle client-side when remote voice server is configured,
  // even if the backend doesn't advertise /voice (rollout gate may block it).
  if (name === 'voice' && process.env.KIRO_VOICE_SERVER_URL) {
    const syntheticCmd = { name: '/voice', meta: {} } as any;
    recordSlashCommandInvocation(syntheticCmd, ctx);
    await dispatch(syntheticCmd, args, ctx);
    return true;
  }

  const cmd =
    findCommand(ctx.kasCommands, name) ?? findCommand(ctx.slashCommands, name);
  if (!cmd) {
    // Not a known command — let the caller handle it as a regular message
    // (e.g. pasted file paths like "/Users/me/file.txt")
    return false;
  }

  recordSlashCommandInvocation(cmd, ctx);

  if (ctx.agentEngine === 'kas') {
    const aliasSubcommand = getKasWorkflowAliasSubcommand(cmd.name);
    if (aliasSubcommand !== undefined) {
      const workflowCmd = ctx.kasCommands.find(
        (candidate) => candidate.name === KasCommandName.Workflow
      );
      if (!workflowCmd) return false;

      const trimmedArgs = args.trim();
      const canonicalArgs = aliasSubcommand
        ? `${aliasSubcommand}${trimmedArgs ? ` ${trimmedArgs}` : ''}`
        : trimmedArgs;
      await dispatch(workflowCmd, canonicalArgs, ctx);
      return true;
    }
  }

  if (cmd.name.toLowerCase() === '/goal') {
    const trimmedArgs = args?.trim() ?? '';
    const shouldSendAsPrompt =
      ctx.agentEngine === 'kas'
        ? trimmedArgs.length > 0
        : trimmedArgs.length > 0 && trimmedArgs !== 'clear';
    if (shouldSendAsPrompt) {
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
