/**
 * Normalize line endings to \n (handles \r\n and \r)
 */
export const normalizeLineEndings = (str: string): string =>
  expandTabs(str.replace(/\r\n/g, '\n').replace(/\r/g, '\n'));

/**
 * Expand tab characters to spaces, respecting tab stop positions.
 *
 * Terminals render tabs by advancing to the next tab stop (every `tabWidth`
 * columns), but `string-width` (used by Ink) reports them as width 0. This
 * mismatch causes Ink to undercount line widths, leading to incorrect wrapping
 * and rendering artifacts. Converting tabs to the correct number of spaces
 * makes the measured width match the visual width.
 */
export function expandTabs(text: string, tabWidth = 2): string {
  if (!text.includes('\t')) return text;
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (!line.includes('\t')) continue;
    let result = '';
    let col = 0;
    for (const ch of line) {
      if (ch === '\t') {
        const spaces = tabWidth - (col % tabWidth);
        result += ' '.repeat(spaces);
        col += spaces;
      } else {
        result += ch;
        col++;
      }
    }
    lines[i] = result;
  }
  return lines.join('\n');
}

/**
 * Check if string contains only printable characters (including newlines and tabs)
 */
export const isPrintable = (str: string): boolean =>
  Array.from(str).every((c) => {
    const code = c.codePointAt(0)!;
    // Allow: tab (9), newline (10), carriage return (13), and anything >= 32
    // except DEL (127) and C1 control characters (128-159)
    return (
      code === 9 ||
      code === 10 ||
      code === 13 ||
      (code >= 32 && code !== 127 && !(code >= 128 && code <= 159))
    );
  });

/**
 * Strip zero-width and non-printable Unicode characters that have no visual
 * representation but inflate string length, breaking wrapping calculations.
 *
 * Preserves: printable chars, tab (U+0009), newline (U+000A), space (U+0020).
 */
/* eslint-disable no-control-regex */
const NON_PRINTABLE_RE =
  /[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u00AD\u200B-\u200F\u2028-\u202F\u2060-\u206F\uFEFF\uFFF9-\uFFFB]/g;
/* eslint-enable no-control-regex */
export const stripNonPrintable = (str: string): string =>
  str.replace(NON_PRINTABLE_RE, '');

/**
 * Characters that macOS Finder (and common shells) escape with a backslash
 * when producing a shell-safe path string for drag-and-drop.
 *
 * This is intentionally limited to the characters Finder actually escapes
 * to avoid false-positives on pasted shell commands or regexes.
 */
const SHELL_ESCAPED_CHARS = new Set([
  ' ',
  '(',
  ')',
  '[',
  ']',
  '{',
  '}',
  '!',
  '&',
  '|',
  ';',
  "'",
  '"',
  '$',
  '`',
  '#',
  '\\',
]);

/**
 * Detect whether a pasted string looks like a shell-escaped file path
 * (e.g. from dragging a file out of macOS Finder into the terminal)
 * and unescape it so the user sees the clean path.
 *
 * Heuristic — all of these must hold:
 *  1. Single line (no embedded newlines).
 *  2. After trimming / stripping optional surrounding single-quotes,
 *     the string starts with `/` or `~` (absolute or home-relative path).
 *  3. Every `\` in the string is followed by a character from the known
 *     set that Finder escapes. This prevents false-positives on pasted
 *     shell commands, regexes, or other text that happens to start with `/`.
 *  4. There are no unescaped spaces — a Finder-dragged path escapes every
 *     space, so an unescaped space indicates this is a command, not a path.
 *
 * On Windows the function is a no-op because Windows paths start with a
 * drive letter (e.g. `C:\`) and Windows terminals don't shell-escape
 * drag-and-drop paths.
 */
export function unescapeShellPath(text: string): string {
  // Only operate on single-line strings
  if (text.includes('\n')) return text;

  let s = text.trim();

  // Strip surrounding single quotes: '/path/to/file' → /path/to/file
  if (s.length >= 2 && s.startsWith("'") && s.endsWith("'")) {
    s = s.slice(1, -1);
  }

  // Must look like an absolute or home-relative path
  if (!s.startsWith('/') && !s.startsWith('~')) return text;

  // No backslash escapes — return the (possibly unquoted) path
  if (!s.includes('\\')) return s;

  // Verify every backslash is followed by a Finder-escaped character.
  // If any backslash precedes something unexpected (e.g. `\d`, `\w`)
  // this is likely a regex or command, not a dragged path.
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '\\') {
      const next = s[i + 1];
      if (next === undefined || !SHELL_ESCAPED_CHARS.has(next)) {
        return text;
      }
      i++; // skip the escaped char
    }
  }

  // A Finder-dragged path has no unescaped spaces (every space is `\ `).
  // If there's an unescaped space, this is probably a command like
  // `/usr/bin/grep foo\ bar baz`.
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '\\') {
      i++; // skip escaped char
    } else if (s[i] === ' ') {
      return text; // unescaped space → not a Finder path
    }
  }

  // All checks passed — unescape
  return s.replace(/\\(.)/g, '$1');
}

/**
 * Shorten a file path by replacing the home directory with ~
 * @param path - The full path to shorten
 * @returns The shortened path with ~ for home directory
 */
export const shortenPath = (path: string): string => {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) return path;

  // Replace home directory with ~
  if (path.startsWith(home)) {
    return path.replace(home, '~');
  }

  return path;
};
