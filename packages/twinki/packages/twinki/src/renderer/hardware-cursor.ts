/**
 * Whether the terminal's own cursor should stay visible while the TUI runs.
 *
 * Multiplexers only forward the cursor position to the outer terminal while the
 * cursor is visible, and IME candidate windows anchor to it — hiding it there
 * breaks composition input. `TWINKI_HARDWARE_CURSOR` overrides the detection in
 * either direction so callers never have to repeat these rules.
 */
export function isHardwareCursorEnabled(): boolean {
  const override = process.env.TWINKI_HARDWARE_CURSOR;
  if (override === '1') return true;
  if (override === '0') return false;
  return 'ZELLIJ' in process.env || 'TMUX' in process.env;
}
