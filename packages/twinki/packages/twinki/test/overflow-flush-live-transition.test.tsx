import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Replay-shaped overflow flush: content that was rendered in the live region
 * (a replayed turn withheld from <Static>) later flushes to static in the
 * same commit that adds a new user row, while the live region overflows the
 * viewport. Every content row must end up in the buffer exactly once — the
 * field failure painted the flushed turn AND kept its live-rendered copy,
 * sandwiching the new input between two copies of the old response.
 */
describe('overflow flush of previously-live content', () => {
  const ROWS = 10;
  const BODY_LINES = 18;

  function bodyLine(i: number): string {
    return `HISTBODY-${i} analysis paragraph content`;
  }

  it('live-to-static transition emits each row once', async () => {
    const terminal = new TestTerminal(60, ROWS);

    let finalize!: () => void;

    const App = () => {
      // Phase 1 (replay-shaped): history prefix in Static; the last turn's
      // body rendered ONLY in the live region (withheld).
      const [history, setHistory] = useState<string[]>([
        'HIST-A earlier turn',
        'HIST-B earlier turn',
      ]);
      const [withheld, setWithheld] = useState(true);
      finalize = () => {
        // Phase 2: the withheld body flushes to static together with the
        // new user row (one commit), live shrinks to the footer.
        setHistory((h) => [
          ...h,
          ...Array.from({ length: BODY_LINES }, (_, i) => bodyLine(i)),
          'You: BRANDNEWINPUT sup',
        ]);
        setWithheld(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {withheld ? (
              Array.from({ length: BODY_LINES }, (_, i) => (
                <Text key={i}>{bodyLine(i)}</Text>
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
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trim());
      }

      const counts = new Map<string, number>();
      for (let i = 0; i < BODY_LINES; i++) {
        counts.set(bodyLine(i), all.filter((l) => l === bodyLine(i)).length);
      }
      const supCount = all.filter((l) => l.includes('BRANDNEWINPUT')).length;

      for (const [key, n] of counts) {
        expect(`${key.slice(0, 12)}:${n}`).toBe(`${key.slice(0, 12)}:1`);
      }
      expect(supCount).toBe(1);
      // Content order: the body precedes the new user row.
      expect(all.indexOf(bodyLine(BODY_LINES - 1))).toBeLessThan(
        all.findIndex((l) => l.includes('BRANDNEWINPUT'))
      );
    } finally {
      inst.unmount();
    }
  });

  it('flush landing one commit before the live shrink emits each row once', async () => {
    const terminal = new TestTerminal(60, ROWS);

    let flushBody!: () => void;
    let shrinkLive!: () => void;

    const App = () => {
      const [history, setHistory] = useState<string[]>([
        'HIST-A earlier turn',
        'HIST-B earlier turn',
      ]);
      const [withheld, setWithheld] = useState(true);
      // Two separate commits: the body flushes to <Static> while the live
      // region STILL renders it (the transient frame carries the content
      // twice); the live region shrinks one commit later.
      flushBody = () =>
        setHistory((h) => [
          ...h,
          ...Array.from({ length: BODY_LINES }, (_, i) => bodyLine(i)),
        ]);
      shrinkLive = () => {
        setHistory((h) => [...h, 'You: BRANDNEWINPUT sup']);
        setWithheld(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {withheld ? (
              Array.from({ length: BODY_LINES }, (_, i) => (
                <Text key={i}>{bodyLine(i)}</Text>
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

      flushBody();
      await wait(60);
      await terminal.flush();
      shrinkLive();
      await wait(60);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trim());
      }

      const counts = new Map<string, number>();
      for (let i = 0; i < BODY_LINES; i++) {
        counts.set(bodyLine(i), all.filter((l) => l === bodyLine(i)).length);
      }
      const supCount = all.filter((l) => l.includes('BRANDNEWINPUT')).length;

      for (const [key, n] of counts) {
        expect(`${key.slice(0, 12)}:${n}`).toBe(`${key.slice(0, 12)}:1`);
      }
      expect(supCount).toBe(1);
      expect(all.indexOf(bodyLine(BODY_LINES - 1))).toBeLessThan(
        all.findIndex((l) => l.includes('BRANDNEWINPUT'))
      );
    } finally {
      inst.unmount();
    }
  });

  it('flush carrying a trailing row past the still-live body keeps order', async () => {
    const terminal = new TestTerminal(60, ROWS);

    let flushBodyAndRow!: () => void;
    let shrinkLive!: () => void;

    const App = () => {
      const [history, setHistory] = useState<string[]>([
        'HIST-A earlier turn',
        'HIST-B earlier turn',
      ]);
      const [withheld, setWithheld] = useState(true);
      // One commit flushes the body PLUS a trailing row (the post-run
      // batch); the live region still renders the body and shrinks a
      // commit later.
      flushBodyAndRow = () =>
        setHistory((h) => [
          ...h,
          ...Array.from({ length: BODY_LINES }, (_, i) => bodyLine(i)),
          'You: BRANDNEWINPUT sup',
        ]);
      shrinkLive = () => setWithheld(false);
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {withheld ? (
              Array.from({ length: BODY_LINES }, (_, i) => (
                <Text key={i}>{bodyLine(i)}</Text>
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
      flushBodyAndRow();
      await wait(60);
      await terminal.flush();
      shrinkLive();
      await wait(60);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trim());
      }

      // Order is the invariant here: the new row must never paint above
      // body content that logically precedes it. Duplication stays the
      // accepted, BOUNDED cost in this adversarial shape (the shrink lands
      // a commit later, so the moved tail can repaint an emitted row) —
      // each row at most twice, nothing lost.
      const supIdx = all.findIndex((l) => l.includes('BRANDNEWINPUT'));
      expect(supIdx).toBeGreaterThan(-1);
      for (let i = 0; i < BODY_LINES; i++) {
        const first = all.indexOf(bodyLine(i));
        expect(first).toBeGreaterThan(-1);
        expect(first).toBeLessThan(supIdx);
        expect(all.filter((l) => l === bodyLine(i)).length).toBeLessThanOrEqual(
          2
        );
      }
      expect(
        all.filter((l) => l.includes('BRANDNEWINPUT')).length
      ).toBeLessThanOrEqual(2);
    } finally {
      inst.unmount();
    }
  });

  it('keeps a finalized row when a distinct new live row has identical text', async () => {
    const terminal = new TestTerminal(60, ROWS);
    const body = Array.from({ length: BODY_LINES }, (_, i) => `BODY-${i}`);
    let finalize!: () => void;

    const App = () => {
      const [history, setHistory] = useState<string[]>(['HIST']);
      const [streaming, setStreaming] = useState(true);
      finalize = () => {
        setHistory((h) => [...h, ...body]);
        setStreaming(false);
      };
      return (
        <Box flexDirection="column">
          <Static items={history}>{(line) => <Text>{line}</Text>}</Static>
          <Box flexDirection="column">
            {streaming ? (
              <>
                {body.map((line) => (
                  <Text key={line}>{line}</Text>
                ))}
                <Text>OLD-FOOTER</Text>
              </>
            ) : (
              <>
                <Text>BODY-9</Text>
                <Text>NEW-FOOTER</Text>
              </>
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
      const all = Array.from({ length: buf.length }, (_, i) =>
        (buf.getLine(i)?.translateToString(true) ?? '').trim()
      );
      for (const row of body) {
        expect(all.filter((line) => line === row)).toHaveLength(
          row === 'BODY-9' ? 2 : 1
        );
      }
    } finally {
      inst.unmount();
    }
  });
});
