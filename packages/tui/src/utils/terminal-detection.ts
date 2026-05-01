/** Checks TERM_PROGRAM and TERM to detect Ghostty, including over SSH. */
export function isGhostty(): boolean {
  return (
    process.env.TERM_PROGRAM === 'ghostty' ||
    process.env.TERM === 'xterm-ghostty'
  );
}

/** Checks TERM, TERM_PROGRAM, and KITTY_WINDOW_ID to detect Kitty, including over SSH. */
export function isKitty(): boolean {
  return (
    process.env.TERM === 'xterm-kitty' ||
    process.env.TERM_PROGRAM === 'kitty' ||
    'KITTY_WINDOW_ID' in process.env
  );
}
