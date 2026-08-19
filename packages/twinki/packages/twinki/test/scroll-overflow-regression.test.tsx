import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait, analyzeFlicker } from './helpers.js';

/**
 * Regression test matrix for the overflow-flush scroll behavior.
 *
 * The `pendingFlush` mechanism preserves streamed content during viewport-tail
 * repaints by scrolling captured rows into scrollback. This is correct for
 * BSU-capable terminals (the entire write is atomic), but on non-BSU terminals
 * (tmux <3.7, screen, older xterm) it causes visible progressive drawing that
 * users perceive as "scrolling from top to bottom."
 *
 * Matrix dimensions:
 *   - preserveScrollback: true | false
 *   - BSU support: yes (default) | no (TWINKI_NO_SYNC=1 simulation)
 *   - Conversation length: short (no overflow) | long (overflow)
 *   - Operation: turn commit | repeated commit | small update
 *
 * Key metric: "progressive lines" = total \r\n sequences written OUTSIDE a BSU
 * pair. On a non-BSU terminal, ALL lines are progressive. This is what the user
 * sees as scroll.
 */

// ─── Helpers ──────────────────────────────────────────────────────────────────

interface WriteEvent {
  data: string;
  /** Number of \r\n in this write */
  newlines: number;
  /** Whether this write starts a BSU pair */
  hasBSUStart: boolean;
  /** Whether this write ends a BSU pair */
  hasBSUEnd: boolean;
  /** Contains \x1b[3J (scrollback destruction) */
  has3J: boolean;
  /** Contains \x1b[2K (line erase) */
  has2K: boolean;
  /** Contains \x1b[2J (screen clear) */
  has2J: boolean;
}

interface FlushMetrics {
  /** Total writes captured */
  totalWrites: number;
  /** Total \r\n across ALL writes */
  totalNewlines: number;
  /** \r\n inside BSU pairs (invisible on BSU terminals, progressive on non-BSU) */
  bsuNewlines: number;
  /** \r\n outside BSU pairs (always progressive/visible) */
  unbufferedNewlines: number;
  /** Whether \x1b[3J was emitted (scrollback destroyed) */
  scrollbackDestroyed: boolean;
  /** Number of writes that contain row erases (\x1b[2K) */
  eraseWrites: number;
  /** Whether HISTORY-0 appears in any BSU write (full-frame re-emit) */
  fullFrameInBSU: boolean;
  /** Whether STREAM content appears in repaint (preserved rows) */
  preservedContentInRepaint: boolean;
  /** Max newlines in a single write (proxy for worst-case progressive draw) */
  maxSingleWriteNewlines: number;
}

function captureAndMeasure(
  terminal: TestTerminal,
  terminalSupportsBsu = true
): {
  captures: WriteEvent[];
  getMetrics: () => FlushMetrics;
} {
  const captures: WriteEvent[] = [];
  const origWrite = terminal.write.bind(terminal);
  (terminal as unknown as { write: (d: string) => void }).write = (
    d: string
  ) => {
    captures.push({
      data: d,
      newlines: (d.match(/\r\n/g) || []).length,
      hasBSUStart: d.includes('\x1b[?2026h'),
      hasBSUEnd: d.includes('\x1b[?2026l'),
      has3J: d.includes('\x1b[3J'),
      has2K: d.includes('\x1b[2K'),
      has2J: d.includes('\x1b[2J'),
    });
    return origWrite(d);
  };

  const getMetrics = (): FlushMetrics => {
    let totalNewlines = 0;
    let bsuNewlines = 0;
    let unbufferedNewlines = 0;
    let scrollbackDestroyed = false;
    let eraseWrites = 0;
    let fullFrameInBSU = false;
    let preservedContentInRepaint = false;
    let maxSingleWriteNewlines = 0;

    for (const c of captures) {
      totalNewlines += c.newlines;
      if (c.newlines > maxSingleWriteNewlines) {
        maxSingleWriteNewlines = c.newlines;
      }
      if (terminalSupportsBsu && (c.hasBSUStart || c.hasBSUEnd)) {
        bsuNewlines += c.newlines;
        if (c.data.includes('HISTORY-0')) fullFrameInBSU = true;
        if (c.data.includes('STREAM-')) preservedContentInRepaint = true;
      } else {
        unbufferedNewlines += c.newlines;
      }
      if (c.has3J) scrollbackDestroyed = true;
      if (c.has2K) eraseWrites++;
    }

    return {
      totalWrites: captures.length,
      totalNewlines,
      bsuNewlines,
      unbufferedNewlines,
      scrollbackDestroyed,
      eraseWrites,
      fullFrameInBSU,
      preservedContentInRepaint,
      maxSingleWriteNewlines,
    };
  };

  return { captures, getMetrics };
}

