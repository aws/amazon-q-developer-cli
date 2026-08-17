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
}

/**
 * Sync the flag after an external exit (e.g. twinki's useFullscreen unmount).
 * Call this when the renderer exits alt-screen through its own path.
 */
export function markAltScreenExited(): void {
  active = false;
}
