/**
 * Replays a rendered frame in a headless terminal emulator so tests can read
 * the attributes of individual cells.
 *
 * A cursor drawn as a reverse-video cell is invisible if the terminal also
 * parks its own block cursor there — the two inversions cancel. Plain-text
 * screen assertions cannot see that, so tests need cell attributes at an exact
 * position.
 */

import { Terminal } from '@xterm/headless';

export interface CellState {
  char: string;
  /** Whether the app painted the cell reverse-video (SGR 7). */
  inverse: boolean;
  row: number;
  col: number;
}

type Size = { cols: number; rows: number };

async function emulate(frame: string, size: Size): Promise<Terminal> {
  const terminal = new Terminal({ ...size, allowProposedApi: true });
  await new Promise<void>((resolve) => terminal.write(frame, resolve));
  return terminal;
}

function readCell(terminal: Terminal, row: number, col: number): CellState {
  const cell = terminal.buffer.active.getLine(row)?.getCell(col);
  return {
    char: cell?.getChars() || ' ',
    inverse: (cell?.isInverse() ?? 0) !== 0,
    row,
    col,
  };
}

/** The cell the terminal's cursor ended up on after the frame was drawn. */
export async function inspectCursorCell(
  frame: string,
  size: Size = { cols: 80, rows: 24 }
): Promise<CellState> {
  const terminal = await emulate(frame, size);
  try {
    const buffer = terminal.buffer.active;
    return readCell(terminal, buffer.baseY + buffer.cursorY, buffer.cursorX);
  } finally {
    terminal.dispose();
  }
}

/** The first cell holding `char`, wherever it landed on screen. */
export async function inspectCell(
  frame: string,
  char: string,
  size: Size = { cols: 80, rows: 24 }
): Promise<CellState | null> {
  const terminal = await emulate(frame, size);
  try {
    const buffer = terminal.buffer.active;
    for (let row = 0; row < buffer.baseY + size.rows; row++) {
      const col = buffer.getLine(row)?.translateToString().indexOf(char) ?? -1;
      if (col !== -1) return readCell(terminal, row, col);
    }
    return null;
  } finally {
    terminal.dispose();
  }
}
