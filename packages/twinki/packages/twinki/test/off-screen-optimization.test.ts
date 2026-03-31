import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import { TestTerminal, MutableComponent, wait } from './helpers.js';

describe('Off-screen change optimization', () => {
  it('skips full redraw when only off-screen lines change and content is growing', async () => {
    // Viewport is 5 rows. Fill with 10 lines so the top 5 are off-screen.
    const term = new TestTerminal(40, 5);
    const tui = new TUI(term);
    const comp = new MutableComponent();

    // Initial: 10 lines, viewport shows lines 5-9
    comp.lines = Array.from({ length: 10 }, (_, i) => `line ${i}`);
    tui.addChild(comp);
    tui.start();
    await wait();
    await term.flush();

    // Change only an off-screen line (line 0) and append a new line (growing)
    comp.lines = ['CHANGED line 0', ...comp.lines.slice(1), 'line 10'];
    tui.requestRender();
    await wait();
    await term.flush();

    const frames = term.getFrames();
    expect(frames.length).toBe(2);
    // The second frame must NOT be a full redraw
    expect(frames[1]!.isFull).toBe(false);

    tui.stop();
  });

  it('skips render entirely when only off-screen lines change and viewport is unchanged', async () => {
    const term = new TestTerminal(40, 5);
    const tui = new TUI(term);
    const comp = new MutableComponent();

    // 10 lines, viewport shows lines 5-9
    comp.lines = Array.from({ length: 10 }, (_, i) => `line ${i}`);
    tui.addChild(comp);
    tui.start();
    await wait();
    await term.flush();

    // Change only off-screen line 0, keep total count the same
    comp.lines = ['CHANGED line 0', ...comp.lines.slice(1)];
    tui.requestRender();
    await wait();
    await term.flush();

    const frames = term.getFrames();
    // Should still be 1 frame — the off-screen-only change produces no output
    expect(frames.length).toBe(1);

    tui.stop();
  });

  it('still does full redraw when content shrinks with off-screen changes', async () => {
    const term = new TestTerminal(40, 5);
    const tui = new TUI(term);
    const comp = new MutableComponent();

    comp.lines = Array.from({ length: 10 }, (_, i) => `line ${i}`);
    tui.addChild(comp);
    tui.start();
    await wait();
    await term.flush();

    // Shrink: remove lines (stale rows need clearing)
    comp.lines = comp.lines.slice(0, 7);
    comp.lines[0] = 'CHANGED line 0';
    tui.requestRender();
    await wait();
    await term.flush();

    const frames = term.getFrames();
    expect(frames.length).toBe(2);
    // Shrink case must trigger full redraw
    expect(frames[1]!.isFull).toBe(true);

    tui.stop();
  });
});
