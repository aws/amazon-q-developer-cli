import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Regression for the "TUI jumps to the top of session history while the agent
 * is streaming" bug (Taskei V2169451316 / V2196259615).
 *
 * Repro shape: finalized history sits in <Static> (terminal scrollback), the
 * live region is TALLER than the viewport (a long in-flight streaming
 * response), and then a chunk finalizes — appending to <Static>. On that
 * append twinki's writeStaticLines overflow branch emits `\x1b[3J` (clear
 * scrollback) + a full redraw, wiping the history the user had scrolled up to
 * read and collapsing the viewport to the top.
 */
describe('scrollback preservation during streaming overflow', () => {
  it('does not clear scrollback when a static flush happens while the live region overflows the viewport', async () => {
    const COLS = 40;
    const ROWS = 10;
    const terminal = new TestTerminal(COLS, ROWS);

    // Capture every raw write so we can detect the scrollback-clear sequence.
    const raw: string[] = [];
    const origWrite = terminal.write.bind(terminal);
    (terminal as unknown as { write: (d: string) => void }).write = (d: string) => {
      raw.push(d);
      return origWrite(d);
    };

    let appendHistory!: () => void;

    // Live region is a constant 30 rows — far taller than the 10-row viewport,
    // exactly the "large amount of text on screen while streaming" condition.
    const LIVE_ROWS = 30;

    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0', 'HIST-1', 'HIST-2']);
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
      preserveScrollbackOnRedraw: true,
    });
    try {
      await wait(40);
      await terminal.flush();

      // Only inspect writes emitted by the mid-stream static flush.
      raw.length = 0;
      appendHistory();
      await wait(50);
      await terminal.flush();

      const clearedScrollback = raw.some((d) => d.includes('\x1b[3J'));
      expect(clearedScrollback).toBe(false);

      // History written before the flush must survive in the terminal buffer
      // (scrollback + viewport), and the viewport must show the frame tail.
      const buf = terminal.xtermBuffer();
      const allLines: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        allLines.push(buf.getLine(i)?.translateToString(true) ?? '');
      }
      const joined = allLines.join('\n');
      expect(joined).toContain('HIST-0');
      expect(joined).toContain('HIST-2');

      const viewport = terminal.getViewport().join('\n');
      expect(viewport).toContain(`LIVE-${LIVE_ROWS - 1}`);

      // Streaming continues: another flush + more history while still
      // overflowing must also leave scrollback alone and keep the viewport
      // tracking the frame tail.
      raw.length = 0;
      appendHistory();
      await wait(50);
      await terminal.flush();

      expect(raw.some((d) => d.includes('\x1b[3J'))).toBe(false);
      expect(terminal.getViewport().join('\n')).toContain(
        `LIVE-${LIVE_ROWS - 1}`
      );
    } finally {
      inst.unmount();
    }
  });
});
