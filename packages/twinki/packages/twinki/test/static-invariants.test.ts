/**
 * Static-buffer invariant gates.
 *
 * These encode three properties the renderer is SUPPOSED to hold. They were
 * documented in code comments rather than tests, and two of them regressed
 * silently when 625ec7857 removed the Yoga free and re-wired resetStatic()
 * to re-render all static items on resize.
 *
 * Gate 1 — Yoga trees of flushed static items are released.
 * Gate 2 — Frame cost is independent of static-scrollback size.
 * Gate 3 — Resize does not flicker.
 *
 * Gates that fail on the current tree are marked `it.fails` so the suite
 * stays green while pinning the exact gap. Flip them to `it` with the fix.
 */
import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TUI } from '../src/renderer/tui.js';
import { TestTerminal, MutableComponent, analyzeFlicker, wait } from './helpers.js';

// ── Gate 1: Yoga release ────────────────────────────────────────────────────

interface Msg {
  id: number;
  text: string;
}

/** Chat app shaped like ConversationView: all but the newest item are static. */
function createChatApp() {
  let addMsg!: (text: string) => void;
  function App() {
    const [messages, setMessages] = useState<Msg[]>([]);
    addMsg = (text: string) =>
      setMessages((prev) => [...prev, { id: prev.length, text }]);
    return React.createElement(
      Box,
      { flexDirection: 'column' },
      React.createElement(Static, { items: messages.slice(0, -1) }, (m: Msg) =>
        React.createElement(
          Box,
          { key: m.id },
          React.createElement(Text, null, m.text)
        )
      ),
      React.createElement(Text, null, `> ${messages.at(-1)?.text ?? ''}`)
    );
  }
  return { App, addMsg: (t: string) => addMsg(t) };
}

describe('Static invariant 1: flushed static items release their Yoga trees', () => {
  it.fails(
    'yogaNodeCount does not grow with the number of flushed static items',
    async () => {
      const term = new TestTerminal(80, 24);
      const { App, addMsg } = createChatApp();
      const instance = render(React.createElement(App), {
        terminal: term,
        exitOnCtrlC: false,
      });
      await wait(30);

      for (let i = 0; i < 10; i++) {
        addMsg(`message ${i}`);
        await wait(10);
      }
      const afterTen = instance.getMetrics().yogaNodeCount;

      for (let i = 10; i < 60; i++) {
        addMsg(`message ${i}`);
        await wait(10);
      }
      const afterSixty = instance.getMetrics().yogaNodeCount;

      instance.unmount();

      // 50 more flushed items must not add 50 more live Yoga subtrees.
      // Allow slack for the live region and the un-flushed tail item.
      expect(afterSixty).toBeLessThan(afterTen + 10);
    }
  );

  it('records the current (regressed) growth rate for reference', async () => {
    const term = new TestTerminal(80, 24);
    const { App, addMsg } = createChatApp();
    const instance = render(React.createElement(App), {
      terminal: term,
      exitOnCtrlC: false,
    });
    await wait(30);

    for (let i = 0; i < 10; i++) {
      addMsg(`message ${i}`);
      await wait(10);
    }
    const afterTen = instance.getMetrics().yogaNodeCount;

    for (let i = 10; i < 60; i++) {
      addMsg(`message ${i}`);
      await wait(10);
    }
    const afterSixty = instance.getMetrics().yogaNodeCount;
    instance.unmount();

    const perItem = (afterSixty - afterTen) / 50;
    console.log(
      `  yogaNodeCount: 10 items=${afterTen}, 60 items=${afterSixty} (${perItem.toFixed(2)} nodes retained per flushed item)`
    );
    expect(afterSixty).toBeGreaterThanOrEqual(afterTen);
  });
});

