import type { AvailableCommand } from '../types/commands.js';

export const SLASH_COMMAND_METRIC_NAMES = [
  '/agent',
  '/autonomous',
  '/changelog',
  '/chat',
  '/checkpoint',
  '/clear',
  '/code',
  '/compact',
  '/context',
  '/context-gatherer',
  '/copy',
  '/disconnect',
  '/editor',
  '/effort',
  '/exit',
  '/experiment',
  '/feedback',
  '/general-task-execution',
  '/goal',
  '/guide',
  '/help',
  '/hooks',
  '/issue',
  '/knowledge',
  '/lite',
  '/load',
  '/logdump',
  '/mcp',
  '/memories',
  '/model',
  '/paste',
  '/plan',
  '/prompt',
  '/prompts',
  '/quit',
  '/reply',
  '/repo',
  '/rewind',
  '/save',
  '/session-id',
  '/sessions',
  '/settings',
  '/skill',
  '/spawn',
  '/spec',
  '/stats',
  '/steering',
  '/switch',
  '/tangent',
  '/theme',
  '/title',
  '/todos',
  '/tools',
  '/transcript',
  '/tui',
  '/upgrade-agent',
  '/usage',
  '/verbosity',
  '/voice',
  '/workflow',
  '/workflow-cancel',
  '/workflow-resume',
  '/workflow-run',
  '/workflow-status',
  '/workflows',
] as const;

const SLASH_COMMAND_METRIC_NAME_SET: ReadonlySet<string> = new Set(
  SLASH_COMMAND_METRIC_NAMES
);

export function canonicalSlashCommandName(command: string): string {
  const normalized = command.trim().toLowerCase();
  const candidate = normalized.startsWith('/') ? normalized : `/${normalized}`;
  return SLASH_COMMAND_METRIC_NAME_SET.has(candidate) ? candidate : '/custom';
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
