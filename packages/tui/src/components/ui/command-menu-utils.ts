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
