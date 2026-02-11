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
import type { TuiCommand } from '../types/commands.js';
import { runEffect } from './effects.js';

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
  const { inputType } = cmd.meta ?? {};
  const isLocal = cmd.meta?.local === true;
  const cmdName = cmd.name.replace(/^\//, '');

  // 1. Input gathering (when no args provided)
  if (!args) {
    if (inputType === 'selection') {
      try {
        const response = await ctx.kiro.getCommandOptions(cmd.name, '');
        if (response.options.length > 0) {
          ctx.setActiveCommand({ command: cmd, options: response.options });
          return;
        }
      } catch {
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
  runEffect(cmdName, result, ctx);

  // 4. Show result message (skip for panel commands - they show their own UI)
  if (result?.message && inputType !== 'panel') {
    ctx.showAlert(result.message, result.success ? 'success' : 'error', 3000);
  }
}
