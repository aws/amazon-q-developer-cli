import type { AvailableCommand } from '../../types/commands';

export interface AtMenuItem {
  label: string;
  description: string;
  group?: string;
}

/**
 * Whether a slash command should surface for the current UI mode. Shared by
 * CommandMenu (filters its list) and PromptInput (backs off Enter/Tab in sync);
 * liteOnly commands bind lite-only rendering hooks, so they're hidden in TUI.
 */
export function isCommandVisibleInUiMode(
  cmd: AvailableCommand,
  uiMode: 'lite' | 'tui' | undefined
): boolean {
  return uiMode === 'lite' || cmd.meta?.liteOnly !== true;
}

export function filterPromptsByQuery(
  slashCommands: readonly AvailableCommand[],
  atQuery: string
): AvailableCommand[] {
  if (!atQuery) return [];
  const lower = atQuery.toLowerCase();
  return slashCommands.filter(
    (cmd) =>
      cmd.meta?.type === 'prompt' &&
      cmd.name.slice(1).toLowerCase().startsWith(lower)
  );
}

export function buildAtMenuItems(
  filteredPrompts: readonly AvailableCommand[],
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
 * Find a prompt by its menu item label. Handles labels with or without
 * leading `/`.
 */
export function findPromptByMenuLabel(
  slashCommands: readonly AvailableCommand[],
  label: string
): AvailableCommand | undefined {
  const name = label.startsWith('/') ? label : `/${label}`;
  return slashCommands.find(
    (c) => c.name === name && c.meta?.type === 'prompt'
  );
}
