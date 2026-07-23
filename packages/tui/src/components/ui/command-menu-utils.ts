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

/**
 * Query used to match prompt entries in the @ menu. Empty unless the @ is
 * the first character of the input: prompts are commands, and commands are
 * leading-only, so a mid-message @ stays a pure file reference. Also empty
 * once the text after the trigger contains whitespace: prompt items only
 * show while the user is still typing the name, so exactly one Enter owner
 * exists at a time (menu during name-typing, submit once args follow).
 */
export function atMenuPromptQuery(
  text: string,
  trigger: { key: string; position: number } | null | undefined
): string {
  if (trigger?.key !== '@' || trigger.position !== 0) return '';
  const afterAt = text.slice(trigger.position + 1);
  return /\s/.test(afterAt) ? '' : afterAt;
}

/** Whether the @ menu is showing prompt items for the current input. */
export function atMenuShowsPrompts(
  slashCommands: readonly AvailableCommand[],
  text: string,
  trigger: { key: string; position: number } | null | undefined
): boolean {
  return (
    filterPromptsByQuery(slashCommands, atMenuPromptQuery(text, trigger))
      .length > 0
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