// ── Gate 2: frame work is independent of static-buffer size ────────────────
//
// Deterministic, not timing-based. `tui.perfLastFrame` counts the exact number
// of iterations each formerly-O(prefix) loop performed. If frame work is truly
// independent of scrollback size then, for a fixed live region, every counter
// is IDENTICAL at 1,000 and 10,000 accumulated static lines. That is an
// equality assertion — no thresholds, no machine dependence, no flake.
//
// Before this optimization all three counters equalled prefixLines + liveLines,
// so the same comparison would have differed by exactly 9,000.

const LIVE_LINES = 12;

/** Renders `frames` differential frames and returns the last frame's counters. */
function frameWork(staticLines: number, wideLines = false) {
  const term = new TestTerminal(120, 40);
  const tui = new TUI(term, { staticScrollbackCap: 1_000_000, wideLines });
  const live = new MutableComponent();
  const setLive = (tick: number) => {
    const out: string[] = [];
    for (let i = 0; i < LIVE_LINES; i++) {
      out.push(i === LIVE_LINES - 1 ? `streaming tail ${tick}` : `live line ${i}`);
    }
    live.lines = out;
  };
  setLive(0);
  tui.addChild(live);
  tui.start();

  if (staticLines > 0) {
    const lines: string[] = [];
    for (let i = 0; i < staticLines; i++) {
      lines.push(`[${i}] committed scrollback line with some content`);
    }
    tui.writeStaticLines(lines);
  }

  const renderNow = () => (tui as unknown as { doRender(): void }).doRender();
  // Warm up so previousLines is established and we measure the differential
  // path (Strategy 4) rather than first-render.
  for (let i = 1; i <= 4; i++) {
    setLive(i);
    renderNow();
  }
  const baseCount = tui.perfRenderCount;
  const baseMs = tui.perfTotalRenderMs;
  for (let i = 5; i <= 104; i++) {
    setLive(i);
    renderNow();
  }
  const work = { ...tui.perfLastFrame };
  const avgMs =
    (tui.perfTotalRenderMs - baseMs) / Math.max(1, tui.perfRenderCount - baseCount);
  const staticBufferLines = tui.staticBufferLines;
  const fullRedraws = tui.fullRedraws;
  tui.stop();
  return { work, avgMs, staticBufferLines, fullRedraws };
}

describe('Static invariant 2: frame work is independent of static-buffer size', () => {
  it('performs identical per-frame work at 1k and 10k static lines', () => {
    const small = frameWork(1_000);
    const large = frameWork(10_000);

    console.log(
      `  1k:  scanned=${small.work.diffScanned} resets=${small.work.resetsApplied} cursor=${small.work.cursorScanned} prefix=${small.work.prefixLines}\n` +
        `  10k: scanned=${large.work.diffScanned} resets=${large.work.resetsApplied} cursor=${large.work.cursorScanned} prefix=${large.work.prefixLines}`
    );

    // Premise guards: the buffers really are the sizes we think, and we stayed
    // on the differential path (a full redraw per frame would void the test).
    expect(small.work.prefixLines).toBe(1_000);
    expect(large.work.prefixLines).toBe(10_000);
    expect(large.fullRedraws).toBeLessThan(5);

    // The invariant: 9,000 extra scrollback lines cost zero extra work.
    expect(large.work.diffScanned).toBe(small.work.diffScanned);
    expect(large.work.resetsApplied).toBe(small.work.resetsApplied);
    expect(large.work.cursorScanned).toBe(small.work.cursorScanned);
    expect(large.work.prefixSkipped).toBe(true);
  });

  it('scans only the live region, never the prefix', () => {
    const { work } = frameWork(10_000);
    // Work is bounded by the live region, not the 10,000-line prefix.
    expect(work.diffScanned).toBeLessThanOrEqual(LIVE_LINES + 1);
    expect(work.resetsApplied).toBeLessThanOrEqual(LIVE_LINES + 1);
    expect(work.cursorScanned).toBe(0);
  });

  it('holds with wideLines enabled (lite mode)', () => {
    const small = frameWork(1_000, true);
    const large = frameWork(10_000, true);
    expect(large.work.diffScanned).toBe(small.work.diffScanned);
    expect(large.work.resetsApplied).toBe(small.work.resetsApplied);
    expect(large.work.cursorScanned).toBe(small.work.cursorScanned);
  });

  it('records the wall-clock curve for reference (informational, not asserted)', () => {
    const rows: string[] = [];
    for (const n of [0, 1_000, 5_000, 10_000]) {
      const r = frameWork(n);
      rows.push(
        `${String(n).padStart(6)} static lines → ${r.avgMs.toFixed(3)} ms/frame ` +
          `(scanned ${r.work.diffScanned} lines)`
      );
    }
    console.log('  ' + rows.join('\n  '));
    expect(rows.length).toBe(4);
  });
});

