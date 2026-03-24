/**
 * Language names that cli-highlight doesn't support natively.
 * When encountered, we pass 'plaintext' to disable highlighting entirely.
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
 * Returns the language to pass to cli-highlight, or 'plaintext' to disable
 * highlighting for unsupported/non-code languages (avoids auto-detect which
 * would incorrectly highlight prose as code keywords).
 */
export function resolveHighlightLanguage(
  language?: string
): string | undefined {
  if (!language) return 'plaintext';
  // Strip backticks and whitespace that can leak in from mid-fence parsing
  const clean = language.replace(/`/g, '').trim();
  if (!clean) return 'plaintext';
  // Only accept clean language identifiers (letters, digits, +, -, #, .)
  if (!/^[a-zA-Z0-9+\-#.]+$/.test(clean)) return 'plaintext';
  return UNSUPPORTED_HIGHLIGHT_LANGUAGES.has(clean.toLowerCase())
    ? 'plaintext'
    : clean;
}
