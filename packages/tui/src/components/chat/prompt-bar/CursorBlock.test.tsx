/**
 * Prompt cursor rendering vs. the terminal's own (hardware) cursor.
 *
 * Under a multiplexer twinki leaves the hardware cursor visible so IME
 * composition anchors to the right cell. A block hardware cursor inverts the
 * cell it sits on, so the frame must not carry a reverse-video attribute on
 * that same cell — the two inversions cancel out and the cursor becomes
 * invisible. The renderer strips the inversion from the marker cell; these
 * cases pin the resulting frame bytes across the cursor-visibility matrix.
 *
 * Each case renders the cursor the way the prompt does (inline, inside a
 * wrapping Text) and inspects the cell the cursor actually landed on.
 */

import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
} from 'bun:test';
import React from 'react';
import {
  Box,
  render,
  useTwinkiContext,
  type Instance,
  type Terminal,
} from 'twinki';
import {
  inspectCell,
  inspectCursorCell,
} from '../../../test-utils/cursor-cell.js';
import { Text } from '../../ui/text/Text.js';
import { CursorBlock } from './PromptInput.js';

class MockTerminal implements Terminal {
  public output = '';
  public cursorVisible = false;

  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }

  start(): void {}
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {
    this.cursorVisible = false;
  }
  showCursor(): void {
    this.cursorVisible = true;
  }
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
}

let activeInstance: Instance | null = null;

/** Cleared before every case: the test runner itself may be inside a multiplexer. */
const CURSOR_ENV = ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR'] as const;
const ambient = CURSOR_ENV.map((key) => [key, process.env[key]] as const);

beforeEach(() => {
  for (const key of CURSOR_ENV) delete process.env[key];
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

afterAll(() => {
  for (const [key, value] of ambient) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

/**
 * Renders `ab` followed by the cursor on `c`, mirroring how the prompt places
 * the cursor mid-text, and reports the cursor cell plus terminal cursor state.
 */
async function paintCursor(props: { markerOwnedElsewhere?: boolean } = {}) {
  const terminal = new MockTerminal();
  activeInstance = render(
    <Text wrap="wrap">
      <Text>ab</Text>
      <CursorBlock char="c" {...props} />
    </Text>,
    { terminal, exitOnCtrlC: false }
  );
  await new Promise((resolve) => setTimeout(resolve, 30));
  return {
    cell: await inspectCursorCell(terminal.output),
    cursorCharCell: await inspectCell(terminal.output, 'c'),
    cursorVisible: terminal.cursorVisible,
  };
}

describe('CursorBlock', () => {
  it('inverts the cursor cell when the terminal draws no cursor', async () => {
    const { cell, cursorVisible } = await paintCursor();
    expect(cell.char).toBe('c');
    expect(cell.inverse).toBe(true);
    expect(cursorVisible).toBe(false);
  });

  it('leaves the cursor cell alone under a multiplexer', async () => {
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    const { cell, cursorVisible } = await paintCursor();
    expect(cell.char).toBe('c');
    expect(cell.inverse).toBe(false);
    expect(cursorVisible).toBe(true);
  });

  it('inverts the cell under a multiplexer when the marker is suppressed', async () => {
    // No marker means the hardware cursor is parked elsewhere (an open menu
    // owns it), so this cell needs the software cursor to stay visible.
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    const { cursorCharCell, cursorVisible } = await paintCursor({
      markerOwnedElsewhere: true,
    });
    expect(cursorCharCell?.inverse).toBe(true);
    expect(cursorVisible).toBe(false);
  });

  it('honors TWINKI_HARDWARE_CURSOR=0 as an opt-out', async () => {
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    process.env.TWINKI_HARDWARE_CURSOR = '0';
    const { cell, cursorVisible } = await paintCursor();
    expect(cell.inverse).toBe(true);
    expect(cursorVisible).toBe(false);
  });

  it('honors TWINKI_HARDWARE_CURSOR=1 outside a multiplexer', async () => {
    process.env.TWINKI_HARDWARE_CURSOR = '1';
    const { cell, cursorVisible } = await paintCursor();
    expect(cell.char).toBe('c');
    expect(cell.inverse).toBe(false);
    expect(cursorVisible).toBe(true);
  });
});

/**
 * The environment is only where the renderer starts: a caller can override the
 * hardware cursor at construction or change it later. The frame has to follow
 * the resolved state, otherwise the cell stays un-inverted after the renderer
 * turned the hardware cursor off, or double-inverts after it turned it on.
 */
describe("CursorBlock vs. the renderer's resolved state", () => {
  let captured: { setShowHardwareCursor(enabled: boolean): void } | null = null;

  const CaptureRenderer = () => {
    captured = useTwinkiContext().tui;
    return null;
  };

  /** Paints the cursor, then forces the renderer's cursor state the other way. */
  async function repaintWithRendererState(forced: boolean) {
    const terminal = new MockTerminal();
    activeInstance = render(
      <Box flexDirection="column">
        <CaptureRenderer />
        <Text wrap="wrap">
          <Text>ab</Text>
          <CursorBlock char="c" />
        </Text>
      </Box>,
      { terminal, exitOnCtrlC: false }
    );
    await new Promise((resolve) => setTimeout(resolve, 30));
    captured?.setShowHardwareCursor(forced);
    await new Promise((resolve) => setTimeout(resolve, 30));
    return inspectCell(terminal.output, 'c');
  }

  afterEach(() => {
    captured = null;
  });

  it('inverts the cell when the renderer turns the hardware cursor off', async () => {
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    const cell = await repaintWithRendererState(false);
    expect(cell?.inverse).toBe(true);
  });

  it('leaves the cell alone when the renderer turns the hardware cursor on', async () => {
    const cell = await repaintWithRendererState(true);
    expect(cell?.inverse).toBe(false);
  });
});
