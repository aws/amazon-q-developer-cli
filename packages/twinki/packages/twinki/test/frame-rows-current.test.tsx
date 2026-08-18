import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import type { RenderCompletedEvent } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * frameRowsCurrent contract: a viewport-tail paint whose window excludes
 * live rows reports false and later differential paints inherit it; a paint
 * that covers the whole live region again (full render, or a tail paint
 * reaching the static prefix) reports true.
 */
describe('RenderCompletedEvent.frameRowsCurrent', () => {
  it('goes false while live rows sit above the tail window and recovers on a covering paint', async () => {
    const terminal = new TestTerminal(60, 10);
    let setLive!: (lines: string[]) => void;
    let setHistory!: (items: string[]) => void;
    const App = () => {
      const [live, setL] = useState<string[]>(['live-0']);
      const [history, setH] = useState<string[]>(['hist-0']);
      setLive = setL;
      setHistory = setH;
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text key={h}>{h}</Text>}</Static>
          {live.map((l) => (
            <Text key={l}>{l}</Text>
          ))}
        </Box>
      );
    };
    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      preserveScrollbackOnRedraw: true,
    });
    const events: RenderCompletedEvent[] = [];
    const unsubscribe = inst.onRenderComplete((e) => events.push(e));
    try {
      await wait(30);
      await terminal.flush();

      // Live region far taller than the 10-row viewport, then a static
      // flush: the repaint takes the viewport-tail path with live rows
      // above its window.
      const tall = Array.from({ length: 30 }, (_, i) => `live-tall-${i}`);
      setLive(tall);
      await wait(30);
      await terminal.flush();
      setHistory(['hist-0', 'flushed-1']);
      await wait(30);
      await terminal.flush();
      const staleEvents = events.filter((e) => !e.frameRowsCurrent);
      expect(staleEvents.length).toBeGreaterThan(0);

      // Differential paints over the stale prefix must not report current.
      events.length = 0;
      setLive([...tall.slice(0, -1), 'live-tall-29-changed']);
      await wait(30);
      await terminal.flush();
      expect(events.length).toBeGreaterThan(0);
      expect(events.every((e) => !e.frameRowsCurrent)).toBe(true);

      // Shrinking the live region so the flush repaint covers all of it
      // clears the staleness.
      events.length = 0;
      setLive(['live-final']);
      setHistory(['hist-0', 'flushed-1', ...tall]);
      await wait(30);
      await terminal.flush();
      expect(events.length).toBeGreaterThan(0);
      expect(events[events.length - 1]!.frameRowsCurrent).toBe(true);
    } finally {
      unsubscribe();
      inst.unmount();
    }
  });
  it('latches when the off-screen rescan skips a changed live row', async () => {
    const terminal = new TestTerminal(60, 10);
    let setLive!: (lines: string[]) => void;
    const App = () => {
      const [live, setL] = useState<string[]>(
        Array.from({ length: 30 }, (_, i) => `row-${i}`)
      );
      setLive = setL;
      return (
        <Box flexDirection="column">
          {live.map((l, i) => (
            <Text key={i}>{l}</Text>
          ))}
        </Box>
      );
    };
    const inst = render(<App />, { terminal, exitOnCtrlC: false });
    const events: RenderCompletedEvent[] = [];
    const unsubscribe = inst.onRenderComplete((e) => events.push(e));
    try {
      await wait(30);
      await terminal.flush();

      // Mutate only a row far above the previous viewport top while the
      // frame keeps its height: Strategy 4's rescan finds no in-viewport
      // change, paints nothing, and commits the changed row unwritten.
      events.length = 0;
      setLive([
        'row-0-changed-above-viewport',
        ...Array.from({ length: 29 }, (_, i) => `row-${i + 1}`),
      ]);
      await wait(30);
      await terminal.flush();
      expect(events.length).toBeGreaterThan(0);
      expect(events.every((e) => !e.frameRowsCurrent)).toBe(true);

      // A shrinking change above the viewport takes the full-render path,
      // which rewrites everything and recovers the guarantee.
      events.length = 0;
      setLive(['only-row']);
      await wait(30);
      await terminal.flush();
      expect(events.length).toBeGreaterThan(0);
      expect(events[events.length - 1]!.frameRowsCurrent).toBe(true);
    } finally {
      unsubscribe();
      inst.unmount();
    }
  });
});
