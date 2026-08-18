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

/** Resolve the input's first token using the same matching as executeCommand. */
export function resolveSlashCommand(
  input: string,
  commands: readonly AvailableCommand[]
): AvailableCommand | undefined {
  const { isCommand, name } = parseCommand(input);
  if (!isCommand) return undefined;
  if (name.toLowerCase() === 'voice' && process.env.KIRO_VOICE_SERVER_URL) {
    return { name: '/voice', description: 'Voice input' };
  }
  return findCommand(commands, name);
}

export function resolveSlashCommandForDispatch(
  input: string,
  commandCatalog: Pick<CommandContext, 'kasCommands' | 'slashCommands'>
): AvailableCommand | undefined {
  const { isCommand, name } = parseCommand(input);
  if (!isCommand) return undefined;
  if (name.toLowerCase() === 'voice' && process.env.KIRO_VOICE_SERVER_URL) {
    return { name: '/voice', description: 'Voice input' };
  }
  return (
    findCommand(commandCatalog.kasCommands, name) ??
    findCommand(commandCatalog.slashCommands, name)
  );
}

/** Whether the input resolves to a command the dispatcher can execute. */
export function isKnownSlashCommandToken(
  input: string,
  commands: readonly AvailableCommand[]
): boolean {
  return resolveSlashCommand(input, commands) !== undefined;
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
  const { isCommand, args } = parseCommand(input);
  if (!isCommand) {
    return false;
  }

  const cmd = resolveSlashCommandForDispatch(input, ctx);
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
