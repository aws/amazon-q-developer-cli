import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Viewport-tail repaint leaves rows above the viewport as committed history,
 * so an element spanning those rows (full-height gutter, border, status bar)
 * keeps its stale partial state and renders with gaps. It must therefore stay
 * opt-in: the default has to keep repainting the past, even at the cost of
 * scrollback.
 */
describe('scrollback preservation is opt-in', () => {
  const COLS = 40;
  const ROWS = 10;
  const LIVE_ROWS = 30;

  const mount = (terminal: TestTerminal, preserve: boolean) => {
    let appendHistory!: () => void;
    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0']);
      appendHistory = () => setHistory((h) => [...h, `HIST-${h.length}`]);
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {Array.from({ length: LIVE_ROWS }, (_, i) => (
              <Text key={i}>{`LIVE-${i}`}</Text>
            ))}
          </Box>
        </Box>
      );
    };
    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      preserveScrollbackOnRedraw: preserve,
    });
    return { inst, appendHistory: () => appendHistory() };
  };

  const clearedScrollbackOnFlush = async (preserve: boolean) => {
    const terminal = new TestTerminal(COLS, ROWS);
    const raw: string[] = [];
    const origWrite = terminal.write.bind(terminal);
    (terminal as unknown as { write: (d: string) => void }).write = (
      d: string
    ) => {
      raw.push(d);
      return origWrite(d);
    };

    const { inst, appendHistory } = mount(terminal, preserve);
    try {
      await wait(40);
      await terminal.flush();
      raw.length = 0;
      appendHistory();
      await wait(50);
      await terminal.flush();
      return raw.some((d) => d.includes('\x1b[3J'));
    } finally {
      inst.unmount();
    }
  };

  it('clears scrollback by default so rows above the viewport are repainted', async () => {
    expect(await clearedScrollbackOnFlush(false)).toBe(true);
  });

  it('preserves scrollback when explicitly enabled', async () => {
    expect(await clearedScrollbackOnFlush(true)).toBe(false);
  });
});
