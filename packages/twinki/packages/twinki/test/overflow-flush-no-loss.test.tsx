import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * With preserveScrollbackOnRedraw, a message finalizing into <Static> while
 * the live region overflows the viewport triggers a viewport-tail repaint.
 * Rows visible at flush time must be scrolled into scrollback, not erased in
 * place — erasure destroyed content in both its streamed and finalized forms
 * (the "output missing when the output is large" report on the fix PR).
 *
 * Mirrors the lite-mode reality that finalized (markdown) rows differ from
 * the raw streamed rows: live shows RAW-i, the static flush writes FMT-i.
 */
describe('no content loss on overflow flush', () => {
  it('every streamed row survives in some form after the flush repaint', async () => {
    const COLS = 40;
    const ROWS = 10;
    const MSG_LINES = 30;
    const terminal = new TestTerminal(COLS, ROWS);

    let finalize!: () => void;

    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0', 'HIST-1']);
      const [streaming, setStreaming] = useState(true);
      finalize = () => {
        setHistory((h) => [
          ...h,
          ...Array.from({ length: MSG_LINES }, (_, i) => `FMT-${i}`),
        ]);
        setStreaming(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {streaming ? (
              Array.from({ length: MSG_LINES }, (_, i) => (
                <Text key={i}>{`RAW-${i}`}</Text>
              ))
            ) : (
              <Text>FOOTER</Text>
            )}
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

      finalize();
      await wait(60);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push(buf.getLine(i)?.translateToString(true) ?? '');
      }
      const joined = all.join('\n');

      const missingFmt: number[] = [];
      for (let i = 0; i < MSG_LINES; i++) {
        if (!joined.includes(`FMT-${i}`)) missingFmt.push(i);
      }
      const presentRaw: number[] = [];
      for (let i = 0; i < MSG_LINES; i++) {
        if (joined.includes(`RAW-${i}`)) presentRaw.push(i);
      }

      // Rows above the repaint seam keep their streamed (RAW) form — that
      // staleness is the accepted price of preserving scrollback. What is
      // NEVER acceptable is a row missing in BOTH forms: content the user
      // watched stream must survive somewhere in the buffer.
      const lostEntirely: number[] = [];
      for (let i = 0; i < MSG_LINES; i++) {
        if (!joined.includes(`FMT-${i}`) && !joined.includes(`RAW-${i}`)) {
          lostEntirely.push(i);
        }
      }
      expect(lostEntirely).toEqual([]);

      // The viewport must show the finalized tail and history must survive.
      expect(joined).toContain(`FMT-${MSG_LINES - 1}`);
      expect(joined).toContain('HIST-0');
      expect(terminal.getViewport().join('\n')).toContain('FOOTER');
    } finally {
      inst.unmount();
    }
  });

  it('repeated flushes during sustained overflow do not accumulate duplicate rows', async () => {
    await runGrowthProbe({ shiftPerFlush: 0 });
  });

  it('repeated flushes with a shifting live region do not accumulate duplicate rows', async () => {
    // The live region advances between capture and repaint whenever
    // streaming continues across a flush — the common case. An overlap
    // check anchored at the frame end sees zero match then and preserves
    // a full screenful per flush.
    await runGrowthProbe({ shiftPerFlush: 1 });
  });

  async function runGrowthProbe(opts: { shiftPerFlush: number }) {
    const COLS = 40;
    const ROWS = 10;
    const LIVE_ROWS = 30;
    const FLUSHES = 8;
    const terminal = new TestTerminal(COLS, ROWS);

    let appendHistory!: () => void;

    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0']);
      const [offset, setOffset] = useState(0);
      appendHistory = () => {
        setHistory((h) => [...h, `HIST-${h.length}`]);
        if (opts.shiftPerFlush > 0) {
          setOffset((o) => o + opts.shiftPerFlush);
        }
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {Array.from({ length: LIVE_ROWS }, (_, i) => (
              <Text key={i}>{`LIVE-${offset + i}`}</Text>
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

      for (let f = 0; f < FLUSHES; f++) {
        appendHistory();
        await wait(40);
        await terminal.flush();
      }

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        const line = buf.getLine(i)?.translateToString(true) ?? '';
        if (line.trim()) all.push(line.trim());
      }

      // Every history row must be present exactly once.
      for (let i = 0; i <= FLUSHES; i++) {
        expect(all.filter((l) => l === `HIST-${i}`).length).toBe(1);
      }
      // No live row may repeat more than twice (once as committed overflow,
      // once in the repainted tail), regardless of whether the live region
      // held still or advanced across each flush.
      let maxRepeat = 0;
      let maxRow = '';
      const totalDistinct = LIVE_ROWS + FLUSHES * opts.shiftPerFlush;
      for (let i = 0; i < totalDistinct; i++) {
        const n = all.filter((l) => l === `LIVE-${i}`).length;
        if (n > maxRepeat) {
          maxRepeat = n;
          maxRow = `LIVE-${i}`;
        }
      }
      expect(`${maxRow}:${maxRepeat}`).toBe(`${maxRow}:${Math.min(maxRepeat, 2)}`);
      // Total buffer growth is bounded by content, not by flush count.
      expect(all.length).toBeLessThanOrEqual(
        1 + FLUSHES + 2 * totalDistinct + ROWS
      );
    } finally {
      inst.unmount();
    }
  }

  it('preserves blank separators and repeated row text across the repaint seam', async () => {
    const COLS = 40;
    const ROWS = 10;
    const terminal = new TestTerminal(COLS, ROWS);

    let finalize!: () => void;

    // Finalized content: paragraphs separated by BLANK rows, with the same
    // literal text appearing once early (lands above the repainted tail) and
    // once at the end (inside the tail). A content-equality overlap filter
    // drops the early copy and every blank separator.
    const FLUSHED = [
      'DUP-ROW',
      'FMT-A1',
      'FMT-A2',
      '',
      'FMT-B1',
      'FMT-B2',
      '',
      'FMT-C1',
      'FMT-C2',
      '',
      'FMT-D1',
      'FMT-D2',
      'DUP-ROW',
    ];

    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0']);
      const [streaming, setStreaming] = useState(true);
      finalize = () => {
        // One multi-line item: blank separators must be rows inside a text
        // block (a standalone empty <Text> item renders zero rows).
        setHistory((h) => [...h, FLUSHED.join('\n')]);
        setStreaming(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {streaming ? (
              Array.from({ length: 30 }, (_, i) => (
                <Text key={i}>{`RAW-${i}`}</Text>
              ))
            ) : (
              <Text>FOOTER</Text>
            )}
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
      finalize();
      await wait(60);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trimEnd());
      }

      // Both copies of the repeated row survive.
      expect(all.filter((l) => l === 'DUP-ROW').length).toBe(2);

      // The flushed run survives contiguously WITH its blank separators:
      // locate FMT-A1 and check the exact sequence through FMT-D2.
      const start = all.findIndex((l) => l === 'FMT-A1');
      expect(start).toBeGreaterThan(-1);
      expect(all.slice(start, start + 12)).toEqual(FLUSHED.slice(1));
    } finally {
      inst.unmount();
    }
  });
});
