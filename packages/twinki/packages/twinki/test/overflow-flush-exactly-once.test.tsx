import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * With preserveScrollbackOnRedraw, a message whose finalized <Static> rows
 * are byte-identical to its streamed rows (the lite TUI contract: streaming
 * goes through the same renderer, live→static flush is a no-op) must appear
 * EXACTLY ONCE after the overflow-flush repaint. The first shipped fix
 * emitted the flushed lines unconditionally above the repainted tail, which
 * re-emitted everything the user had already watched stream — the entire
 * turn appeared twice in scrollback, with the old footer chrome sandwiched
 * between the copies.
 */
describe('exactly-once on overflow flush (identical streamed/finalized rows)', () => {
  const COLS = 40;
  const ROWS = 10;

  function buildApp(streamRows: number, flushRows: number) {
    let finalize!: () => void;
    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST-0']);
      const [streaming, setStreaming] = useState(true);
      finalize = () => {
        setHistory((h) => [
          ...h,
          Array.from({ length: flushRows }, (_, i) => `MSG-${i}`).join('\n'),
        ]);
        setStreaming(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {streaming ? (
              <>
                {Array.from({ length: streamRows }, (_, i) => (
                  <Text key={i}>{`MSG-${i}`}</Text>
                ))}
                <Text>OLDCHROME</Text>
              </>
            ) : (
              <Text>NEWCHROME</Text>
            )}
          </Box>
        </Box>
      );
    };
    return { App, getFinalize: () => finalize };
  }

  async function collectAfterFlush(streamRows: number, flushRows: number) {
    const terminal = new TestTerminal(COLS, ROWS);
    const { App, getFinalize } = buildApp(streamRows, flushRows);
    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      preserveScrollbackOnRedraw: true,
    });
    try {
      await wait(40);
      await terminal.flush();
      getFinalize()();
      await wait(60);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        const line = (buf.getLine(i)?.translateToString(true) ?? '').trim();
        if (line) all.push(line);
      }
      return { all, viewport: terminal.getViewport().join('\n') };
    } finally {
      inst.unmount();
    }
  }

  it('a fully streamed message survives exactly once, with stale chrome dropped', async () => {
    const { all, viewport } = await collectAfterFlush(30, 30);
    const bad: string[] = [];
    for (let i = 0; i < 30; i++) {
      const n = all.filter((l) => l === `MSG-${i}`).length;
      if (n !== 1) bad.push(`MSG-${i}=${n}`);
    }
    expect(bad).toEqual([]);
    expect(all.filter((l) => l === 'HIST-0').length).toBe(1);
    // The old footer was superseded by the re-rendered live region the tail
    // paints — preserving it would sandwich dead chrome into scrollback.
    expect(all.filter((l) => l === 'OLDCHROME').length).toBe(0);
    expect(viewport).toContain('NEWCHROME');
  });

  it('paint lag: rows flushed but never streamed are emitted exactly once', async () => {
    // The last painted frame showed only 20 of the 30 rows the flush
    // finalizes; the unpainted suffix has no streamed record anywhere and
    // must be emitted, while the painted prefix must not be re-emitted.
    const { all, viewport } = await collectAfterFlush(20, 30);
    const bad: string[] = [];
    for (let i = 0; i < 30; i++) {
      const n = all.filter((l) => l === `MSG-${i}`).length;
      if (n !== 1) bad.push(`MSG-${i}=${n}`);
    }
    expect(bad).toEqual([]);
    expect(all.filter((l) => l === 'OLDCHROME').length).toBe(0);
    expect(viewport).toContain('NEWCHROME');
  });

  it('message ordering survives the seam intact', async () => {
    const { all } = await collectAfterFlush(30, 30);
    const first = all.indexOf('MSG-0');
    expect(first).toBeGreaterThan(-1);
    expect(all.slice(first, first + 30)).toEqual(
      Array.from({ length: 30 }, (_, i) => `MSG-${i}`)
    );
  });
});
