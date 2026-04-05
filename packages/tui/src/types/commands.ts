/**
 * Slash command types for ACP protocol integration
 * Uses strongly-typed generated types from Rust via typeshare
 */

export type {
  CommandOption,
  CommandOptionsResponse,
  CommandResult,
  TuiCommand,
  ModelArgs,
  ContextArgs,
  CompactArgs,
  HelpArgs,
  AgentArgs,
  ClearArgs,
  QuitArgs,
  UsageArgs,
  PlanArgs,
} from './generated/agent';

import type { CommandOption } from './generated/agent';

/** Command metadata for rich UI features */
export interface CommandMeta {
  optionsMethod?: string;
  inputType?: 'text' | 'selection' | 'multiselect' | 'panel';
  searchable?: boolean;
  subcommands?: string[];
  subcommandHints?: Record<string, string>;
  hint?: string;
  local?: boolean;
  type?: 'action' | 'prompt' | 'skill';
  arguments?: Array<{
    name: string;
    description?: string;
    required?: boolean;
  }>;
  serverName?: string;
  /** When true, Menu uses bold instead of accent color for selected items, preserving embedded ANSI colors. */
  preserveLabelColors?: boolean;
}

/** Command advertised by backend */
export interface AvailableCommand {
  name: string;
  description: string;
  meta?: CommandMeta;
}

/** Check if the first token after "/" looks like a file path rather than a command name. */
function looksLikeFilePath(afterSlash: string): boolean {
  const spaceIndex = afterSlash.indexOf(' ');
  const firstToken =
    spaceIndex === -1 ? afterSlash : afterSlash.slice(0, spaceIndex);
  return (
    firstToken.includes('/') ||
    firstToken.includes('\\') ||
    firstToken.includes('.')
  );
}

/** Parse command from input text */
export function parseCommand(input: string): {
  isCommand: boolean;
  name: string;
  args: string;
} {
  const trimmed = input.trim();
  if (!trimmed.startsWith('/')) {
    return { isCommand: false, name: '', args: '' };
  }

  const withoutSlash = trimmed.slice(1);

  // Treat input as a regular message if the first token looks like a file path
  // (contains path separators or dots), matching V1 behavior.
  if (looksLikeFilePath(withoutSlash)) {
    return { isCommand: false, name: '', args: '' };
  }

  const spaceIndex = withoutSlash.indexOf(' ');

  if (spaceIndex === -1) {
    return { isCommand: true, name: withoutSlash, args: '' };
  }

  return {
    isCommand: true,
    name: withoutSlash.slice(0, spaceIndex),
    args: withoutSlash.slice(spaceIndex + 1),
  };
}

/** Group options by their group field */
export function groupOptions(
  options: CommandOption[]
): Map<string, CommandOption[]> {
  const groups = new Map<string, CommandOption[]>();

  for (const opt of options) {
    const group = opt.group || 'Other';
    const existing = groups.get(group) || [];
    existing.push(opt);
    groups.set(group, existing);
  }

  return groups;
}

/** Filter commands by partial name match */
export function filterCommands(
  commands: AvailableCommand[],
  partial: string
): AvailableCommand[] {
  const lower = partial.toLowerCase();
  return commands.filter(
    (cmd) =>
      cmd.name.toLowerCase().startsWith(lower) ||
      cmd.description.toLowerCase().includes(lower)
  );
}
