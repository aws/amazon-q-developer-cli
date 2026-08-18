import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import React from 'react';
import { render, Text, useSelectionCopy } from '../../renderer.js';
import type { Instance, Terminal } from 'twinki';
import {
  connectMouseCapture,
  isMouseCaptureEnabled,
  setMouseCaptureEnabled,
} from '../mouse-capture.js';

class SelectionTerminal implements Terminal {
  public output = '';
  private onInput: ((data: string) => void) | null = null;

  constructor(
    private readonly width = 30,
    private readonly height = 5
  ) {}

  get columns() {
    return this.width;
  }
  get rows() {
    return this.height;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void): void {
    this.onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
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
  sendInput(data: string): void {
    this.onInput?.(data);
  }
}

const mouseDown = (x: number, y: number) => `\x1b[<0;${x + 1};${y + 1}M`;
const mouseMove = (x: number, y: number) => `\x1b[<32;${x + 1};${y + 1}M`;
const mouseUp = (x: number, y: number) => `\x1b[<0;${x + 1};${y + 1}m`;

const OSC52_PATTERN = new RegExp(
  `${String.fromCharCode(27)}\\]52;c;([A-Za-z0-9+/=]*)${String.fromCharCode(7)}`,
  'g'
);

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

async function drag(
  terminal: SelectionTerminal,
  from: { x: number; y: number },
  to: { x: number; y: number }
): Promise<void> {
  terminal.sendInput(mouseDown(from.x, from.y));
  terminal.sendInput(mouseMove(to.x, to.y));
  terminal.sendInput(mouseUp(to.x, to.y));
  await flush();
}

let activeInstance: Instance | null = null;
let disconnect: (() => void) | null = null;

beforeEach(() => {
  setMouseCaptureEnabled(false);
});

afterEach(() => {
  disconnect?.();
  disconnect = null;
  activeInstance?.unmount();
  activeInstance = null;
  setMouseCaptureEnabled(false);
});

/** Mounts with the renderer options and mouse-capture bridge the app uses. */
async function mountApp(): Promise<{
  terminal: SelectionTerminal;
  copied: string[];
}> {
  const terminal = new SelectionTerminal();
  const copied: string[] = [];

  function App(): React.ReactElement {
    useSelectionCopy((text) => copied.push(text));
    return <Text>hook callback</Text>;
  }

  activeInstance = render(<App />, {
    terminal,
    exitOnCtrlC: false,
    mouse: true,
    textSelection: true,
  });
  disconnect = connectMouseCapture(activeInstance);
  await flush();
  return { terminal, copied };
}

describe('renderer selection wiring', () => {
  it('leaves terminal mouse reporting off after boot', async () => {
    await mountApp();
    // Selection support must not quietly re-enable reporting: that would take
    // the wheel and native selection away from the terminal in ordinary chat.
    expect(activeInstance!.isMouseEnabled()).toBe(false);
    expect(isMouseCaptureEnabled()).toBe(false);
  });

  it('does not copy while mouse capture is off', async () => {
    const { terminal, copied } = await mountApp();
    await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
    expect(copied).toEqual([]);
  });

  it('copies the dragged text once mouse capture is enabled', async () => {
    const { terminal, copied } = await mountApp();
    setMouseCaptureEnabled(true);
    await flush();

    await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
    expect(copied).toEqual(['hook']);
  });

  it('writes OSC 52 itself, so listeners must not repeat it', async () => {
    const { terminal, copied } = await mountApp();
    setMouseCaptureEnabled(true);
    await flush();

    await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
    const osc52 = [...terminal.output.matchAll(OSC52_PATTERN)];
    expect(osc52).toHaveLength(1);
    expect(Buffer.from(osc52[0]![1]!, 'base64').toString('utf8')).toBe('hook');
    expect(copied).toEqual(['hook']);
  });

  it('stops copying after capture is turned back off', async () => {
    const { terminal, copied } = await mountApp();
    setMouseCaptureEnabled(true);
    await flush();
    await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
    expect(copied).toEqual(['hook']);

    setMouseCaptureEnabled(false);
    await flush();
    await drag(terminal, { x: 0, y: 0 }, { x: 3, y: 0 });
    expect(copied).toEqual(['hook']);
  });
});
