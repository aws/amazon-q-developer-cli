/**
 * Centralized alt-screen tracking.
 *
 * Every call site that enters the terminal's alternate screen buffer MUST call
 * enterAltScreen() instead of writing the SMCUP sequence directly, and call
 * leaveAltScreen() when returning to the main screen. This tracks whether the
 * process is CURRENTLY in alt-screen so resetTerminal() only issues RMCUP when
 * necessary. Without this guard, terminals on the main screen receive a
 * spurious RMCUP that triggers a cursor-restore to a garbage position, leaking
 * characters into the shell.
 */

import { RESET_SGR } from './terminal-sequences.js';

const SMCUP = '\x1b[?1049h';
const RMCUP = '\x1b[?1049l';

let active = false;

/** Whether the process is currently in the alternate screen buffer. */
export function isAltScreenActive(): boolean {
  return active;
}

/** Enter the alternate screen buffer. */
export function enterAltScreen(): void {
  active = true;
  process.stdout.write(SMCUP);
}

/** Leave the alternate screen buffer. */
export function leaveAltScreen(): void {
  active = false;
  process.stdout.write(RMCUP);
  // RMCUP restores the rendition saved at SMCUP, so clear SGR after it — both
  // on exit (so nothing leaks to the parent shell) and mid-session (so a stale
  // attribute doesn't bleed back into the TUI when a surface closes).
  process.stdout.write(RESET_SGR);
}

/**
 * Sync the flag after an external exit (e.g. twinki's useFullscreen unmount).
 * Call this when the renderer exits alt-screen through its own path.
 */
export function markAltScreenExited(): void {
  active = false;
}