// ─── App Factories ────────────────────────────────────────────────────────────

const COLS = 60;
const ROWS = 15;
const SHORT_MSG_LINES = 5; // fits in viewport
const LONG_MSG_LINES = 30; // overflows viewport

function createLongConversationApp() {
  let commitTurn!: () => void;
  let sendSmallUpdate!: (text: string) => void;

  const App = () => {
    const [history, setHistory] = useState<string[]>(
      Array.from({ length: 5 }, (_, i) => `HISTORY-${i}`)
    );
    const [streaming, setStreaming] = useState(true);
    const [footer, setFooter] = useState('> typing...');

    commitTurn = () => {
      setHistory((h) => [
        ...h,
        ...Array.from({ length: LONG_MSG_LINES }, (_, i) => `MSG-${i}`),
      ]);
      setStreaming(false);
    };
    sendSmallUpdate = (text: string) => setFooter(text);

    return (
      <Box flexDirection="column">
        <Static items={history}>{(h) => <Text key={h}>{h}</Text>}</Static>
        <Box flexDirection="column">
          {streaming ? (
            Array.from({ length: LONG_MSG_LINES }, (_, i) => (
              <Text key={i}>{`STREAM-${i}`}</Text>
            ))
          ) : (
            <Text>{footer}</Text>
          )}
        </Box>
      </Box>
    );
  };

  return {
    App,
    commitTurn: () => commitTurn(),
    sendSmallUpdate: (t: string) => sendSmallUpdate(t),
  };
}

function createShortConversationApp() {
  let commitTurn!: () => void;

  const App = () => {
    const [history, setHistory] = useState<string[]>(
      Array.from({ length: 3 }, (_, i) => `HISTORY-${i}`)
    );
    const [streaming, setStreaming] = useState(true);

    commitTurn = () => {
      setHistory((h) => [
        ...h,
        ...Array.from({ length: SHORT_MSG_LINES }, (_, i) => `MSG-${i}`),
      ]);
      setStreaming(false);
    };

    return (
      <Box flexDirection="column">
        <Static items={history}>{(h) => <Text key={h}>{h}</Text>}</Static>
        <Box flexDirection="column">
          {streaming ? (
            Array.from({ length: SHORT_MSG_LINES }, (_, i) => (
              <Text key={i}>{`STREAM-${i}`}</Text>
            ))
          ) : (
            <Text>{'> ready'}</Text>
          )}
        </Box>
      </Box>
    );
  };

  return { App, commitTurn: () => commitTurn() };
}

function terminalContent(terminal: TestTerminal): string {
  const buf = terminal.xtermBuffer();
  const rows: string[] = [];
  for (let i = 0; i < buf.length; i++) {
    rows.push(buf.getLine(i)?.translateToString(true) ?? '');
  }
  return rows.join('\n');
}

// ─── Test Matrix ──────────────────────────────────────────────────────────────