// ── Gate 4: a static flush is always painted ────────────────────────────────
//
// Regression test for the stale-bar bug. Messages are rendered in the live tail
// with an "active" left bar, then flushed to scrollback WITHOUT the bar. If the
// flush frame is skipped by the differential diff, the bar-styled rows stay in
// scrollback forever (until a resize rebuilds it), which is what users saw.
//
// The original prefix-alignment guard compared the last prefix line by content
// and assumed that proved object identity. `===` on strings compares by VALUE,
// so a blank spacer line at the end of the prefix — extremely common — made the
// guard report "prefix unchanged" and skip the entire flush. This reproduces
// exactly that shape: the flushed batch ends in a blank line.

describe('Static invariant 4: a static flush is always painted', () => {
  it('writes newly flushed static lines even when the prefix ends in a blank line', () => {
    const term = new TestTerminal(80, 24);
    const writes: string[] = [];
    const originalWrite = term.write.bind(term);
    term.write = (data: string) => {
      writes.push(data);
      originalWrite(data);
    };

    const tui = new TUI(term);
    const live = new MutableComponent();
    // Live tail as it looks while "active": a styled bar column, then a blank.
    live.lines = ['\x1b[48;5;99m \x1b[0m message rendered while active', ''];
    tui.addChild(live);
    tui.start();

    const renderNow = () => (tui as unknown as { doRender(): void }).doRender();

    // First batch of committed scrollback, ending in a blank line.
    tui.writeStaticLines(['older committed line', '']);
    renderNow();
    renderNow();

    // Turn completes: the tail is flushed to scrollback WITHOUT the bar, and
    // the batch again ends in a blank line so the last prefix slot matches
    // what `previousLines` holds at that index.
    writes.length = 0;
    tui.writeStaticLines(['  message rendered while active', '']);
    live.lines = ['fresh live row', ''];
    renderNow();

    const painted = writes.join('');
    tui.stop();

    // The un-barred version must reach the terminal on the flush frame.
    expect(painted).toContain('message rendered while active');
    // And it must be the un-barred copy, not only the styled original.
    expect(painted).not.toMatch(/\x1b\[48;5;99m \x1b\[0m message rendered/);
  });
});

describe('Static invariant 5: overlays do not contaminate the static prefix', () => {
  it('restores static rows after an overlay closes', async () => {
    const term = new TestTerminal(40, 10);
    const tui = new TUI(term);
    const live = new MutableComponent();
    live.lines = ['LIVE'];
    tui.writeStaticLines(['STATIC ORIGINAL']);
    tui.addChild(live);
    tui.start();
    await wait();
    await term.flush();

    const overlay = new MutableComponent();
    overlay.lines = ['OVERLAY'];
    const handle = tui.showOverlay(overlay, {
      anchor: 'top-left',
      width: 20,
    });
    await wait();
    await term.flush();
    expect(term.getViewport()[0]).toContain('OVERLAY');

    handle.hide();
    await wait();
    await term.flush();

    expect(term.getViewport()[0]?.trim()).toBe('STATIC ORIGINAL');
    expect(tui.perfLastFrame.prefixSkipped).toBe(false);
    tui.stop();
  });

  it('keeps the prefix fast path active while overlays are hidden', async () => {
    const term = new TestTerminal(80, 20);
    const tui = new TUI(term, { staticScrollbackCap: 2_000 });
    const live = new MutableComponent();
    live.lines = ['LIVE'];
    tui.writeStaticLines(
      Array.from({ length: 1_000 }, (_, index) => `STATIC ${index}`)
    );
    tui.addChild(live);
    tui.start();
    await wait();
    await term.flush();

    const overlay = new MutableComponent();
    overlay.lines = ['OVERLAY'];
    const handle = tui.showOverlay(overlay, { anchor: 'top-left' });
    await wait();
    handle.setHidden(true);
    await wait();

    live.lines = ['LIVE UPDATED'];
    tui.requestRender();
    await wait();

    expect(tui.perfLastFrame.prefixSkipped).toBe(true);
    expect(tui.perfLastFrame.diffScanned).toBe(1);
    expect(tui.perfLastFrame.resetsApplied).toBe(1);
    tui.stop();
  });
});

//
// ── Gate 3: resize clears atomically ────────────────────────────────────────
//
// NOTE: `analyzeFlicker` cannot see the resize flash. Frames are only captured
// on writes containing `\x1b[?2026l` (synchronized-update end), and the resize
// pre-clear in `TUI.start()`'s doResize() is written bare — so the blank
// intermediate state never becomes a frame. The detector also skips frames
// where content height changed, which a resize always does.
//
// So we assert the mechanism instead: every clear sequence a resize emits must
// be inside a synchronized-update block. An unwrapped `\x1b[3J`/`\x1b[2J` is a
// screen the user sees empty before the repaint lands.

describe('Static invariant 3: resize clears inside a synchronized update', () => {
  it.fails(
    'resize emits no unsynchronized clear sequence',
    async () => {
      const term = new TestTerminal(60, 12);
      const writes: string[] = [];
      const originalWrite = term.write.bind(term);
      term.write = (data: string) => {
        writes.push(data);
        originalWrite(data);
      };

      const tui = new TUI(term);
      const live = new MutableComponent();
      live.lines = ['live one', 'live two', 'live three'];
      tui.addChild(live);
      tui.start();
      await wait();

      tui.writeStaticLines(
        Array.from({ length: 30 }, (_, i) => `static scrollback line ${i}`)
      );
      await wait();
      await term.flush();

      writes.length = 0;

      // twinki throttles resize at 100ms (leading + trailing), so wait past
      // the window or the resizes coalesce into a single render.
      for (const cols of [50, 60, 50]) {
        term.resize(cols, 12);
        await wait(130);
        await term.flush();
      }
      tui.stop();

      const unsynchronized = writes.filter(
        (w) =>
          (w.includes('\x1b[3J') || w.includes('\x1b[2J')) &&
          !w.includes('\x1b[?2026h')
      );
      if (unsynchronized.length > 0) {
        console.log(
          `  ${unsynchronized.length} unsynchronized clear write(s); first: ${JSON.stringify(unsynchronized[0]!.slice(0, 40))}`
        );
      }

      expect(unsynchronized).toEqual([]);
    }
  );

  it('resize itself produces no detectable flicker in the headless harness', async () => {
    const term = new TestTerminal(60, 12);
    const tui = new TUI(term);
    const live = new MutableComponent();
    live.lines = ['live one', 'live two', 'live three'];
    tui.addChild(live);
    tui.start();
    await wait();

    tui.writeStaticLines(
      Array.from({ length: 30 }, (_, i) => `static scrollback line ${i}`)
    );
    await wait();
    await term.flush();

    for (const cols of [50, 60, 50, 60]) {
      term.resize(cols, 12);
      await wait(130);
      await term.flush();
    }

    const report = analyzeFlicker(term.getFrames());
    tui.stop();

    // Passes today, and must keep passing after the fix. This is a
    // no-regression floor, NOT evidence that resize is flash-free — see the
    // note above. Real-terminal evidence (Terminal.app, tmux) is required
    // separately; xterm.js reflows on resize, so this harness only ever
    // exercises the reflowing-terminal case.
    expect(report.clean).toBe(true);
  });
});
