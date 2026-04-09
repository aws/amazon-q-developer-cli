/**
 * Filesystem path completion for Tab key in the TUI input.
 *
 * Given the current input text and cursor position, extracts the path token
 * at the cursor and returns matching filesystem entries.
 */

import { readdirSync, statSync } from 'fs';
import { dirname, basename, join, resolve } from 'path';
import { homedir } from 'os';

/** Whether backslash is a path separator (Windows) vs escape char (Unix). Exported for test mocking. */
export let isWindows = process.platform === 'win32';

/** @internal Override platform detection for testing */
export function _setIsWindows(value: boolean): void {
  isWindows = value;
}

/** Check if a string contains a path separator (/ on all platforms, \ on Windows) */
function containsPathSep(s: string): boolean {
  return s.includes('/') || (isWindows && s.includes('\\'));
}

export interface CompletionResult {
  /** Start index of the path token in the input string */
  start: number;
  /** The replacement string (completed path) */
  replacement: string;
  /** All matching candidates (for cycling or display) */
  candidates: string[];
}

/**
 * Extract the path token at the cursor position.
 * A path token is a contiguous sequence ending at the cursor, where
 * backslash-escaped spaces (\ ) are treated as part of the token.
 * If the initial token doesn't resolve to a valid path, tries extending
 * backward past unescaped spaces to handle paths with literal spaces.
 */
/** @internal Exported for testing */
export function extractPathToken(
  text: string,
  cursor: number
): { token: string; start: number } {
  // Walk backward from cursor to find the start of the token,
  // treating "\ " (backslash + space) as part of the path.
  let start = cursor;

  // Handle quoted paths: if cursor is inside or right after a quoted string,
  // use the opening quote as the token boundary (strip the quote itself).
  // This handles Windows-style "C:\path with spaces\file" and also works on Unix.
  const textBeforeCursor = text.slice(0, cursor);
  const lastQuote = textBeforeCursor.lastIndexOf('"');
  if (lastQuote !== -1) {
    // Check there's no closing quote between the opening quote and cursor
    const afterQuote = textBeforeCursor.slice(lastQuote + 1);
    if (!afterQuote.includes('"')) {
      const token = afterQuote;
      return { token, start: lastQuote + 1 };
    }
  }

  while (start > 0) {
    const ch = text[start - 1];
    if (ch === '\n') break;
    if (ch === ' ') {
      // Check if this space is escaped by a preceding backslash (Unix only —
      // on Windows, backslash is a path separator, not an escape character).
      if (!isWindows && start >= 2 && text[start - 2] === '\\') {
        start -= 2; // skip past the "\ "
        continue;
      }
      break;
    }
    start--;
  }
  const token = text.slice(start, cursor);

  // If the token looks like it could be a continuation of a path with
  // literal spaces (e.g., user typed "/tmp/test dir" without escaping),
  // try extending backward past spaces to find a valid path prefix.
  // Only attempt this if the text before the token contains a path separator,
  // avoiding unnecessary readdirSync calls for plain words.
  if (
    start > 0 &&
    (text.lastIndexOf('/', start - 1) !== -1 ||
      (isWindows && text.lastIndexOf('\\', start - 1) !== -1))
  ) {
    let extStart = start;
    while (extStart > 0 && text[extStart - 1] !== '\n') {
      // Skip past the space and previous word
      if (text[extStart - 1] === ' ') {
        extStart--;
      }
      while (
        extStart > 0 &&
        text[extStart - 1] !== ' ' &&
        text[extStart - 1] !== '\n'
      ) {
        extStart--;
      }
      const candidate = text.slice(extStart, cursor);
      const raw = unescapePath(candidate);
      // Check if the extended token looks like a path and its directory exists
      if (containsPathSep(raw) || raw.startsWith('~')) {
        try {
          const expanded = raw.startsWith('~/')
            ? join(homedir(), raw.slice(1))
            : raw;
          const dir =
            expanded.endsWith('/') || (isWindows && expanded.endsWith('\\'))
              ? resolve(expanded)
              : resolve(dirname(expanded));
          readdirSync(dir);
          // Valid directory — use the extended token
          return { token: candidate, start: extStart };
        } catch {
          // Not a valid path, keep trying
        }
      }
    }
  }

  return { token, start };
}

/**
 * Unescape a shell-escaped path (reverse of escapePath).
 * On Windows, backslashes are path separators, not escape characters,
 * so escaping/unescaping is skipped.
 */
