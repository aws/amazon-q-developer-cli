import type { AvailableCommand } from '../../types/commands';

export interface AtMenuItem {
  label: string;
  description: string;
  group?: string;
}

/**
 * Whether a slash command should surface in the menu for the current UI mode.
 * Single source of truth shared by CommandMenu (which filters its list) and
 * PromptInput (which must back off Enter/Tab in sync with what's rendered).
 * liteOnly commands bind to lite-mode rendering hooks and would no-op/error in
 * TUI, so they're hidden there.
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
