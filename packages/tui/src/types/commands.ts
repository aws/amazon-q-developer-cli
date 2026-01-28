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
} from './generated/agent';

import type { CommandOption } from './generated/agent';

/** Command metadata for rich UI features */
export interface CommandMeta {
  optionsMethod?: string;
  inputType?: 'text' | 'selection' | 'multiselect' | 'panel';
  subcommands?: string[];
  hint?: string;
}

/** Command advertised by backend */
export interface AvailableCommand {
  name: string;
  description: string;
  meta?: CommandMeta;
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
export function groupOptions(options: CommandOption[]): Map<string, CommandOption[]> {
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
