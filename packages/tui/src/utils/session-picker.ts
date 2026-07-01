/**
/**
 * Interactive session picker for --resume-picker.
 *
 * Runs before React/Ink takes over the terminal, using raw stdin
 * to let the user arrow-key through sessions and press Enter to select.
 */

import {
  listSessionsForCwd,
  formatSessionEntry,
  formatRelativeTime,
} from './sessions.js';
import type { V2SessionFsEntry } from './sessions.js';
import type { SessionEntry } from './list-all-sessions-cli.js';
import { sanitizeSessionTitleForDisplay } from './sanitize-title.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';

/**
 * Current terminal width for the stderr-based picker. Falls back to 80
 * when unavailable (no TTY or zero-width). Read at render time, not
 * module load, so SIGWINCH-style resizes in long-running pickers are
 * picked up on the next redraw.
 */
function terminalWidth(): number {
  return process.stderr.columns || 80;
}

/**
 * Show an interactive session picker and return the selected session ID.
 *
 * Returns undefined if no sessions exist or the user dismisses the picker
 * with Escape — in which case the caller should fall through to start a new
 * session. Ctrl+C exits the process via the normal SIGINT path.
 */
export async function pickSession(cwd: string): Promise<string | undefined> {
  const sessions = listSessionsForCwd(cwd);
  if (sessions.length === 0) {
    process.stderr.write('No saved sessions found for this directory.\n');
    return undefined;
  }

  return new Promise<string | undefined>((resolve) => {
    const glyphs = getActiveGlyphs();
    let selectedIndex = 0;

    // Cap visible items to fit the terminal, leaving room for header + footer
    const termRows = process.stderr.rows || 24;
    const maxVisible = Math.max(3, termRows - 3); // header + blank + buffer
    let scrollOffset = 0;

    const render = () => {
      const visibleCount = Math.min(sessions.length, maxVisible);
      const totalLines = visibleCount + 2; // header + visible items + blank
      process.stderr.write(`\x1b[${totalLines}A\x1b[J`);
      printMenu(sessions, selectedIndex);
    };

    const printMenu = (entries: V2SessionFsEntry[], selected: number) => {
      process.stderr.write('Select a chat session to resume:\n');

      const visibleCount = Math.min(entries.length, maxVisible);
      for (let vi = 0; vi < visibleCount; vi++) {
        const i = scrollOffset + vi;
        const prefix =
          i === selected ? `\x1b[36m${glyphs.chevron}\x1b[0m ` : '  ';
        const text = formatSessionEntry(entries[i]!, terminalWidth());
        const styled = i === selected ? `\x1b[1m${text}\x1b[0m` : text;
        process.stderr.write(`${prefix}${styled}\n`);
      }
      process.stderr.write('\n');
    };

    const updateScroll = () => {
      const visibleCount = Math.min(sessions.length, maxVisible);
      if (selectedIndex < scrollOffset) {
        scrollOffset = selectedIndex;
      } else if (selectedIndex >= scrollOffset + visibleCount) {
        scrollOffset = selectedIndex - visibleCount + 1;
      }
    };

    // Initial render — print blank lines so the first cursor-up works
    const initialVisible = Math.min(sessions.length, maxVisible);
    for (let i = 0; i < initialVisible + 2; i++) {
      process.stderr.write('\n');
    }
    render();

    const wasRaw = process.stdin.isRaw;
    process.stdin.setRawMode?.(true);
    process.stdin.resume();

    const onData = (data: Buffer) => {
      const key = data.toString();

      if (key === '\x03') {
        // Ctrl+C: raw mode swallows SIGINT, so re-raise it. This routes through
        // the SIGINT handler in index.tsx which runs kiro.close() to terminate
        // the agent process before exiting. Mirrors what the V1 Rust picker
        // (dialoguer/console) does via libc::raise(SIGINT).
        cleanup();
        process.kill(process.pid, 'SIGINT');
        return;
      }

      if (key === '\x1b') {
        // Escape: dismiss picker; caller falls through to a new session
        // (matches the V1 Rust picker behavior).
        cleanup();
        resolve(undefined);
        return;
      }

      if (key === '\r' || key === '\n') {
        cleanup();
        resolve(sessions[selectedIndex]!.sessionId);
        return;
      }

      if (key === '\x1b[A' || key === 'k') {
        selectedIndex = Math.max(0, selectedIndex - 1);
        updateScroll();
        render();
      } else if (key === '\x1b[B' || key === 'j') {
        selectedIndex = Math.min(sessions.length - 1, selectedIndex + 1);
        updateScroll();
        render();
      }
    };

    const cleanup = () => {
      process.stdin.removeListener('data', onData);
      process.stdin.setRawMode?.(wasRaw ?? false);
      process.stdin.pause();
    };

    process.stdin.on('data', onData);
  });
}

