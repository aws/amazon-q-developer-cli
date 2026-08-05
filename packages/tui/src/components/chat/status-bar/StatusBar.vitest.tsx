/**
 * The left status bar must disappear on EVERY path for surfaces that drop it.
 * `Message` opted out for itself, but `SystemMessage` and `ShellOutputMessage`
 * rendered `StatusBar` unconditionally. Rendered through twinki so the
 * assertion is on emitted cells rather than on a React tree shape.
 */
import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const { dropsBarMock } = vi.hoisted(() => ({ dropsBarMock: vi.fn() }));

vi.mock('../../../hooks/useDropsLeftStatusBar.js', () => ({
  useDropsLeftStatusBar: dropsBarMock,
}));

import { StatusBar } from './StatusBar.js';

class CapturingTerminal implements Terminal {
  public out = '';
  get columns() {
    return 40;
  }
  get rows() {
    return 10;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(): void {}
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.out += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let active: Instance | null = null;

afterEach(() => {
  active?.unmount();
  active = null;
  dropsBarMock.mockReset();
});

const renderBar = async (dropsBar: boolean) => {
  dropsBarMock.mockReturnValue(dropsBar);
  const terminal = new CapturingTerminal();
  active = render(
    <StatusBar status="success">
      <>BODY</>
    </StatusBar>,
    { terminal, exitOnCtrlC: false }
  );
  await sleep(60);
  return terminal.out;
};

/** Status icon on the first bar line, background-colored cell on the rest. */
const ESC = String.fromCharCode(27);
const GUTTER = new RegExp(`●|${ESC}\\[(4[0-7]|48;)`);

describe('StatusBar suppression on bar-free surfaces', () => {
  test('draws the gutter when the surface keeps the left status bar', async () => {
    const out = await renderBar(false);
    expect(out).toContain('BODY');
    expect(out).toMatch(GUTTER);
  });

  test('emits no gutter when the surface drops the left status bar', async () => {
    const out = await renderBar(true);
    expect(out).toContain('BODY');
    expect(out).not.toMatch(GUTTER);
  });
});
