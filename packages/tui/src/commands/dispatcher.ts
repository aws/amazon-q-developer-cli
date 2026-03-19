/**
 * Command dispatcher.
 *
 * Single function that handles all command execution:
 * 1. Input gathering (selection menu, panel)
 * 2. Backend execution
 * 3. Effect execution
 */

import type { CommandContext } from './types.js';
import type { SlashCommand } from '../stores/app-store.js';
import type { TuiCommand, CommandOption } from '../types/commands.js';
import { runEffect } from './effects.js';
import { formatRelativeTime } from '../utils/sessions.js';

/**
 * Dispatch a command through the standard flow.
 *
 * @param cmd - Slash command definition
 * @param args - Arguments (empty string if none)
 * @param ctx - Command context
 */
export async function dispatch(
  cmd: SlashCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const { inputType, type } = cmd.meta ?? {};
  const isLocal = cmd.meta?.local === true;
  const cmdName = cmd.name.replace(/^\//, '');

  // Handle prompt commands - send as regular message, backend resolves via session/prompt interception
  if (type === 'prompt') {
    const message = args ? `/${cmdName} ${args}` : `/${cmdName}`;
    await ctx.sendMessage(message);
    return;
  }

  // 1. Input gathering (when no args provided)
  if (!args) {
    if (inputType === 'selection') {
      try {
        ctx.setLoadingMessage(`Loading ${cmdName} options...`);
        // TODO - dispatch flow needs to be thought through more, coupling slash commands
        // all within the same dispatch flow doesn't seem right.
        //
        // /chat -> use listSessions API, fallback to extension method
        const options =
          cmdName === 'chat'
            ? await fetchChatOptions(ctx)
            : (await ctx.kiro.getCommandOptions(cmd.name, '')).options;
        ctx.setLoadingMessage(null);
        if (options.length > 0) {
          ctx.setActiveCommand({ command: cmd, options });
          return;
        }
        const noOptionsMsg =
          cmdName === 'chat'
            ? 'No previous sessions found'
            : `No options available for /${cmdName}`;
        ctx.showAlert(noOptionsMsg, 'error', 3000);
        return;
      } catch {
        ctx.setLoadingMessage(null);
        // Fall through to execute if options fetch fails
      }
    }

    // Panel commands set activeCommand to block input while panel is open
    if (inputType === 'panel') {
      ctx.setActiveCommand({ command: cmd, options: [] });
    }
  }
  // 2. Execute backend (skip for local commands)
  let result = null;
  if (cmd.source === 'backend' && !isLocal) {
    // Show loading for agent swap
    if (cmdName === 'agent' && args) {
      ctx.setLoadingMessage(`Agent changing to ${args}`);
    }
    try {
      result = await ctx.kiro.executeCommand({
        command: cmdName,
        args: args ? { value: args } : {},
      } as TuiCommand);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Command failed';
      ctx.setLoadingMessage(null);
      ctx.showAlert(message, 'error');
      return;
    }
    ctx.setLoadingMessage(null);
  }

  // 3. Run effect
  const effectHandledMessage = runEffect(cmd, result, ctx, args);

  // 4. Show result message (skip for panel commands without args - they show their own UI,
  //    and skip when the effect already handled messaging)
  if (
    result?.message &&
    !effectHandledMessage &&
    !(inputType === 'panel' && !args)
  ) {
    ctx.showAlert(result.message, result.success ? 'success' : 'error', 5000);
  }
}

async function fetchChatOptions(ctx: CommandContext): Promise<CommandOption[]> {
  const { sessions } = await ctx.kiro.listSessions(process.cwd());
  const currentSessionId = ctx.kiro.sessionId;
  return sessions
    .filter((s) => s.sessionId !== currentSessionId)
    .filter((s) => s.title != null)
    .map((s) => ({
      value: s.sessionId,
      label: `${s.title!} (${s.sessionId.slice(0, 8)})`,
      description: s.updatedAt ? formatRelativeTime(s.updatedAt) : undefined,
    }));
}
