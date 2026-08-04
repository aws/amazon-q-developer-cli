import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import { formatEffort } from '../../utils/string';
import { extractRpcErrorMessage } from '../../utils/error-handling';
import { persistEffortDefault } from '../../utils/effort-defaults';
import { logger } from '../../utils/logger';

/**
 * `/effort` selection menu + switch for KAS. Levels come from the
 * `kasAvailableEfforts` store slice (parsed from the `effortLevel`
 * configOption, present only when the active model declares a thought-level
 * schema).
 *
 * Selecting a level switches via
 * `ctx.kiro.setConfigOption('effortLevel', …)`.
 *
 * Switching is session-only;
 * `/effort set-current-as-default` persists the current level for the current model
 * to the user's settings.
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
  if (trimmed === 'set-current-as-default') {
    return saveCurrentAsDefault(ctx);
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
  const currentModel = ctx.getCurrentModel?.();
  const modelEntry = currentModel
    ? ctx.kasAvailableModels.find((m) => m.id === currentModel.id)
    : undefined;
  const defaultLevel = modelEntry?.defaultEffortLevel;

  const options = ctx.kasAvailableEfforts.map((o) => {
    const isActive = o.value === currentLevel;
    const isDefault = o.value === defaultLevel;
    const description = isActive ? '[active]' : isDefault ? '[default]' : '';
    return {
      value: o.value,
      label: formatEffort(o.value),
      description,
    };
  });
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
  ctx.showAlert(`Effort set to ${formatEffort(level)}`, 'success', 3000);
}

async function saveCurrentAsDefault(ctx: CommandContext): Promise<void> {
  const model = ctx.getCurrentModel?.();
  if (!model) {
    ctx.showAlert(
      'Select a model with an effort configured to save as the default',
      'error',
      3000
    );
    return;
  }
  const level = ctx.getCurrentEffort?.();
  if (!level) {
    ctx.showAlert(
      `No effort level is currently set. Effort may not be available on ${model.name}.`,
      'error',
      3000
    );
    return;
  }
  const entry = ctx.kasAvailableModels.find((m) => m.id === model.id);
  if (!entry?.effortSchemaPath) {
    ctx.showAlert(
      `Effort defaults are not available for ${model.name}`,
      'error',
      5000
    );
    return;
  }
  // Persisting is the whole command, so a failed write is reported as an
  // error - but gracefully, never as an unhandled throw.
  try {
    await persistEffortDefault(
      model.id,
      level,
      `${entry.effortSchemaPath}.effort`
    );
  } catch (err) {
    logger.warn('[effort] failed to save default effort:', err);
    ctx.showAlert(
      `Failed to save ${formatEffort(level)} as default effort for ${model.name}`,
      'error',
      5000
    );
    return;
  }
  ctx.showAlert(
    `Set ${formatEffort(level)} as default effort for ${model.name}`,
    'success',
    3000
  );
}
