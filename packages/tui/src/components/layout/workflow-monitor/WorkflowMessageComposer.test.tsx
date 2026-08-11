/**
 * Composer cursor vs. the terminal's own (hardware) cursor.
 *
 * The composer previously emitted only the cursor marker: under a multiplexer
 * the hardware cursor made that visible, but in a plain terminal — where the
 * hardware cursor is hidden — the composer had no visible cursor at all. It
 * now paints a software inverse block unconditionally, and the frame drops
 * that inversion whenever the terminal draws its own cursor on the cell.
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
import { render, type Instance, type Terminal } from 'twinki';
import { inspectCursorCell } from '../../../test-utils/cursor-cell.js';
import { WorkflowMessageComposer } from './WorkflowMessageComposer.js';

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

async function paintComposer() {
  const terminal = new MockTerminal();
  activeInstance = render(
    <WorkflowMessageComposer
      mode="steer"
      targetLabel="coder"
      value="hello"
      width={40}
    />,
    { terminal, exitOnCtrlC: false }
  );
  await new Promise((resolve) => setTimeout(resolve, 30));
  return {
    cell: await inspectCursorCell(terminal.output),
    cursorVisible: terminal.cursorVisible,
  };
}

describe('WorkflowMessageComposer cursor', () => {
  it('paints an inverse cursor cell when the terminal draws no cursor', async () => {
    const { cell, cursorVisible } = await paintComposer();
    expect(cell.inverse).toBe(true);
    expect(cursorVisible).toBe(false);
  });

  it('yields the cursor cell to the terminal under a multiplexer', async () => {
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    const { cell, cursorVisible } = await paintComposer();
    expect(cell.inverse).toBe(false);
    expect(cursorVisible).toBe(true);
  });
});
