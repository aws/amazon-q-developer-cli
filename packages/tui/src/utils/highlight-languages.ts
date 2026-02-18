/**
 * Language names that cli-highlight doesn't support.
 * Used to skip explicit language detection and fall back to auto-detect.
 */
export const UNSUPPORTED_HIGHLIGHT_LANGUAGES = new Set([
  'markdown',
  'md',
  'text',
  'plaintext',
  'plain',
  'txt',
  'output',
  'console',
  'log',
  'none',
  '',
]);

/**
 * Returns the language to pass to cli-highlight, or undefined for auto-detect.
 */
export function resolveHighlightLanguage(
  language?: string
): string | undefined {
  if (!language) return undefined;
  return UNSUPPORTED_HIGHLIGHT_LANGUAGES.has(language.toLowerCase())
    ? undefined
    : language;
}
