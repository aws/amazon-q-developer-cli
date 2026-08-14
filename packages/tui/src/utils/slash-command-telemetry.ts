import type { AvailableCommand } from '../types/commands.js';
import { SlashCommandMetricName } from '../types/generated/telemetry.js';

export const SLASH_COMMAND_METRIC_NAMES = Object.values(SlashCommandMetricName);

const SLASH_COMMAND_METRIC_NAME_SET: ReadonlySet<string> = new Set(
  SLASH_COMMAND_METRIC_NAMES
);

export function canonicalSlashCommandName(command: string): string {
  const normalized = command.trim().toLowerCase();
  const candidate = normalized.startsWith('/') ? normalized : `/${normalized}`;
  return SLASH_COMMAND_METRIC_NAME_SET.has(candidate)
    ? candidate
    : SlashCommandMetricName.Custom;
}

export function commandMetricName(command: AvailableCommand): string {
  if (command.meta?.telemetryId) {
    return canonicalSlashCommandName(command.meta.telemetryId);
  }
  const type = command.meta?.type;
  if (type === 'prompt' || type === 'skill' || type === 'steering') {
    return `/${type}`;
  }
  return canonicalSlashCommandName(command.name);
}
