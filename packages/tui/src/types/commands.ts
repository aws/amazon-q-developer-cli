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
  type?: 'action' | 'prompt' | 'skill' | 'steering';
  arguments?: Array<{
    name: string;
    description?: string;
    required?: boolean;
  }>;
  /**
   * Discriminated source for projected prompt/skill/steering slash commands.
   * Set by the *-to-slash-command mappers in stores/visible-slash-commands.ts.
   * Replaces the legacy stringly-typed `serverName` field for prompt/skill
   * projections.
   */
  source?: PromptSource | SkillSource | SteeringSource;
  /** When true, Menu uses bold instead of accent color for selected items, preserving embedded ANSI colors. */
  preserveLabelColors?: boolean;
  /** When true, command is executable but hidden from autocomplete dropdown. */
  hidden?: boolean;
}

/**
 * Origin of a {@link PromptEntry}. Discriminated union; consumers switch on
 * `kind` rather than parsing strings.
 *
 * - `workspace`: file under <cwd>/.kiro/prompts/*.md (V2) or KAS workspace prompt
 * - `global`:    file under ~/.kiro/prompts/*.md (V2) or KAS global prompt
 * - `mcp`:       prompt advertised by an MCP server; `serverName` is the
 *                upstream MCP server name
 *
 * `path` is optional for workspace/global because neither V2 nor KAS reliably
 * emits a filesystem path today; it can be enriched later without breaking
 * consumers.
 */
export type PromptSource =
  | { kind: 'workspace'; path?: string }
  | { kind: 'global'; path?: string }
  | { kind: 'mcp'; serverName: string };

/** A user-invocable prompt template. */
export interface PromptEntry {
  name: string;
  description?: string;
  arguments: Array<{
    name: string;
    description?: string;
    required?: boolean;
  }>;
  source: PromptSource;
}

/**
 * Origin of a {@link SkillEntry}.
 *
 * - `workspace`/`global`: KAS scope-tagged skills (path enrichment pending)
 * - `agent-config`: V2 skill resource declared in the agent config
 *   (`skill://` URI scheme). The wire shape today does not carry the
 *   resolved file path, so `path` is optional; `_meta.kiro.path`
 *   enrichment from the V2 agent will populate it.
 */
export type SkillSource =
  | { kind: 'workspace'; path?: string }
  | { kind: 'global'; path?: string }
  | { kind: 'agent-config'; path?: string };

/** A skill: agentic instruction set surfaced as a slash command. */
export interface SkillEntry {
  name: string;
  description?: string;
  source: SkillSource;
}

/** Origin of a {@link SteeringEntry}. KAS-only; V2 has no steering concept. */
export type SteeringSource =
  | { kind: 'workspace'; path?: string }
  | { kind: 'global'; path?: string };

/** A KAS steering document: auto-included context. */
export interface SteeringEntry {
  name: string;
  description?: string;
  source: SteeringSource;
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
