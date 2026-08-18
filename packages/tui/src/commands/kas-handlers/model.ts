import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import { updateCliSetting } from '../../utils/cli-settings';
import { Settings } from '../../constants/settings';
import { extractRpcErrorMessage } from '../../utils/error-handling';
import { logger } from '../../utils/logger';

/**
 * `/model` selection menu + switch for KAS. Options come from the
 * `kasAvailableModels` store slice (parsed from the ACP `configOptions`
 * payload); selecting one performs the switch via
 * `ctx.kiro.setConfigOption('model', …)`, which re-emits the normalized
 * model events so the store self-heals. Validation reads the store back, and
 * rejected switches restore the previously active model.
 */
export async function handleModel(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const trimmed = args.trim();
  if (!trimmed) {
    return showModelPicker(ctx, cmd);
  }
  if (trimmed === 'set-current-as-default') {
    return saveCurrentAsDefault(ctx);
  }
  return switchModel(ctx, trimmed);
}

function showModelPicker(ctx: CommandContext, cmd: KasCommand): void {
  if (ctx.kasAvailableModels.length === 0) {
    // Cloud: the sandbox owns the model surface and reports it via the
    // config self-heal shortly after attach — an empty list is
    // a not-yet, not a failure.
    if (ctx.cloudSessionActive) {
      ctx.showAlert(
        'Waiting for the sandbox to report its models — try again in a moment',
        'warning',
        4000
      );
      return;
    }
    ctx.showAlert('No models available', 'error', 3000);
    return;
  }
  const currentId = ctx.getCurrentModel?.()?.id;
  const options = ctx.kasAvailableModels.map((m) => {
    const isActive = m.id === currentId;
    const desc = m.description ?? '';
    // Right-aligned credits column (mirrors v2's `to_command_option`): a rate
    // multiplier renders as e.g. "0.25x credits"; absent rate data renders the
    // "----- credits" placeholder so the column stays aligned.
    const credits =
      m.rateMultiplier !== undefined
        ? `${m.rateMultiplier.toFixed(2)}x credits`
        : '----- credits';
    return {
      value: m.id,
      label: m.name,
      description: isActive ? (desc ? `[active] ${desc}` : '[active]') : desc,
      group: credits,
    };
  });
  ctx.setActiveCommand({ command: cmd, options });
}

async function switchModel(
  ctx: CommandContext,
  modelId: string
): Promise<void> {
  const previousModel = ctx.getCurrentModel?.();
  try {
    await ctx.kiro.setConfigOption('model', modelId);
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch model'),
      'error',
      5000
    );
    return;
  }
  // The store only adopts a currentValue that matches an available model.
  // Restore the server when a rejected switch leaves the display unchanged.
  const current = ctx.getCurrentModel?.();
  if (current?.id !== modelId) {
    let restoredModel: typeof previousModel;
    let restoreFailed = false;
    if (previousModel && current?.id === previousModel.id) {
      try {
        await ctx.kiro.setConfigOption('model', previousModel.id);
        if (ctx.getCurrentModel?.()?.id === previousModel.id) {
          restoredModel = previousModel;
        } else {
          restoreFailed = true;
        }
      } catch (err) {
        restoreFailed = true;
        logger.warn(
          `[model] failed to restore ${previousModel.id} after rejected switch:`,
          err
        );
      }
    }
    const message = restoredModel
      ? `The model '${modelId}' is not available. Continuing with ${restoredModel.name}.`
      : restoreFailed
        ? `The model '${modelId}' is not available, and ${previousModel?.name ?? 'the previous model'} could not be restored. Run /model to select a model.`
        : `Model '${modelId}' not available`;
    ctx.showAlert(message, 'error', 5000);
    return;
  }
  ctx.showAlert(`Switched to ${current.name}`, 'success', 3000);
}

async function saveCurrentAsDefault(ctx: CommandContext): Promise<void> {
  const current = ctx.getCurrentModel?.();
  if (!current) {
    ctx.showAlert('Select a model to save as the default', 'error', 3000);
    return;
  }
  // Persisting is the whole command, so a failed write is reported as an
  // error - but gracefully, never as an unhandled throw.
  try {
    await updateCliSetting(Settings.CHAT_DEFAULT_MODEL, current.id);
  } catch (err) {
    logger.warn('[model] failed to save default model:', err);
    ctx.showAlert(
      `Failed to save ${current.name} as default model`,
      'error',
      5000
    );
    return;
  }
  ctx.showAlert(`Saved ${current.name} as default model`, 'success', 3000);
}
