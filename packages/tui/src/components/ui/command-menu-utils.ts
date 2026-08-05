import type { AvailableCommand } from '../../types/commands';

export interface AtMenuItem {
  label: string;
  description: string;
  group?: string;
}

export interface PromptMenuVisibilityInput {
  activeCommandOpen: boolean;
  activeTrigger:
    | { key: string; position: number; type?: 'start' | 'inline' }
    | null
    | undefined;
  commandInputValue: string;
  filePickerHasResults: boolean;
  slashCommands: readonly AvailableCommand[];
  uiMode: 'lite' | 'tui' | undefined;
}

export interface PromptMenuState {
  activeCommand: unknown | null;
  activeTrigger: PromptMenuVisibilityInput['activeTrigger'];
  commandInputValue: string;
  filePickerHasResults: boolean;
  uiMode: PromptMenuVisibilityInput['uiMode'];
}

/**
 * Whether a slash command should surface for the current UI mode. Hidden
 * compatibility commands never surface; liteOnly commands stay out of TUI.
 */
export function isCommandVisibleInUiMode(
  cmd: AvailableCommand,
  uiMode: 'lite' | 'tui' | undefined
): boolean {
  return (
    cmd.meta?.hidden !== true &&
    (uiMode === 'lite' || cmd.meta?.liteOnly !== true)
  );
}

/**
 * Substring match so namespaced entries (e.g. /agent-sop:pdd) surface when
 * the user types any part of the name, not just the leading prefix.
 */
export function commandMatchesQuery(cmdName: string, query: string): boolean {
  return cmdName.slice(1).toLowerCase().includes(query.toLowerCase());
}

/** 0 = prefix match, 1 = other substring match. Lower sorts first. */
export function commandMatchRank(cmdName: string, query: string): number {
  return cmdName.slice(1).toLowerCase().startsWith(query.toLowerCase()) ? 0 : 1;
}

const isPromptLike = (cmd: AvailableCommand): boolean =>
  cmd.meta?.type === 'prompt' ||
  cmd.meta?.type === 'skill' ||
  cmd.meta?.type === 'steering';

/**
 * Filter and order slash-menu entries. Rank is compared across the whole
 * list — not per group — so a prefix-matched prompt always beats a
 * substring-matched command; within a rank, commands sort before prompts,
 * then alphabetical. Hidden entries only surface if prompt-like.
 */
export function filterSlashMenuCommands(
  slashCommands: readonly AvailableCommand[],
  partial: string
): AvailableCommand[] {
  return slashCommands
    .filter(
      (cmd) =>
        commandMatchesQuery(cmd.name, partial) &&
        (isPromptLike(cmd) || !cmd.meta?.hidden)
    )
    .sort(
      (a, b) =>
        commandMatchRank(a.name, partial) - commandMatchRank(b.name, partial) ||
        Number(isPromptLike(a)) - Number(isPromptLike(b)) ||
        a.name.localeCompare(b.name)
    );
}

export function filterPromptsByQuery(
  slashCommands: readonly AvailableCommand[],
  atQuery: string
): AvailableCommand[] {
  if (!atQuery) return [];
  return slashCommands
    .filter(
      (cmd) =>
        cmd.meta?.type === 'prompt' && commandMatchesQuery(cmd.name, atQuery)
    )
    .sort(
      (a, b) =>
        commandMatchRank(a.name, atQuery) - commandMatchRank(b.name, atQuery) ||
        a.name.localeCompare(b.name)
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

export function isPromptMenuOpen({
  activeCommandOpen,
  activeTrigger,
  commandInputValue,
  filePickerHasResults,
  slashCommands,
  uiMode,
}: PromptMenuVisibilityInput): boolean {
  if (activeCommandOpen) return true;

  if (
    activeTrigger?.key === '/' &&
    !commandInputValue.includes(' ') &&
    slashCommands.some(
      (cmd) =>
        isCommandVisibleInUiMode(cmd, uiMode) &&
        commandMatchesQuery(cmd.name, commandInputValue.slice(1))
    )
  ) {
    return true;
  }

  return (
    activeTrigger?.key === '@' &&
    (filePickerHasResults ||
      atMenuShowsPrompts(slashCommands, commandInputValue, activeTrigger))
  );
}

export function isPromptMenuOpenForState(
  state: PromptMenuState,
  slashCommands: readonly AvailableCommand[]
): boolean {
  return isPromptMenuOpen({
    activeCommandOpen: state.activeCommand != null,
    activeTrigger: state.activeTrigger,
    commandInputValue: state.commandInputValue,
    filePickerHasResults: state.filePickerHasResults,
    slashCommands,
    uiMode: state.uiMode,
  });
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
