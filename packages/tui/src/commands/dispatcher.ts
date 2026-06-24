/**
 * Command dispatcher.
 *
 * Single function that handles all command execution:
 * 1. Input gathering (selection menu, panel)
 * 2. Backend execution
 * 3. Effect execution
 */

import type { CommandContext } from './types.js';
import type { AvailableCommand, TuiCommand } from '../types/commands.js';
import { runEffect } from './effects.js';
import { kasHandlers } from './kas-handlers/index.js';
import { handleChat as handleV2Chat } from './v2-handlers/chat.js';
import { isKasCommand, KasCommandName } from '../kas-commands.js';
import { startPTTRecording } from './voice-helper.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';

export interface DispatchOptions {
  /** True when args were provided programmatically rather than typed by the user. */
  argIsSynthetic?: boolean;
}

/**
 * Cross-dispatch chain slot.
 *
 * Used by `/effort` when the active model is `auto`: we open the `/model`
 * picker first, stash `{ name: 'effort', args: '' }` here, and re-fire
 * `/effort` once the model is chosen. The chain only runs when the
 * trigger command (`/model`) lands a real selection — cancellation
 * (esc) leaves the slot untouched and the next /model dispatch will
 * still consume it.
 */
let pendingChainedCommand: { name: string; args: string } | null = null;

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
      const handlerEmitsTelemetry = kasHandlerEmitsCommandUsage(cmd.name, args);
      try {
        await handler(cmd, args, ctx, options);
        if (!handlerEmitsTelemetry) {
          emitFrontendCommandUsage(cmd, args, ctx, true);
        }
      } catch (error) {
        if (!handlerEmitsTelemetry) {
          emitFrontendCommandUsage(cmd, args, ctx, false);
        }
        throw error;
      }
      return;
    }
  }

  // Drop a stale chain if the user dispatches anything other than /model
  // (or the /effort that just set the chain). Esc'ing the /model picker
  // would otherwise leave the slot armed across unrelated commands.
  if (
    pendingChainedCommand &&
    cmdName !== 'model' &&
    cmdName !== pendingChainedCommand.name
  ) {
    pendingChainedCommand = null;
  }

  // V2 /chat intercept: a parallel handler owns the full /chat flow
  // (picker, save/load delegation, new, ensure-session conversion)
  // when the active engine is V2. Mirrors the KAS intercept above.
  if (cmd.name === '/chat') {
    const shouldEmit = !isV2ChatBackendDelegated(args);
    try {
      await handleV2Chat(cmd, args, ctx, options);
      if (shouldEmit) emitFrontendCommandUsage(cmd, args, ctx, true);
    } catch (error) {
      if (shouldEmit) emitFrontendCommandUsage(cmd, args, ctx, false);
      throw error;
    }
    return;
  }

  // Handle prompt, skill, and steering commands - send as regular message,
  // backend resolves via session/prompt interception
  if (type === 'prompt' || type === 'skill' || type === 'steering') {
    const message = args ? `/${cmdName} ${args}` : `/${cmdName}`;
    await ctx.sendMessage(message);
    emitFrontendCommandUsage(cmd, args, ctx, true);
    return;
  }

  // /effort with no args on the auto model dead-ends with "Effort
  // configuration is currently not available on auto. Select a /model
  // that supports effort..." Picker doesn't open, user has to /model
  // first then /effort. Chain it: open /model picker, then re-run
  // /effort once a real model is selected.
  if (cmdName === 'effort' && !args && ctx.getCurrentModel?.()?.id === 'auto') {
    const modelCmd = ctx.slashCommands.find((c) => c.name === '/model');
    if (modelCmd) {
      pendingChainedCommand = { name: 'effort', args: '' };
      await dispatch(modelCmd, '', ctx);
      return;
    }
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
      emitFrontendCommandUsage(cmd, args, ctx, false);
      return;
    }
    emitFrontendCommandUsage(cmd, args, ctx, true);
    return;
  }

  // 1. Input gathering (when no args provided)
  if (!args) {
    if (inputType === 'selection') {
      try {
        ctx.setLoadingMessage(`Loading ${cmdName} options...`);
        const { options } = await ctx.kiro.getCommandOptions(cmd.name, '');
        ctx.setLoadingMessage(null);
        if (options.length > 0) {
          ctx.setActiveCommand({ command: cmd, options });
          emitFrontendCommandUsage(cmd, args, ctx, true);
          return;
        }
        if (cmdName === 'effort') {
          // Fall through to execute — backend returns a descriptive error
        } else {
          ctx.showAlert(`No options available for /${cmdName}`, 'error', 3000);
          emitFrontendCommandUsage(cmd, args, ctx, false);
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
  // 2. Execute backend (skip for local commands).
  let result = null;
  if (!isLocal) {
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
      if (shouldEmitFrontendCommandUsage(ctx, isLocal)) {
        emitFrontendCommandUsage(cmd, args, ctx, false);
      }
      return;
    }
    ctx.setLoadingMessage(null);
  }
  if (shouldEmitFrontendCommandUsage(ctx, isLocal)) {
    emitFrontendCommandUsage(cmd, args, ctx, result?.success ?? true);
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

  // 5. Consume chained command. The /model picker opened above (no args)
  // returned early before reaching here, so this branch only fires when
  // /model was just dispatched with a chosen model AND the backend
  // accepted the selection. Re-fire the chained command (currently only
  // /effort uses this path).
  if (pendingChainedCommand && cmdName === 'model' && args && result?.success) {
    const next = pendingChainedCommand;
    pendingChainedCommand = null;
    const nextCmd = ctx.slashCommands.find((c) => c.name === `/${next.name}`);
    if (nextCmd) {
      await dispatch(nextCmd, next.args, ctx);
    }
  }
}

function shouldEmitFrontendCommandUsage(
  ctx: CommandContext,
  isLocal: boolean
): boolean {
  return ctx.agentEngine === 'kas' || isLocal;
}

function kasHandlerEmitsCommandUsage(
  commandName: string,
  args: string
): boolean {
  return (
    commandName === KasCommandName.Prompts &&
    /^(prompt|skill|steering):/.test(args.trim())
  );
}

function isV2ChatBackendDelegated(args: string): boolean {
  const token = firstArgToken(args);
  return token === 'save' || token === 'load';
}

function emitFrontendCommandUsage(
  cmd: AvailableCommand,
  args: string,
  ctx: CommandContext,
  success: boolean
): void {
  const command = telemetryCommandName(cmd);
  const subcommand = telemetrySubcommand(cmd, args);
  ctx.kiro.sendChatSlashCommandTelemetry({
    command,
    ...(subcommand && { subcommand }),
    success,
    ...(!success && { reason: 'CommandFailed' }),
  });
}

function telemetryCommandName(cmd: AvailableCommand): string {
  const type = cmd.meta?.type;
  if (type === 'prompt' || type === 'skill' || type === 'steering') {
    return `/${type}`;
  }
  return cmd.name.toLowerCase();
}

function telemetrySubcommand(
  cmd: AvailableCommand,
  args: string
): string | undefined {
  const token = firstArgToken(args);
  if (!token) return undefined;

  return knownSubcommand(token, cmd.meta?.subcommands ?? []);
}

function firstArgToken(args: string): string | undefined {
  return args.trim().split(/\s+/, 1)[0]?.toLowerCase();
}

function knownSubcommand(
  token: string,
  allowed: readonly string[]
): string | undefined {
  return allowed.some((value) => value.toLowerCase() === token)
    ? token
    : undefined;
}