describe('overflow-flush regression matrix', () => {
  let originalNoSync: string | undefined;

  beforeEach(() => {
    originalNoSync = process.env['TWINKI_NO_SYNC'];
  });
  afterEach(() => {
    if (originalNoSync === undefined) delete process.env['TWINKI_NO_SYNC'];
    else process.env['TWINKI_NO_SYNC'] = originalNoSync;
  });

  // ── Scenario 1: Long conversation, turn commit ──────────────────────────────

  describe('long conversation + turn commit (overflow trigger)', () => {
    describe('preserve=false (default user experience)', () => {
      it('BSU terminal: full-frame in BSU (invisible but heavy)', async () => {
        const terminal = new TestTerminal(COLS, ROWS);
        const { App, commitTurn } = createLongConversationApp();
        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: false,
        });
        await wait(40);
        await terminal.flush();

        const { getMetrics } = captureAndMeasure(terminal);
        commitTurn();
        await wait(60);
        await terminal.flush();

        const m = getMetrics();
        // Destroys scrollback (bad)
        expect(m.scrollbackDestroyed).toBe(true);
        // Full frame re-emitted inside BSU
        expect(m.fullFrameInBSU).toBe(true);
        // All newlines are inside BSU (invisible on BSU terminal)
        expect(m.unbufferedNewlines).toBe(0);
        // But on non-BSU, ALL these lines draw progressively
        console.log(
          `[preserve=false, BSU] total=${m.totalNewlines} bsu=${m.bsuNewlines} max-single=${m.maxSingleWriteNewlines}`
        );
        inst.unmount();
      });

      it('non-BSU terminal: full progressive scroll (worst case)', async () => {
        process.env['TWINKI_NO_SYNC'] = '1';
        const terminal = new TestTerminal(COLS, ROWS);
        const { App, commitTurn } = createLongConversationApp();
        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: false,
        });
        await wait(40);
        await terminal.flush();

        const { getMetrics } = captureAndMeasure(terminal);
        commitTurn();
        await wait(60);
        await terminal.flush();

        const m = getMetrics();
        expect(m.scrollbackDestroyed).toBe(true);
        // No BSU wrapping — all newlines are unbuffered (progressive)
        expect(m.bsuNewlines).toBe(0);
        // This is what tmux 3.6a users see: every line draws individually
        console.log(
          `[preserve=false, NO-BSU] progressive-lines=${m.unbufferedNewlines} max-single=${m.maxSingleWriteNewlines}`
        );
        // Document the severity: how many lines flash on screen
        expect(m.unbufferedNewlines).toBeGreaterThan(ROWS);
        inst.unmount();
      });
    });

    describe('preserve=true (opt-in mitigation)', () => {
      it('BSU terminal: preserved content + viewport tail (invisible)', async () => {
        const terminal = new TestTerminal(COLS, ROWS);
        const { App, commitTurn } = createLongConversationApp();
        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: true,
        });
        await wait(40);
        await terminal.flush();

        const { getMetrics } = captureAndMeasure(terminal);
        commitTurn();
        await wait(60);
        await terminal.flush();

        const m = getMetrics();
        // No scrollback destruction
        expect(m.scrollbackDestroyed).toBe(false);
        // No full-frame (doesn't contain HISTORY-0)
        expect(m.fullFrameInBSU).toBe(false);
        // All inside BSU (invisible on BSU terminals)
        expect(m.unbufferedNewlines).toBe(0);
        console.log(
          `[preserve=true, BSU] total=${m.totalNewlines} bsu=${m.bsuNewlines} preserved-content=${m.preservedContentInRepaint} max-single=${m.maxSingleWriteNewlines}`
        );
        inst.unmount();
      });

      it('non-BSU terminal: bounds progressive output to the viewport', async () => {
        process.env['TWINKI_NO_SYNC'] = '1';
        const terminal = new TestTerminal(COLS, ROWS);
        const { App, commitTurn } = createLongConversationApp();
        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: true,
        });
        await wait(40);
        await terminal.flush();

        const { getMetrics } = captureAndMeasure(terminal);
        commitTurn();
        await wait(60);
        await terminal.flush();

        const m = getMetrics();
        expect(m.scrollbackDestroyed).toBe(false);
        // No BSU wrapping, so every emitted newline is progressive.
        expect(m.bsuNewlines).toBe(0);
        console.log(
          `[preserve=true, NO-BSU] progressive-lines=${m.unbufferedNewlines} preserved=${m.preservedContentInRepaint} max-single=${m.maxSingleWriteNewlines}`
        );

        // Before the fix, pendingFlush pushed this from a viewport-sized
        // write to roughly 54 progressive lines.
        expect(m.maxSingleWriteNewlines).toBeLessThanOrEqual(
          ROWS + 5 // viewport tail + small margin for cursor positioning
        );
        expect(m.unbufferedNewlines).toBeLessThanOrEqual(ROWS + 5);
        inst.unmount();
      });

      it('production tmux path stays bounded when DEC 2026 markers are ignored', async () => {
        delete process.env['TWINKI_NO_SYNC'];
        const terminal = new TestTerminal(COLS, ROWS);
        const { App, commitTurn } = createLongConversationApp();
        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: true,
          // Production passes the detected capability separately from the
          // marker-emission kill switch. Old tmux ignores emitted BSU markers.
          synchronizedOutput: false,
        });
        await wait(40);
        await terminal.flush();

        const { captures, getMetrics } = captureAndMeasure(terminal, false);
        commitTurn();
        await wait(60);
        await terminal.flush();

        const m = getMetrics();
        expect(captures.some((c) => c.hasBSUStart && c.hasBSUEnd)).toBe(true);
        expect(m.bsuNewlines).toBe(0);
        expect(m.unbufferedNewlines).toBe(m.totalNewlines);
        expect(m.maxSingleWriteNewlines).toBeLessThanOrEqual(ROWS + 5);
        expect(m.unbufferedNewlines).toBeLessThanOrEqual(ROWS + 5);
        expect(
          captures.some((c) => c.has3J || c.has2J || c.data.includes('\x1b[H'))
        ).toBe(false);

        const content = terminalContent(terminal);
        expect(content).toContain('HISTORY-0');
        expect(content).toContain('MSG-29');
        expect(terminal.getViewport().join('\n')).toContain('> typing...');
        for (let i = 0; i < LONG_MSG_LINES; i++) {
          expect(
            content.includes(`STREAM-${i}`) || content.includes(`MSG-${i}`)
          ).toBe(true);
        }

        inst.unmount();
      });

      it('production tmux path preserves rows that were never streamed', async () => {
        delete process.env['TWINKI_NO_SYNC'];
        const terminal = new TestTerminal(COLS, ROWS);
        let finalize!: () => void;

        const App = () => {
          const [history, setHistory] = useState<string[]>(['HISTORY-0']);
          const [streaming, setStreaming] = useState(true);
          finalize = () => {
            setHistory((current) => [
              ...current,
              ...Array.from({ length: 40 }, (_, i) => `FINAL-${i}`),
            ]);
            setStreaming(false);
          };
          return (
            <Box flexDirection="column">
              <Static items={history}>
                {(line) => <Text key={line}>{line}</Text>}
              </Static>
              {streaming ? (
                Array.from({ length: 20 }, (_, i) => (
                  <Text key={i}>{`STREAMED-${i}`}</Text>
                ))
              ) : (
                <Text>READY</Text>
              )}
            </Box>
          );
        };

        const inst = render(<App />, {
          terminal,
          exitOnCtrlC: false,
          preserveScrollbackOnRedraw: true,
          synchronizedOutput: false,
        });
        await wait(40);
        await terminal.flush();

        const { getMetrics } = captureAndMeasure(terminal, false);
        finalize();
        await wait(60);
        await terminal.flush();

        const content = terminalContent(terminal);
        for (let i = 20; i < 40; i++) {
          expect(content).toContain(`FINAL-${i}`);
        }
        for (let i = 0; i < 20; i++) {
          expect(
            content.includes(`STREAMED-${i}`) || content.includes(`FINAL-${i}`)
          ).toBe(true);
        }
        expect(getMetrics().scrollbackDestroyed).toBe(false);

        inst.unmount();
      });
    });
  });

  // ── Scenario 2: Short conversation, turn commit (NO overflow) ───────────────

  describe('short conversation + turn commit (no overflow)', () => {
    it('preserve=false: no overflow flush, clean differential', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createShortConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: false,
      });
      await wait(40);
      await terminal.flush();

      const { getMetrics } = captureAndMeasure(terminal);
      commitTurn();
      await wait(60);
      await terminal.flush();

      const m = getMetrics();
      // No scrollback destruction (no overflow)
      expect(m.scrollbackDestroyed).toBe(false);
      // No massive repaint
      expect(m.maxSingleWriteNewlines).toBeLessThanOrEqual(ROWS);
      console.log(
        `[short, preserve=false] total=${m.totalNewlines} max-single=${m.maxSingleWriteNewlines}`
      );
      inst.unmount();
    });

    it('preserve=true: no overflow flush, clean differential', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createShortConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
      });
      await wait(40);
      await terminal.flush();

      const { getMetrics } = captureAndMeasure(terminal);
      commitTurn();
      await wait(60);
      await terminal.flush();

      const m = getMetrics();
      expect(m.scrollbackDestroyed).toBe(false);
      expect(m.maxSingleWriteNewlines).toBeLessThanOrEqual(ROWS);
      console.log(
        `[short, preserve=true] total=${m.totalNewlines} max-single=${m.maxSingleWriteNewlines}`
      );
      inst.unmount();
    });
  });

  // ── Scenario 3: Small update in long conversation ───────────────────────────

  describe('long conversation + small update (no overflow trigger)', () => {
    it('preserve=true, non-BSU: differential only, minimal lines', async () => {
      delete process.env['TWINKI_NO_SYNC'];
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn, sendSmallUpdate } = createLongConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
        synchronizedOutput: false,
      });
      await wait(40);
      await terminal.flush();

      // First: commit the turn to get into post-overflow steady state
      commitTurn();
      await wait(60);
      await terminal.flush();

      // Now: small update (just change footer text)
      const { getMetrics } = captureAndMeasure(terminal, false);
      sendSmallUpdate('> idle');
      await wait(40);
      await terminal.flush();

      const m = getMetrics();
      // Small update should NEVER trigger overflow or full redraws
      expect(m.scrollbackDestroyed).toBe(false);
      // Strategy 4 may erase 1 line (tail-deletion clears old footer row)
      expect(m.eraseWrites).toBeLessThanOrEqual(1);
      // At most a few lines changed (Strategy 4 differential)
      expect(m.totalNewlines).toBeLessThanOrEqual(3);
      console.log(
        `[small-update, preserve=true, NO-BSU] progressive-lines=${m.unbufferedNewlines} total=${m.totalNewlines}`
      );
      inst.unmount();
    });
  });

  // ── Scenario 4: Multiple consecutive turn commits ───────────────────────────

  describe('repeated overflow commits (steady-state behavior)', () => {
    it('preserve=true, non-BSU: each commit progressive lines should be bounded', async () => {
      delete process.env['TWINKI_NO_SYNC'];
      const terminal = new TestTerminal(COLS, ROWS);
      let addTurn!: (id: number) => void;
      let turnCount = 0;

      const App = () => {
        const [turns, setTurns] = useState<string[][]>([
          Array.from({ length: 20 }, (_, i) => `T0-LINE-${i}`),
        ]);
        const [live, setLive] = useState<string[]>(
          Array.from({ length: 20 }, (_, i) => `LIVE-${i}`)
        );

        addTurn = (id: number) => {
          setTurns((t) => [...t, live]);
          setLive(Array.from({ length: 20 }, (_, i) => `T${id}-LIVE-${i}`));
        };

        return (
          <Box flexDirection="column">
            <Static items={turns.flat()}>
              {(line) => <Text key={line}>{line}</Text>}
            </Static>
            <Box flexDirection="column">
              {live.map((line, i) => (
                <Text key={i}>{line}</Text>
              ))}
            </Box>
          </Box>
        );
      };

      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
        synchronizedOutput: false,
      });
      await wait(40);
      await terminal.flush();

      const metrics: FlushMetrics[] = [];

      // Commit 3 turns in sequence, measure each
      for (let i = 1; i <= 3; i++) {
        const { getMetrics } = captureAndMeasure(terminal, false);
        addTurn(i);
        await wait(60);
        await terminal.flush();
        metrics.push(getMetrics());
      }

      console.log('[repeated commits, preserve=true, NO-BSU]');
      for (let i = 0; i < metrics.length; i++) {
        const m = metrics[i]!;
        console.log(
          `  turn ${i + 1}: progressive=${m.unbufferedNewlines} max-single=${m.maxSingleWriteNewlines} 3J=${m.scrollbackDestroyed} erases=${m.eraseWrites}`
        );
      }

      // REGRESSION CHECK: Each commit should write at most ROWS lines
      // (viewport tail), not the growing preserved content
      for (let i = 0; i < metrics.length; i++) {
        expect(metrics[i]!.maxSingleWriteNewlines).toBeLessThanOrEqual(
          ROWS + 5 // viewport + small margin
        );
        expect(metrics[i]!.unbufferedNewlines).toBeLessThanOrEqual(ROWS + 5);
        expect(metrics[i]!.scrollbackDestroyed).toBe(false);
      }

      const rows = terminalContent(terminal)
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean);
      for (let i = 0; i < 20; i++) {
        expect(rows.filter((line) => line === `LIVE-${i}`).length).toBe(1);
        expect(rows.filter((line) => line === `T1-LIVE-${i}`).length).toBe(1);
        expect(rows.filter((line) => line === `T2-LIVE-${i}`).length).toBe(1);
      }

      inst.unmount();
    });
  });

  // ══════════════════════════════════════════════════════════════════════════════
  // FRAME-BASED VALIDATION: Uses TestTerminal frame capture for deterministic
  // visual assertions. These verify WHAT THE USER SEES, not just byte counts.
  // ══════════════════════════════════════════════════════════════════════════════

  describe('frame-based: visual content validation after overflow', () => {
    it('viewport shows correct content after turn commit (preserve=true)', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createLongConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
      });
      await wait(40);
      await terminal.flush();

      // Capture frame before commit
      const beforeViewport = terminal.getViewport();
      expect(beforeViewport.some((l) => l.includes('STREAM-'))).toBe(true);

      commitTurn();
      await wait(60);
      await terminal.flush();

      // After commit: viewport should show the footer (new live content)
      const afterViewport = terminal.getViewport();
      const hasFooter = afterViewport.some((l) => l.includes('> typing...'));
      expect(hasFooter).toBe(true);

      // History should be in scrollback, not viewport
      const hasHistoryInViewport = afterViewport.some((l) =>
        l.includes('HISTORY-0')
      );
      // With a long static prefix, HISTORY-0 is far above the viewport
      expect(hasHistoryInViewport).toBe(false);

      inst.unmount();
    });

    it('no flicker during turn commit (preserve=true)', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createLongConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
      });
      await wait(40);
      await terminal.flush();

      commitTurn();
      await wait(60);
      await terminal.flush();

      const frames = terminal.getFrames();
      const flicker = analyzeFlicker(frames);

      // No cell should go non-blank → blank → non-blank
      expect(flicker.clean).toBe(true);

      inst.unmount();
    });

    it('scrollback contains committed content (preserve=true)', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createLongConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: true,
      });
      await wait(40);
      await terminal.flush();

      commitTurn();
      await wait(60);
      await terminal.flush();

      // Check the full xterm buffer (scrollback + viewport)
      const buf = terminal.xtermBuffer();
      const allLines: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        allLines.push(buf.getLine(i)?.translateToString(true) ?? '');
      }
      const fullContent = allLines.join('\n');

      // Committed content (MSG-*) should exist somewhere in the buffer
      expect(fullContent).toContain('MSG-0');
      expect(fullContent).toContain('MSG-24');
      // History should be preserved
      expect(fullContent).toContain('HISTORY-0');

      inst.unmount();
    });

    it('preserve=false: scrollback is destroyed after commit', async () => {
      const terminal = new TestTerminal(COLS, ROWS);
      const { App, commitTurn } = createLongConversationApp();
      const inst = render(<App />, {
        terminal,
        exitOnCtrlC: false,
        preserveScrollbackOnRedraw: false,
      });
      await wait(40);
      await terminal.flush();

      commitTurn();
      await wait(60);
      await terminal.flush();

      // Check the full xterm buffer
      const buf = terminal.xtermBuffer();
      const allLines: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        allLines.push(buf.getLine(i)?.translateToString(true) ?? '');
      }
      const fullContent = allLines.join('\n');

      // With preserve=false, \x1b[3J clears scrollback.
      // Content re-emitted in fullRender should still be present
      // (it's re-written to the terminal after the clear)
      expect(fullContent).toContain('MSG-0');

      inst.unmount();
    });
  });
});
