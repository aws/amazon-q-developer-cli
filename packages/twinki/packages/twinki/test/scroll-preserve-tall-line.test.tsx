import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Viewport-tail repaint must never write more than a viewport's worth of
 * physical rows. With `wideLines`, one logical line can be taller than the
 * whole viewport; emitting it from the top pushes its leading rows into
 * scrollback, where the next redraw appends them again — duplicating the
 * fragment and destroying the history it displaced.
 */
describe('viewport-tail repaint bounds', () => {
  const COLS = 40;
  const ROWS = 10;
  const MARKER = 'TALLSTART';
  /** 11 physical rows at 40 cols — taller than the 10-row viewport. */
  const TALL = MARKER + 'x'.repeat(COLS * 11 - MARKER.length);

  const fullBuffer = (terminal: TestTerminal): string[] => {
    const buf = terminal.xtermBuffer();
    const out: string[] = [];
    for (let i = 0; i < buf.length; i++) {
      const line = buf.getLine(i);
      out.push(line ? line.translateToString(true) : '');
    }
    return out;
  };

  const mountAndFlush = async (liveLine: string, appends: number) => {
    const terminal = new TestTerminal(COLS, ROWS);
    let appendHistory!: () => void;
    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0']);
      appendHistory = () => setHistory((h) => [...h, `HIST-${h.length}`]);
      return (
        <Box flexDirection="column">
          <Static items={history}>
            {(h) => <Text wrap="overflow">{h}</Text>}
          </Static>
          <Box flexDirection="column">
            <Text wrap="overflow">{liveLine}</Text>
          </Box>
        </Box>
      );
    };
    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      wideLines: true,
      preserveScrollbackOnRedraw: true,
    });
    try {
      await wait(40);
      await terminal.flush();
      for (let i = 0; i < appends; i++) {
        appendHistory();
        await wait(50);
        await terminal.flush();
      }
      return fullBuffer(terminal);
    } finally {
      inst.unmount();
    }
  };

  it('does not duplicate a logical line taller than the viewport', async () => {
    const rows = await mountAndFlush(TALL, 5);
    const copies = rows.filter((r) => r.includes(MARKER)).length;
    expect(copies).toBe(1);
  });

  it('keeps the static history the duplicates would have displaced', async () => {
    const rows = await mountAndFlush(TALL, 5);
    const hist = rows.filter((r) => /HIST-\d/.test(r)).map((r) => r.trim());
    expect(hist).toEqual([
      'HIST-0',
      'HIST-1',
      'HIST-2',
      'HIST-3',
      'HIST-4',
      'HIST-5',
    ]);
  });
});
