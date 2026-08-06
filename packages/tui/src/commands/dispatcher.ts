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
import { isKasCommand } from '../kas-commands.js';
import { startPTTRecording, type ModelDownloadInfo } from './voice-helper.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import {
  getCommandPanelState,
  hasCommandEffect,
  isKasHandlerCommandName,
} from './command-registry.js';

/**
 * Run one local voice capture round.
 *
 * On first use the speech model isn't present. Rather than downloading
 * silently, the subprocess emits `needs_download`; we surface an interactive
 * confirm gate (Yes downloads, No declines). Accepting re-runs this with
 * `confirmDownload=true`, which proceeds through download → "ready" and the user
 * runs /voice again to actually record.
 */
async function runVoiceCapture(
  ctx: CommandContext,
  confirmDownload: boolean
): Promise<void> {
  const remoteServerUrl = process.env.KIRO_VOICE_SERVER_URL ?? undefined;
  let terminal = false; // true when the round ended without a transcript (download/confirm/cancel)
  let cancelled = false;

  const session = startPTTRecording(
    remoteServerUrl,
    {
      onLevel: (level: number) => ctx.setVoiceLevel(level),
      onPartial: (text: string) => ctx.setVoicePartialText(text),
      onNeedsDownload: (info: ModelDownloadInfo) => {
        terminal = true;
        promptModelDownload(ctx, info);
      },
      onStatus: (status: string) => {
        if (status === 'recording') {
          ctx.setVoiceLevel(0);
        } else if (status === 'downloading') {
          ctx.showAlert(
            'Downloading voice model — this runs once, then voice is ready.',
            'success',
            120000
          );
        } else if (status === 'download_complete') {
          terminal = true;
          ctx.showAlert(
            'Voice model ready! Hold Space or type /voice to start recording.',
            'success',
            8000
          );
        }
      },
    },
    false,
    confirmDownload
  );
  ctx.setVoiceStop(session.stop);
  ctx.setVoiceCancel(() => {
    cancelled = true;
    session.cancel();
  });

  let text: string | null | undefined;
  try {
    text = await session.text;
  } finally {
    // Always tear down the recording UI, even when session.text rejects
    // (e.g. a model-download failure) — otherwise the level meter / ghost
    // text would stick around after the round ends.
    ctx.setVoiceStop(null);
    ctx.setVoiceCancel(null);
    ctx.setVoiceLevel(null);
    ctx.setVoicePartialText(null);
  }

  if (terminal || cancelled) return;
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
}

/**
 * Surface the interactive "download the speech model?" confirm gate. The gate
 * (a dedicated menu that owns the keyboard) resolves via Yes/No: accepting kicks
 * off the download; declining leaves voice off with a hint.
 */