/**
 * Render one merged-listing entry as a single line for the picker:
 *   "{relative_time} | {title} | {count} msgs"
 * The line is truncated to `maxWidth` to prevent line wrapping which
 * breaks the picker's redraw. Callers are responsible for passing the
 * terminal width; the function is pure so tests don't need to mock
 * `process.stderr.columns`.
 *
 * Exported for unit testing; not used outside this module.
 */
export function formatMergedEntry(
  entry: SessionEntry,
  maxWidth: number
): string {
  const timestamp = entry.updatedAt
    ? formatRelativeTime(entry.updatedAt)
    : 'unknown';
  const sanitizedTitle = sanitizeSessionTitleForDisplay(entry.title);
  const title = sanitizedTitle || '(no title)';
  const line =
    entry.messageCount && entry.messageCount > 0
      ? `${timestamp} | ${title} | ${entry.messageCount} msgs`
      : `${timestamp} | ${title}`;
  const maxLen = maxWidth - 4;
  if (line.length > maxLen) {
    return line.slice(0, maxLen - 3) + '...';
  }
  return line;
}

/**
 * Show an interactive session picker for cross-engine merged listing
 * entries and return the selected entry.
 *
 * Returns undefined if `entries` is empty or the user dismisses the
 * picker with Escape — caller should fall through to start a new
 * session. Ctrl+C exits the process.
 */
export async function pickSessionFromEntries(
  entries: SessionEntry[]
): Promise<SessionEntry | undefined> {
  if (entries.length === 0) {
    process.stderr.write('No saved sessions found for this directory.\n');
    return undefined;
  }

  return new Promise<SessionEntry | undefined>((resolve) => {
    const glyphs = getActiveGlyphs();
    let selectedIndex = 0;

    const termRows = process.stderr.rows || 24;
    const maxVisible = Math.max(3, termRows - 3);
    let scrollOffset = 0;

    const render = () => {
      const visibleCount = Math.min(entries.length, maxVisible);
      const totalLines = visibleCount + 2;
      process.stderr.write(`\x1b[${totalLines}A\x1b[J`);
      process.stderr.write('Select a chat session to resume:\n');
      for (let vi = 0; vi < visibleCount; vi++) {
        const i = scrollOffset + vi;
        const prefix =
          i === selectedIndex ? `\x1b[36m${glyphs.chevron}\x1b[0m ` : '  ';
        const text = formatMergedEntry(entries[i]!, terminalWidth());
        const styled = i === selectedIndex ? `\x1b[1m${text}\x1b[0m` : text;
        process.stderr.write(`${prefix}${styled}\n`);
      }
      process.stderr.write('\n');
    };

    const updateScroll = () => {
      const visibleCount = Math.min(entries.length, maxVisible);
      if (selectedIndex < scrollOffset) scrollOffset = selectedIndex;
      else if (selectedIndex >= scrollOffset + visibleCount)
        scrollOffset = selectedIndex - visibleCount + 1;
    };

    const initialVisible = Math.min(entries.length, maxVisible);
    for (let i = 0; i < initialVisible + 2; i++) process.stderr.write('\n');
    render();

    const wasRaw = process.stdin.isRaw;
    process.stdin.setRawMode?.(true);
    process.stdin.resume();

    const onData = (data: Buffer) => {
      const key = data.toString();
      if (key === '\x03') {
        // Re-raise SIGINT so the index.tsx handler can run kiro.close() before
        // the process dies. See pickSession() above for the full rationale.
        cleanup();
        process.kill(process.pid, 'SIGINT');
        return;
      }
      if (key === '\x1b') {
        cleanup();
        resolve(undefined);
        return;
      }
      if (key === '\r' || key === '\n') {
        cleanup();
        resolve(entries[selectedIndex]!);
        return;
      }
      if (key === '\x1b[A' || key === 'k') {
        selectedIndex = Math.max(0, selectedIndex - 1);
        updateScroll();
        render();
      } else if (key === '\x1b[B' || key === 'j') {
        selectedIndex = Math.min(entries.length - 1, selectedIndex + 1);
        updateScroll();
        render();
      }
    };

    const cleanup = () => {
      process.stdin.removeListener('data', onData);
      process.stdin.setRawMode?.(wasRaw ?? false);
      process.stdin.pause();
    };

    process.stdin.on('data', onData);
  });
}
