import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type {
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from '../../types/commands';

/**
 * `/prompts` selection menu for KAS. Surfaces every user-invocable
 * template (prompts, skills, steering) the active engine has advertised
 * via `available_commands_update` and routed into the typed AppState
 * slices. Picking an entry sends `/<name>` as a chat message; the
 * backend's session/prompt interception resolves it into the underlying
 * template body.
 */
export async function handlePrompts(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const trimmed = args.trim();
  if (!trimmed) {
    return showPromptsPicker(ctx, cmd);
  }
  // Synthetic dispatch from the picker: `args` is the picked entry name.
  await ctx.sendMessage(`/${trimmed}`);
}

type PickerEntry =
  | { type: 'prompt'; entry: PromptEntry }
  | { type: 'skill'; entry: SkillEntry }
  | { type: 'steering'; entry: SteeringEntry };

function showPromptsPicker(ctx: CommandContext, cmd: KasCommand): void {
  const entries: PickerEntry[] = [
    ...ctx.prompts.map((entry): PickerEntry => ({ type: 'prompt', entry })),
    ...ctx.skills.map((entry): PickerEntry => ({ type: 'skill', entry })),
    ...ctx.steering.map((entry): PickerEntry => ({ type: 'steering', entry })),
  ];

  if (entries.length === 0) {
    ctx.showAlert('No prompts, skills, or steering available', 'error', 3000);
    return;
  }

  const options = entries
    .map(({ type, entry }) => {
      const args = type === 'prompt' ? entry.arguments : [];
      const hint =
        args
          .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
          .join(' ') || undefined;
      // Group label: prompts use their scope (mcp scope shows the
      // server name); skills and steering use the entity type so each
      // category renders as its own group in the menu.
      const group =
        type === 'prompt'
          ? entry.source.kind === 'mcp'
            ? entry.source.serverName
            : entry.source.kind
          : type;
      return {
        value: entry.name,
        label: `/${entry.name}`,
        description: entry.description ?? '',
        group,
        hint,
      };
    })
    .sort(
      (a, b) =>
        (a.group ?? '').localeCompare(b.group ?? '') ||
        a.label.toLowerCase().localeCompare(b.label.toLowerCase())
    );

  ctx.setActiveCommand({ command: cmd, options });
}
