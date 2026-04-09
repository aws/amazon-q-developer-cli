import type { SlashCommand } from '../../stores/app-store';

export interface AtMenuItem {
  label: string;
  description: string;
  group?: string;
}

export function filterPromptsByQuery(
  slashCommands: SlashCommand[],
  atQuery: string
): SlashCommand[] {
  if (!atQuery) return [];
  const lower = atQuery.toLowerCase();
  return slashCommands.filter(
    (cmd) =>
      cmd.meta?.type === 'prompt' &&
      cmd.name.slice(1).toLowerCase().startsWith(lower)
  );
}

export function buildAtMenuItems(
  filteredPrompts: SlashCommand[],
  fileResults: string[]
): AtMenuItem[] {
  const promptItems = filteredPrompts.map((cmd) => ({
    label: cmd.name.slice(1),
    description: cmd.description,
    group: 'Prompt' as const,
  }));
  const fileItems = fileResults.map((path) => ({
    label: path,
    description: '',
  }));
  return [...promptItems, ...fileItems];
}

/**
 * Find a prompt SlashCommand by its menu item label.
 * Handles labels with or without leading `/`.
 */
export function findPromptByMenuLabel(
  slashCommands: SlashCommand[],
  label: string
): SlashCommand | undefined {
  const name = label.startsWith('/') ? label : `/${label}`;
  return slashCommands.find(
    (c) => c.name === name && c.meta?.type === 'prompt'
  );
}
