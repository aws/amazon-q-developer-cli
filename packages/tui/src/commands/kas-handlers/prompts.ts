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
  const selection = resolveSelection(trimmed, ctx);
  try {
    await ctx.sendMessage(`/${selection.name}`);
    if (selection.source === 'picker') {
      emitSelectionTelemetry(ctx, selection.type, true);
    }
  } catch (error) {
    if (selection.source === 'picker') {
      emitSelectionTelemetry(ctx, selection.type, false);
    }
    throw error;
  }
}

type PickerEntry =
  | { type: 'prompt'; entry: PromptEntry }
  | { type: 'skill'; entry: SkillEntry }
  | { type: 'steering'; entry: SteeringEntry };

type Selection = {
  name: string;
  source: 'picker' | 'manual';
  type: PickerEntry['type'];
};

const PICKER_VALUE_SEPARATOR = ':';

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
        value: pickerValue(type, entry.name),
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

function pickerValue(type: PickerEntry['type'], name: string): string {
  return `${type}${PICKER_VALUE_SEPARATOR}${name}`;
}

function resolveSelection(raw: string, ctx: CommandContext): Selection {
  const encoded = decodePickerValue(raw);
  if (encoded) {
    return { ...encoded, source: 'picker' };
  }

  if (ctx.skills.some((entry) => entry.name === raw)) {
    return { name: raw, source: 'manual', type: 'skill' };
  }
  if (ctx.steering.some((entry) => entry.name === raw)) {
    return { name: raw, source: 'manual', type: 'steering' };
  }
  return { name: raw, source: 'manual', type: 'prompt' };
}

function decodePickerValue(
  raw: string
): Pick<Selection, 'name' | 'type'> | null {
  const splitAt = raw.indexOf(PICKER_VALUE_SEPARATOR);
  if (splitAt <= 0) return null;

  const type = raw.slice(0, splitAt);
  const name = raw.slice(splitAt + 1);
  if (!name) return null;
  if (type === 'prompt' || type === 'skill' || type === 'steering') {
    return { type, name };
  }
  return null;
}

function emitSelectionTelemetry(
  ctx: CommandContext,
  type: PickerEntry['type'],
  success: boolean
): void {
  ctx.kiro.sendChatSlashCommandTelemetry({
    command: `/${type}`,
    success,
    ...(!success && { reason: 'CommandFailed' }),
  });
}
