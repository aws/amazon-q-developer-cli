/**
 * Terminal focus tracking via DECSET mode 1004.
 *
 * Enables focus event reporting so the terminal sends:
 *   \x1b[I  — focus gained
 *   \x1b[O  — focus lost
 *
 * Listens on process.stdin (prepended so it runs before Ink's handler)
 * and writes enable/disable sequences to /dev/tty.
 */

import { openSync, writeSync, closeSync } from 'node:fs';

let enabled = false;
let focused = true; // assume focused until told otherwise

const ENABLE_FOCUS = '\x1b[?1004h';
const DISABLE_FOCUS = '\x1b[?1004l';
const FOCUS_IN = '\x1b[I';
const FOCUS_OUT = '\x1b[O';

export function isTerminalFocused(): boolean {
  return focused;
}

function onStdinData(chunk: Buffer): void {
  const str = chunk.toString();
  if (str.includes(FOCUS_IN)) focused = true;
  if (str.includes(FOCUS_OUT)) focused = false;
}

export function enableFocusTracking(): void {
  if (process.platform === 'win32' || enabled) return;

  try {
    const fd = openSync('/dev/tty', 'w');
    try {
      writeSync(fd, ENABLE_FOCUS);
    } finally {
      closeSync(fd);
    }
  } catch {
    return; // /dev/tty not available
  }

  process.stdin.prependListener('data', onStdinData);
  enabled = true;
}

export function disableFocusTracking(): void {
  if (!enabled) return;
  enabled = false;

  process.stdin.removeListener('data', onStdinData);

  try {
    const fd = openSync('/dev/tty', 'w');
    try {
      writeSync(fd, DISABLE_FOCUS);
    } finally {
      closeSync(fd);
    }
  } catch {
    // ignore
  }
}
