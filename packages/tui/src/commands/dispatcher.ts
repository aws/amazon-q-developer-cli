/**
 * Command dispatcher.
 *
 * Single function that handles all command execution:
 * 1. Input gathering (selection menu, panel)
 * 2. Backend execution
 * 3. Effect execution
 */

import type { CommandContext } from './types.js';
import type {
  AvailableCommand,
  TuiCommand,
  CommandOption,
} from '../types/commands.js';
import { runEffect } from './effects.js';
import { kasHandlers } from './kas-handlers/index.js';
import { isKasCommand } from '../kas-commands.js';
import { startPTTRecording } from './voice-helper.js';
import { formatRelativeTime } from '../utils/sessions.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';

export interface DispatchOptions {
  /** True when args were provided programmatically rather than typed by the user. */
  argIsSynthetic?: boolean;
}

/**
 * Dispatch a command through the standard flow.
 *
 * @param cmd - Slash command definition
 * @param args - Arguments (empty string if none)
 * @param ctx - Command context
 * @param options - Optional dispatch metadata
 */
export async function dispatch(
  cmd: AvailableCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
): Promise<void> {
  const { inputType, type } = cmd.meta ?? {};
  const isLocal = cmd.meta?.local === true;
  const cmdName = cmd.name.replace(/^\//, '');

  // KAS intercept: in KAS mode, registered handlers own the command flow
  // and skip the V2 dispatcher pipeline entirely.
  if (ctx.agentEngine === 'kas' && isKasCommand(cmd)) {
    const handler = kasHandlers[cmd.name];
    if (handler) {
      await handler(cmd, args, ctx, options);
      return;
    }
  }

  // Handle prompt and skill commands - send as regular message, backend resolves via session/prompt interception
  if (type === 'prompt' || type === 'skill') {
    const message = args ? `/${cmdName} ${args}` : `/${cmdName}`;
    await ctx.sendMessage(message);
    return;
  }

  // Voice command: spawn local voice helper, capture text, place in input or auto-submit
  if (cmdName === 'voice') {
    try {
      const remoteServerUrl = process.env.KIRO_VOICE_SERVER_URL ?? undefined;
      let downloadOnly = false;
      let cancelled = false;
      const session = startPTTRecording(
        remoteServerUrl,
        {
          onLevel: (level: number) => {
            ctx.setVoiceLevel(level);
          },
          onPartial: (text: string) => {
            ctx.setVoicePartialText(text);
          },
          onStatus: (status: string) => {
            if (status === 'recording') {
              ctx.setVoiceLevel(0);
            } else if (status === 'downloading') {
              ctx.showAlert('Downloading voice model...', 'success', 120000);
            } else if (status === 'download_complete') {
              downloadOnly = true;
              ctx.showAlert(
                'Voice model ready! Hold Space or type /voice to start recording.',
                'success',
                8000
              );
            }
          },
        },
        false
      );
      ctx.setVoiceStop(session.stop);
      ctx.setVoiceCancel(() => {
        cancelled = true;
        session.cancel();
      });
      const text = await session.text;
      ctx.setVoiceStop(null);
      ctx.setVoiceCancel(null);
      ctx.setVoiceLevel(null);
      ctx.setVoicePartialText(null);
      if (downloadOnly || cancelled) return;
      ctx.incrementVoiceHint();
      if (text) {
        if (ctx.voiceAutoSubmit) {
          await ctx.sendMessage(text);
        } else {
          ctx.setPendingVoiceText(text);
        }
      } else {
        ctx.showAlert('No speech detected', 'error', 2000);
      }
    } catch (error) {
      ctx.setVoiceStop(null);
      ctx.setVoiceCancel(null);
      ctx.setVoiceLevel(null);
      const msg = error instanceof Error ? error.message : 'Voice input failed';
      ctx.showAlert(msg, 'error', 3000);
    }
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
        if (cmdName === 'chat') {
          ctx.showAlert('No previous sessions found', 'error', 3000);
          return;
        }
        if (cmdName === 'effort') {
          // Fall through to execute — backend returns a descriptive error
        } else {
          ctx.showAlert(`No options available for /${cmdName}`, 'error', 3000);
          return;
        }
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
  // 2. Execute backend (skip for local commands, but /chat subcommands go to backend)
  const isChatSubcommand =
    cmdName === 'chat' && args && /^(save|load)\b/.test(args);
  let result = null;
  if (!isLocal || isChatSubcommand) {
    // Show loading for agent swap
    const isSubcommand =
      args === 'create' ||
      args === 'edit' ||
      args.startsWith('create ') ||
      args.startsWith('edit ');
    if (cmdName === 'agent' && args && !isSubcommand) {
      const displayName = args.startsWith('swap ') ? args.slice(5) : args;
      ctx.setLoadingMessage(`Agent changing to ${displayName}`);
    }
    if (cmdName === 'guide') {
      ctx.setLoadingMessage('Switching agent...');
    }
    try {
      result = await ctx.kiro.executeCommand({
        command: cmdName,
        args: args ? { value: args } : {},
      } as TuiCommand);
    } catch (error) {
      const message = extractRpcErrorMessage(error, 'Command failed');
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
