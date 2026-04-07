import type { SlashCommand } from '../stores/app-store';

/** Convert `@name args` → `/name args` if `name` matches a known prompt. */
export const normalizeAtPrompt = (
  input: string,
  commands: SlashCommand[]
): string => {
  if (!input.startsWith('@')) return input;
  const name = input.slice(1).split(/\s/, 1)[0]!;
  if (commands.some((c) => c.meta?.type === 'prompt' && c.name === `/${name}`))
    return '/' + input.slice(1);
  return input;
};
