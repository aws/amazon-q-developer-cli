/**
 * Interactive session picker for --resume-picker.
 *
 * Runs before React/Ink takes over the terminal, using raw stdin
 * to let the user arrow-key through sessions and press Enter to select.
 */

import { listSessionsForCwd, formatSessionEntry } from './sessions.js';
import type { SessionEntry } from './sessions.js';

/**
 * Show an interactive session picker and return the selected session ID.
 * Returns undefined if no sessions exist or user cancels (Ctrl+C / Escape).
 */
export async function pickSession(cwd: string): Promise<string | undefined> {
  const sessions = listSessionsForCwd(cwd);
  if (sessions.length === 0) {
    process.stderr.write('No saved sessions found for this directory.\n');
    return undefined;
  }

  return new Promise<string | undefined>((resolve) => {
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

    const printMenu = (entries: SessionEntry[], selected: number) => {
      process.stderr.write('Select a chat session to resume:\n');

      const visibleCount = Math.min(entries.length, maxVisible);
      for (let vi = 0; vi < visibleCount; vi++) {
        const i = scrollOffset + vi;
        const prefix = i === selected ? '\x1b[36m❯\x1b[0m ' : '  ';
        const text = formatSessionEntry(entries[i]!);
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

      if (key === '\x03' || key === '\x1b') {
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
