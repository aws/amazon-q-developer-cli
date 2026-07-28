/**
 * Split slash-command arguments while preserving quoted and escaped values.
 *
 * This mirrors V2's Rust `shell_split`: quote delimiters are removed,
 * backslash escapes preserve the next character, and an unclosed quote keeps
 * the accumulated token.
 */
export function shellSplit(input: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  let hasContent = false;

  for (let index = 0; index < input.length; index += 1) {
    const character = input.charAt(index);
    if (quote) {
      if (character === quote) quote = null;
      else current += character;
    } else if (character === '"' || character === "'") {
      quote = character;
      hasContent = true;
    } else if (character === '\\' && index + 1 < input.length) {
      current += input.charAt((index += 1));
      hasContent = true;
    } else if (/\s/.test(character)) {
      if (hasContent) tokens.push(current);
      current = '';
      hasContent = false;
    } else {
      current += character;
      hasContent = true;
    }
  }

  if (hasContent) tokens.push(current);
  return tokens;
}
