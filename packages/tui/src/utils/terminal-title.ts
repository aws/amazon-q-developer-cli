/**
 * Dynamic terminal window title management.
 *
 * Gated by the `chat.terminalTitle` setting (default: false). When enabled,
 * writes OSC 0 escape sequences to update the terminal's title bar.
 *
 * Title precedence:
 *   1. User override (sticky via /title <text>, cleared via /title --clear)
 *   2. Backend-persisted session title (read from session JSON on disk)
 *   3. Workspace basename via shortenPath(cwd)
 */

import { readFile } from 'fs/promises';
import { join, sep } from 'path';
import { shortenPath, stripNonPrintable } from './string.js';
import { kiroHomePath } from './kiro-home.js';

export type TitleSetResult =
  | { ok: true; title: string }
  | { ok: false; reason: 'disabled' | 'empty' };

export type TitleClearResult = { ok: true } | { ok: false; reason: 'disabled' };

let isEnabledGetter: () => boolean = () => false;

/** Whether the terminal title feature is active. */
export const isTerminalTitleEnabled = (): boolean => isEnabledGetter();
const MAX_TITLE_LENGTH = 60;

let userOverride: string | undefined;
let lastSessionTitle: string | undefined;
let lastEmitted = '';

const getSessionsDir = (): string => {
  return process.env.KIRO_TEST_SESSIONS_DIR ?? kiroHomePath('sessions', 'cli');
};

const sanitize = (raw: string): string => {
  const cleaned = stripNonPrintable(raw)
    // Collapse runs of whitespace into a single space
    .replace(/\s+/g, ' ')
    // Remove leading/trailing whitespace
    .trim();

  return cleaned.slice(0, MAX_TITLE_LENGTH);
};

const writeOscTitle = (title: string): void => {
  if (!isEnabledGetter()) {
    return;
  }
  // Defence-in-depth: sessionTitle and cwd paths don't pass through sanitize()
  const safe = stripNonPrintable(title);
  if (safe === lastEmitted) {
    return;
  }
  try {
    process.stdout.write(`\x1b]0;${safe}\x07`);
    lastEmitted = safe;
  } catch {
    // stdout may be closed (PTY died)
  }
};

/** Shorten a path for display: use ~ for home, or last 2 segments as fallback. */
const shortenCwd = (cwd: string): string => {
  const shortened = shortenPath(cwd);

  // shortenPath replaces the $HOME prefix with ~. If the path is outside $HOME
  // (e.g. /Volumes/workplace/project), it returns unchanged — fall back to the
  // last 2 segments to keep the title short.
  if (shortened !== cwd) {
    return shortened;
  }

  // Otherwise fall back to the last 2 path segments
  const segments = cwd.split(sep).filter(Boolean);
  if (segments.length <= 2) {
    return cwd;
  }

  return segments.slice(-2).join(sep);
};

/** Compute the title string from the given inputs without side effects. */
export const deriveTitle = (opts: {
  cwd: string;
  sessionTitle?: string;
  override?: string;
}): string => {
  const effective =
    opts.override?.trim() || opts.sessionTitle?.trim() || shortenCwd(opts.cwd);

  const truncated = effective.slice(0, MAX_TITLE_LENGTH);
  return `kiro: ${truncated}`;
};

/** Return the title that would currently be emitted (without writing to stdout). */
export const getCurrentTitle = (): string => {
  return deriveTitle({
    cwd: process.cwd(),
    sessionTitle: lastSessionTitle,
    override: userOverride,
  });
};

/** Read the session title from the persisted session JSON on disk. */
export const readPersistedSessionTitle = async (
  sessionId: string
): Promise<string | undefined> => {
  try {
    const filePath = join(getSessionsDir(), `${sessionId}.json`);
    const data = JSON.parse(await readFile(filePath, 'utf-8'));
    const title = data?.title;

    if (typeof title === 'string' && title.trim()) {
      return stripNonPrintable(title).trim();
    }

    return undefined;
  } catch {
    return undefined;
  }
};

/** Emit the current derived title to the terminal. */
export const emitCurrentTitle = (): void => {
  writeOscTitle(getCurrentTitle());
};

/** Initialize the terminal title module with an isEnabled getter, and emit the initial title. */
export const initTerminalTitle = (opts: { isEnabled: () => boolean }): void => {
  isEnabledGetter = opts.isEnabled;
  emitCurrentTitle();
};

/** Re-read the session title from disk and update the terminal title (unless a user override is active). */
export const refreshFromSession = async (sessionId: string): Promise<void> => {
  // Always update lastSessionTitle even when disabled — it's needed if the user enables the feature later.
  lastSessionTitle = await readPersistedSessionTitle(sessionId);

  if (userOverride) {
    return;
  }

  writeOscTitle(getCurrentTitle());
};

/** Set a sticky manual title override. Returns an error result if disabled or input is empty. */
export const setUserTitle = (title: string): TitleSetResult => {
  if (!isEnabledGetter()) {
    return { ok: false, reason: 'disabled' };
  }

  const sanitized = sanitize(title);

  if (!sanitized) {
    return { ok: false, reason: 'empty' };
  }

  userOverride = sanitized;
  const derived = getCurrentTitle();
  writeOscTitle(derived);

  return { ok: true, title: derived };
};

/** Remove the manual title override and revert to the automatic title. */
export const clearUserTitle = (): TitleClearResult => {
  if (!isEnabledGetter()) {
    return { ok: false, reason: 'disabled' };
  }

  userOverride = undefined;
  writeOscTitle(getCurrentTitle());

  return { ok: true };
};

/** Write an empty OSC 0 sequence to clear the terminal title (called during cleanup or disable). */
export const resetTerminalTitle = (): void => {
  if (!lastEmitted) {
    return;
  }
  try {
    process.stdout.write('\x1b]0;\x07');
    lastEmitted = '';
  } catch {
    // stdout may be closed (PTY died)
  }
};

/** Reset module state between tests. */
export const resetTerminalTitleState = (): void => {
  userOverride = undefined;
  lastSessionTitle = undefined;
  lastEmitted = '';
  isEnabledGetter = () => false;
};
