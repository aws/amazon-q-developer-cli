/**
 * Slash-command projections for the TUI's typed prompt / skill /
 * steering slices.
 *
 * `AppState.slashCommands` holds host commands and V2 backend commands
 * only (see app-store.ts). Prompts, skills, and steering live in their
 * own typed slices and are merged into the visible slash-command list
 * at read time via {@link selectVisibleSlashCommands}. This module
 * isolates the mappers + merge so it can be imported by both selector
 * hooks (`selectors.ts`) and the in-store dispatch context builder
 * (`app-store.ts`) without circular imports — the AppState reference
 * here is type-only and erases at runtime.
 */

import type { AppState } from './app-store.js';
import type {
  AvailableCommand,
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from '../types/commands.js';

/** AppState slice the merge needs. Pick'd off AppState so any future
 *  field renames or shape changes there flow into the merge signature
 *  automatically. */
type VisibleSlashCommandsState = Pick<
  AppState,
  | 'agentEngine'
  | 'kasCommands'
  | 'slashCommands'
  | 'prompts'
  | 'skills'
  | 'steering'
  | 'cloudSessionActive'
>;

/**
 * Project a {@link PromptEntry} to {@link AvailableCommand}. The
 * discriminated `source` is preserved on `meta.source`; the dispatcher
 * routes on `meta.type === 'prompt'`.
 */
function promptToSlashCommand(prompt: PromptEntry): AvailableCommand {
  const groupLabel =
    prompt.source.kind === 'mcp'
      ? prompt.source.serverName
      : prompt.source.kind;
  return {
    name: `/${prompt.name}`,
    description: prompt.description || `Prompt from ${groupLabel}`,
    meta: {
      type: 'prompt',
      arguments: prompt.arguments,
      source: prompt.source,
    },
  };
}

function skillToSlashCommand(skill: SkillEntry): AvailableCommand {
  return {
    name: `/${skill.name}`,
    description:
      skill.description || `Skill from ${skill.source.kind.replace('-', ' ')}`,
    meta: {
      type: 'skill',
      source: skill.source,
    },
  };
}

function steeringToSlashCommand(steering: SteeringEntry): AvailableCommand {
  return {
    name: `/${steering.name}`,
    description: steering.description || `Steering (${steering.source.kind})`,
    meta: {
      type: 'steering',
      source: steering.source,
    },
  };
}

/**
 * Returns the slash commands the autocomplete should show for the
 * current engine. The `slashCommands` slice holds host commands +
 * V2-backend commands only; `prompts` / `skills` / `steering` slices
 * are merged in here at read time. In KAS mode, `kasCommands` (the
 * static + capability-filtered KAS list) is prepended.
 *
 * Duplicates by `name` are dropped: the first occurrence wins, so host
 * + backend commands take precedence over typed-slice projections, and
 * within the slices the precedence is prompts > skills > steering. This
 * keeps the dispatcher's `find(c => c.name === ...)` deterministic and
 * prevents React duplicate-key warnings in the autocomplete menu.
 *
 * The returned list is what the dispatcher sees too, so consumers
 * looking up `/research` (a prompt) by name in `ctx.slashCommands`
 * find it because of this merge. CommandMenu (`CommandMenu.tsx`)
 * partitions the result back into regular commands (sorted
 * alphabetically) and prompt/skill/steering commands (kept in this
 * merge order) at render time.
 */
export const selectVisibleSlashCommands = (
  state: VisibleSlashCommandsState
): readonly AvailableCommand[] => {
  const promptCmds = state.prompts.map(promptToSlashCommand);
  const skillCmds = state.skills.map(skillToSlashCommand);
  const steeringCmds = state.steering.map(steeringToSlashCommand);
  const ordered =
    state.agentEngine === 'kas'
      ? [
          ...state.kasCommands,
          ...state.slashCommands,
          ...promptCmds,
          ...skillCmds,
          ...steeringCmds,
        ]
      : [...state.slashCommands, ...promptCmds, ...skillCmds, ...steeringCmds];
  const seen = new Set<string>();
  const deduped: AvailableCommand[] = [];
  for (const cmd of ordered) {
    if (seen.has(cmd.name)) continue;
    // Cloud-only commands (e.g. `/repo`) are hidden from autocomplete unless the
    // current session is a cloud session — so existing local-only users
    // never see them (dark-ship). `cloudSessionActive` is false on released builds.
    if (cmd.meta?.cloudOnly && !state.cloudSessionActive) continue;
    seen.add(cmd.name);
    deduped.push(cmd);
  }
  return deduped;
};
