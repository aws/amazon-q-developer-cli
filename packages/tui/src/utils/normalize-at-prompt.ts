import type { AvailableCommand } from '../types/commands';

/**
 * Convert `@name args` → `/name args` if `name` matches a known prompt.
 * Only rewrites when the name is followed by a space or end of input:
 * downstream command parsing splits on spaces only, so rewriting across
 * other whitespace would produce a token no command matches.
 */
export const normalizeAtPrompt = (
  input: string,
  commands: readonly AvailableCommand[]
): string => {
  const match = /^@(\S+)( |$)/.exec(input);
  if (!match) return input;
  const lowerName = match[1]!.toLowerCase();
  const prompt = commands.find(
    (c) => c.meta?.type === 'prompt' && c.name.toLowerCase() === `/${lowerName}`
  );
  if (!prompt) return input;
  return prompt.name + input.slice(1 + match[1]!.length);
};