function promptModelDownload(
  ctx: CommandContext,
  info: ModelDownloadInfo
): void {
  ctx.setVoiceDownloadConfirm({
    info,
    onConfirm: () => {
      ctx.setVoiceDownloadConfirm(null);
      ctx.showAlert('Downloading voice model…', 'success', 120000);
      // Re-run with confirmation. Fire-and-forget: errors surface via alert.
      runVoiceCapture(ctx, true).catch((error) => {
        const msg =
          error instanceof Error ? error.message : 'Voice download failed';
        ctx.showAlert(msg, 'error', 3000);
      });
    },
    onDecline: () => {
      ctx.setVoiceDownloadConfirm(null);
      ctx.showAlert(
        'Voice needs a one-time model download. Run /voice again when ready.',
        'warning',
        6000
      );
    },
  });
}

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

  // Cloud-only commands are hidden from autocomplete outside cloud sessions,
  // but prefix-typed input (e.g. `/d` → /disconnect) still resolves them —
  // refuse here so they can't fire against a local session.
  if (cmd.meta?.cloudOnly && !ctx.cloudSessionActive) {
    return;
  }

  // Local-only commands (e.g. `/workflow*`) are hidden from autocomplete inside
  // a cloud session, but prefix-typed input still resolves them — refuse here
  // so they can't fire against a cloud sandbox where KAS's workflow handlers
  // walk local filesystem paths that don't exist (kiro-agent #178).
  if (cmd.meta?.localOnly && ctx.cloudSessionActive) {
    return;
  }

  if (
    inputType === 'panel' &&
    (!getCommandPanelState(cmdName) ||
      (!hasCommandEffect(cmdName) &&
        !(
          ctx.agentEngine === 'kas' &&
          isKasCommand(cmd) &&
          isKasHandlerCommandName(cmd.name)
        )))
  ) {
    ctx.showAlert(
      `Unsupported panel command /${cmdName}: no shared frontend panel is registered.`,
      'error',
      5000
    );
    return;
  }

  // KAS intercept: in KAS mode, registered handlers own the command flow
  // and skip the V2 dispatcher pipeline entirely.
  if (
    ctx.agentEngine === 'kas' &&
    isKasCommand(cmd) &&
    isKasHandlerCommandName(cmd.name)
  ) {
    await kasHandlers[cmd.name](cmd, args, ctx, options);
    return;
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
    await handleV2Chat(cmd, args, ctx, options);
    return;
  }

  // Handle prompt, skill, and steering commands - send as regular message,
  // backend resolves via session/prompt interception
  if (type === 'prompt' || type === 'skill' || type === 'steering') {
    const message = args ? `/${cmdName} ${args}` : `/${cmdName}`;
    await ctx.sendMessage(message);
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
      await runVoiceCapture(ctx, false);
    } catch (error) {
      // runVoiceCapture tears down its own UI state in a finally; here we just
      // surface the failure (e.g. model download failed) to the user.
      const msg = error instanceof Error ? error.message : 'Voice input failed';
      ctx.showAlert(msg, 'error', 3000);
      return;
    }
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
  // 2. Execute backend (skip for local commands).
  let result = null;
  if (!isLocal) {
    // Show loading for agent swap
    const isSubcommand =
      args === 'create' ||
      args === 'edit' ||
      args.startsWith('create ') ||
      args.startsWith('edit ');
    // Only clear the loading message the dispatcher itself set. Commands whose
    // progress is owned by an out-of-band event stream must not be cleared
    // here: `/compact` returns its RPC immediately (the backend spawns the
    // summarization async) while a `compaction_status: started` event has
    // already set `loadingMessage` to "Compacting conversation...". A blanket
    // clear here nulls that loader a few ms after it appears, so the spinner
    // never shows for the whole multi-second compaction (worst in lite, which
    // never paints the flash at all). KAS dodged this because `/compact` is a
    // kas-handler intercept that never reaches this generic path.
    let dispatcherSetLoading = false;
    if (cmdName === 'agent' && args && !isSubcommand) {
      const displayName = args.startsWith('swap ') ? args.slice(5) : args;
      ctx.setLoadingMessage(`Agent changing to ${displayName}`);
      dispatcherSetLoading = true;
    }
    if (cmdName === 'guide') {
      ctx.setLoadingMessage('Switching agent...');
      dispatcherSetLoading = true;
    }
    try {
      result = await ctx.kiro.executeCommand({
        command: cmdName,
        args: args ? { value: args } : {},
      } as TuiCommand);
    } catch (error) {
      const message = extractRpcErrorMessage(error, 'Command failed');
      // On error the command won't proceed, so clear any loader unconditionally
      // (a failed /compact should not leave a "Compacting..." spinner up).
      ctx.setLoadingMessage(null);
      ctx.showAlert(message, 'error');
      return;
    }
    // Success: only clear a loader the dispatcher itself set. See the comment
    // above — an out-of-band event stream (e.g. compaction_status) may own the
    // loader for work that continues after this RPC resolves.
    if (dispatcherSetLoading) ctx.setLoadingMessage(null);
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
