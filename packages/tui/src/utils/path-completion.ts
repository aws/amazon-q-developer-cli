/**
 * Filesystem path completion for Tab key in the TUI input.
 *
 * Given the current input text and cursor position, extracts the path token
 * at the cursor and returns matching filesystem entries.
 */

import { readdirSync, statSync } from 'fs';
import { dirname, basename, join, resolve } from 'path';
import { homedir } from 'os';

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
 * A path token is a contiguous non-whitespace sequence ending at the cursor.
 */
function extractPathToken(
  text: string,
  cursor: number
): { token: string; start: number } | null {
  // Walk backward from cursor to find the start of the token
  let start = cursor;
  while (start > 0 && text[start - 1] !== ' ' && text[start - 1] !== '\n') {
    start--;
  }
  const token = text.slice(start, cursor);
  if (!token) return null;
  return { token, start };
}

/**
 * Resolve a partial path token to an absolute directory + prefix for matching.
 */
function resolvePartial(token: string): { dir: string; prefix: string } | null {
  // Expand ~ to home directory
  let expanded = token;
  if (expanded.startsWith('~/') || expanded === '~') {
    expanded = join(homedir(), expanded.slice(1));
  }

  const resolved = resolve(expanded);

  // If the token ends with /, list that directory
  if (token.endsWith('/')) {
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
 */
function escapePath(p: string): string {
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
  const extracted = extractPathToken(text, cursor);
  if (!extracted) return null;

  const { token, start } = extracted;

  // Only complete tokens that look like paths
  if (
    !token.startsWith('/') &&
    !token.startsWith('./') &&
    !token.startsWith('../') &&
    !token.startsWith('~') &&
    !token.includes('/')
  ) {
    return null;
  }

  const parsed = resolvePartial(token);
  if (!parsed) return null;

  const { dir, prefix } = parsed;

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
    const tokenDir = token.endsWith('/')
      ? token
      : token.slice(0, token.lastIndexOf('/') + 1);
    return escapePath(tokenDir + name) + (isDir ? '/' : '');
  });

  // Find common prefix of all candidates
  const common = commonPrefix(candidates);

  // If common prefix is the same as the current token (escaped), no progress — return candidates for cycling
  const currentEscaped = escapePath(token);
  if (common === currentEscaped && candidates.length > 1) {
    return { start, replacement: token, candidates };
  }

  return {
    start,
    replacement: common,
    candidates,
  };
}
