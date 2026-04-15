/**
 * Tests that Box rendering does not emit trailing whitespace
 * when there is no background color or border.
 *
 * This is critical for clean terminal text selection / clipboard copies.
 * See: https://taskei.amazon.dev/tasks/D417221170
 */
import { describe, it, expect } from 'vitest';
import React from 'react';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import type { Terminal } from '../src/terminal/terminal.js';

// Terminal that captures raw write output for inspection
class RawCaptureTerminal implements Terminal {
  private inputHandler?: (data: string) => void;
  private _cols: number;
  private _rows: number;
  rawWrites: string[] = [];

  constructor(cols = 60, rows = 10) {
    this._cols = cols;
    this._rows = rows;
  }
  get kittyProtocolActive() { return true; }
  get columns() { return this._cols; }
  get rows() { return this._rows; }
  start(onInput: (data: string) => void) { this.inputHandler = onInput; }
  stop() {}
  async drainInput() {}
  write(data: string) { this.rawWrites.push(data); }
  sendInput(data: string) { this.inputHandler?.(data); }
  moveBy(n: number) { if (n > 0) this.write(`\x1b[${n}B`); else if (n < 0) this.write(`\x1b[${-n}A`); }
  hideCursor() { this.write('\x1b[?25l'); }
  showCursor() { this.write('\x1b[?25h'); }
  clearLine() { this.write('\x1b[K'); }
  clearFromCursor() { this.write('\x1b[J'); }
  clearScreen() { this.write('\x1b[2J\x1b[H'); }
  setTitle() {}
  enableMouse() {}
  disableMouse() {}
  async flush(): Promise<void> {}

  /**
   * Extract visible content lines from the raw terminal output.
   * Strips all ANSI escape sequences, then filters to non-empty lines.
   */
  getContentLines(): string[] {
    const all = this.rawWrites.join('');
    // Split on \r\n which separates rendered lines
    const rawLines = all.split('\r\n');
    return rawLines
      .map(l => stripAnsi(l))
      .filter(l => l.trim().length > 0);
  }
}

/** Strip all ANSI escape sequences from a string */
function stripAnsi(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b\[[0-9;]*[A-Za-z]|\x1b\[\?[0-9]+[hl]|\x1b\][^\x07]*\x07|\x1b\(B/g, '');
}

async function wait(ms = 30) { await new Promise(r => setTimeout(r, ms)); }

describe('Trailing whitespace in Box rendering', () => {
  it('plain Box with Text has no trailing spaces', async () => {
    const term = new RawCaptureTerminal(80, 10);
    const inst = render(
      React.createElement(Box, { flexDirection: 'column' },
        React.createElement(Text, null, 'Hello world'),
      ),
      { terminal: term, exitOnCtrlC: false },
    );

    await wait();

    const lines = term.getContentLines();
    expect(lines.some(l => l.includes('Hello world'))).toBe(true);
    for (const line of lines) {
      expect(line).toBe(line.trimEnd());
    }

    inst.unmount();
  });

  it('nested Boxes without background have no trailing spaces', async () => {
    // Mimics the StatusBar layout: outer row with a narrow left column + content column
    const term = new RawCaptureTerminal(80, 10);
    const inst = render(
      React.createElement(Box, { flexDirection: 'row', width: 80 },
        React.createElement(Box, { width: 1 },
          React.createElement(Text, null, '│'),
        ),
        React.createElement(Box, { flexGrow: 1, marginLeft: 1, flexDirection: 'column' },
          React.createElement(Text, null, 'Line one of the response'),
          React.createElement(Text, null, 'Line two of the response'),
        ),
      ),
      { terminal: term, exitOnCtrlC: false },
    );

    await wait();

    const lines = term.getContentLines();
    expect(lines.length).toBeGreaterThanOrEqual(2);
    for (const line of lines) {
      expect(line).toBe(line.trimEnd());
    }

    inst.unmount();
  });

  it('Box with backgroundColor DOES have trailing fill', async () => {
    const term = new RawCaptureTerminal(40, 10);
    const inst = render(
      React.createElement(Box, { backgroundColor: 'blue', width: 40 },
        React.createElement(Text, null, 'Hi'),
      ),
      { terminal: term, exitOnCtrlC: false },
    );

    await wait();

    const lines = term.getContentLines();
    const hiLine = lines.find(l => l.includes('Hi'))!;
    expect(hiLine).toBeDefined();
    // Background fill should pad the line beyond just "Hi"
    expect(hiLine.length).toBeGreaterThan('Hi'.length);

    inst.unmount();
  });

  it('Box with border DOES have trailing fill', async () => {
    const term = new RawCaptureTerminal(40, 10);
    const inst = render(
      React.createElement(Box, { borderStyle: 'single', width: 20 },
        React.createElement(Text, null, 'Hi'),
      ),
      { terminal: term, exitOnCtrlC: false },
    );

    await wait();

    const lines = term.getContentLines();
    const hiLine = lines.find(l => l.includes('Hi'))!;
    expect(hiLine).toBeDefined();
    // Border should cause the line to end with │
    expect(hiLine.trimEnd().endsWith('│')).toBe(true);

    inst.unmount();
  });

  it('multi-line wrapped text in plain Box has no trailing spaces', async () => {
    const term = new RawCaptureTerminal(30, 10);
    const longText = 'The quick brown fox jumps over the lazy dog near the river bank';
    const inst = render(
      React.createElement(Box, { flexDirection: 'column', width: 30 },
        React.createElement(Text, { wrap: 'wrap' }, longText),
      ),
      { terminal: term, exitOnCtrlC: false },
    );

    await wait();

    const lines = term.getContentLines();
    expect(lines.length).toBeGreaterThan(1);
    for (const line of lines) {
      expect(line).toBe(line.trimEnd());
    }

    inst.unmount();
  });
});
