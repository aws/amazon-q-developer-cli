import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import { formatEffort } from '../../utils/string';
import { extractRpcErrorMessage } from '../../utils/error-handling';
import { readCliSettings } from '../../utils/cli-settings';
import { Settings } from '../../constants/settings';
import { persistEffortDefault } from '../../utils/effort-defaults';
import { logger } from '../../utils/logger';

/**
 * `/effort` selection menu + switch for KAS. Levels come from the
 * `kasAvailableEfforts` store slice (parsed from the `effortLevel`
 * configOption, present only when the active model declares a thought-level
 * schema). Selecting a level switches via
 * `ctx.kiro.setConfigOption('effortLevel', …)`.
 */
export async function handleEffort(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const trimmed = args.trim();
  if (!trimmed) {
    return showEffortPicker(ctx, cmd);
  }
  return switchEffort(ctx, trimmed);
}

function showEffortPicker(ctx: CommandContext, cmd: KasCommand): void {
  if (ctx.kasAvailableEfforts.length === 0) {
    ctx.showAlert(
      'Effort is not available on the current model. Select a model that supports effort levels.',
      'error',
      5000
    );
    return;
  }
  const currentLevel = ctx.getCurrentEffort?.();
  const options = ctx.kasAvailableEfforts.map((o) => ({
    value: o.value,
    label: o.name,
    description: o.value === currentLevel ? '[active]' : '',
  }));
  ctx.setActiveCommand({ command: cmd, options });
}

async function switchEffort(ctx: CommandContext, level: string): Promise<void> {
  if (ctx.kasAvailableEfforts.length === 0) {
    ctx.showAlert(
      'Effort is not available on the current model. Select a model that supports effort levels.',
      'error',
      5000
    );
    return;
  }
  try {
    await ctx.kiro.setConfigOption('effortLevel', level);
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to set effort'),
      'error',
      5000
    );
    return;
  }
  // KAS silently ignores invalid levels, leaving currentValue unchanged.
  if (ctx.getCurrentEffort?.() !== level) {
    ctx.showAlert(`Effort '${level}' not available`, 'error', 5000);
    return;
  }
  // Best-effort persist of the per-model default at the model's advertised
  // effort schema path; no path advertised -> skip rather than guess. The
  // switch already took effect, so a failed write never fails the command;
  // the suffix is shown only when a write actually happened.
  const model = ctx.getCurrentModel?.();
  const optedOut =
    readCliSettings()[Settings.CHAT_DISABLE_AUTO_DEFAULT_EFFORT] === true;
  let savedForModel = false;
  if (model && !optedOut) {
    const entry = ctx.kasAvailableModels.find((m) => m.id === model.id);
    if (entry?.effortSchemaPath) {
      try {
        await persistEffortDefault(
          model.id,
          level,
          `${entry.effortSchemaPath}.effort`
        );
        savedForModel = true;
      } catch (err) {
        logger.warn('[effort] failed to persist effort default:', err);
      }
    } else {
      logger.debug(
        `[effort] no effortSchemaPath advertised for ${model.id}; skipping persist`
      );
    }
  }
  if (savedForModel) {
    ctx.showAlert(
      `Effort set to ${formatEffort(level)} (saved for ${model!.name}; disable with kiro-cli settings ${Settings.CHAT_DISABLE_AUTO_DEFAULT_EFFORT} true)`,
      'success',
      4000
    );
  } else {
    ctx.showAlert(`Effort set to ${formatEffort(level)}`, 'success', 3000);
  }
}