/** @internal Exported for testing */
export function unescapePath(p: string): string {
  if (isWindows) return p;
  return p.replace(/\\ /g, ' ').replace(/\\\\/g, '\\');
}

/**
 * Resolve a partial path token to an absolute directory + prefix for matching.
 */
function resolvePartial(token: string): { dir: string; prefix: string } | null {
  // Unescape shell escapes before resolving on the filesystem
  let expanded = unescapePath(token);
  if (expanded.startsWith('~/') || expanded === '~') {
    expanded = join(homedir(), expanded.slice(1));
  }

  const resolved = resolve(expanded);

  // If the token ends with a path separator, list that directory
  if (token.endsWith('/') || (isWindows && token.endsWith('\\'))) {
    return { dir: resolved, prefix: '' };
  }

  // Otherwise, complete the basename in the parent directory
  return { dir: dirname(resolved), prefix: basename(resolved) };
}

/**
 * Find the longest common prefix among a list of strings.
 */
function commonPrefix(strings: string[]): string {
  if (strings.length === 0) return '';
  if (strings.length === 1) return strings[0]!;
  let prefix = strings[0]!;
  for (let i = 1; i < strings.length; i++) {
    const s = strings[i]!;
    let j = 0;
    while (j < prefix.length && j < s.length && prefix[j] === s[j]) j++;
    prefix = prefix.slice(0, j);
    if (!prefix) return '';
  }
  return prefix;
}

/**
 * Escape spaces in a path for shell-like input.
 * On Windows, backslashes are path separators so only spaces are escaped.
 */
/** @internal Exported for testing */
export function escapePath(p: string): string {
  if (isWindows) return p; // Windows handles spaces via quoting, not escaping
  return p.replace(/\\/g, '\\\\').replace(/ /g, '\\ ');
}

/**
 * Compute tab completion for the given input text and cursor position.
 * Returns null if no completion is available.
 */
export function completePathAtCursor(
  text: string,
  cursor: number
): CompletionResult | null {
  const { token, start } = extractPathToken(text, cursor);

  // Don't complete when there's no token at all (empty input or cursor after space with nothing typed)
  if (!token && start === 0 && text.trim() === '') return null;

  // Unescape for filesystem operations — the token may contain "\ " from previous completions
  const rawToken = unescapePath(token);

  // Resolve directory and prefix for matching
  let dir: string;
  let prefix: string;

  if (!rawToken) {
    // Empty token (cursor after space) — complete in cwd
    dir = resolve('.');
    prefix = '';
  } else if (
    rawToken.startsWith('/') ||
    rawToken.startsWith('./') ||
    rawToken.startsWith('../') ||
    rawToken.startsWith('~') ||
    containsPathSep(rawToken)
  ) {
    // Explicit path — resolve normally
    const parsed = resolvePartial(token);
    if (!parsed) return null;
    dir = parsed.dir;
    prefix = parsed.prefix;
  } else {
    // Bare prefix (e.g. ".", "..", "Car") — match in cwd
    dir = resolve('.');
    prefix = rawToken;
  }

  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return null;
  }

  // Filter entries matching the prefix
  const matches = prefix
    ? entries.filter((e) => e.startsWith(prefix))
    : entries.filter((e) => !e.startsWith('.'));

  if (matches.length === 0) return null;

  // Build full path candidates relative to the original token style
  const candidates = matches.map((name) => {
    let isDir = false;
    try {
      isDir = statSync(join(dir, name)).isDirectory();
    } catch {
      // ignore
    }

    // Reconstruct the token prefix (everything before the basename)
    const lastSepIdx = isWindows
      ? Math.max(rawToken.lastIndexOf('/'), rawToken.lastIndexOf('\\'))
      : rawToken.lastIndexOf('/');
    const tokenDir =
      rawToken.endsWith('/') || (isWindows && rawToken.endsWith('\\'))
        ? rawToken
        : rawToken.slice(0, lastSepIdx + 1);
    return escapePath(tokenDir + name) + (isDir ? '/' : '');
  });

  // Find common prefix of all candidates
  const common = commonPrefix(candidates);

  // If common prefix is the same as the current token (escaped), no progress — return candidates for cycling
  if (common === token && candidates.length > 1) {
    return { start, replacement: token, candidates };
  }

  return {
    start,
    replacement: common,
    candidates,
  };
}
