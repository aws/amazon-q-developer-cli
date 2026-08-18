import { describe, it, expect, afterEach } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import { CURSOR_MARKER } from '../src/renderer/component.js';
import { TestTerminal, MutableComponent, wait } from './helpers.js';

/**
 * Drives the TUI directly (no React) through the straddled-line capture, so
 * the frames land exactly as constructed with no reconciler batching or
 * self-healing intermediate renders. Pins the bookkeeping the React-pipeline
 * suite cannot isolate: the post-skip short paint's viewport anchor, and the
 * capture walk's on-screen extent after that short paint.
 */
describe('straddled-line capture bookkeeping (bare TUI)', () => {
  const COLS = 60;
  const ROWS = 10;

  const RULE_TOP = '--- thinking ---------------------------------------';
  const RULE_BOT = '----------------------------------------------------';
  // ~8 physical rows at 60 cols.
  const PARA = Array.from(
    { length: 8 },
    (_, i) => `SENT-${i} the quick brown fox jumps over the lazy dog.`
  ).join(' ');

  let tui: TUI | null = null;
  afterEach(() => {
    tui?.stop();
    tui = null;
  });

  async function frame(
    term: TestTerminal,
    comp: MutableComponent,
    lines: string[]
  ) {
    comp.lines = lines;
    tui!.requestRender();
    await wait(30);
    await term.flush();
  }

  function bufferLines(term: TestTerminal): string[] {
    const buf = term.xtermBuffer();
    const all: string[] = [];
    for (let i = 0; i < buf.length; i++) {
      const line = (buf.getLine(i)?.translateToString(true) ?? '').trim();
      if (line) all.push(line);
    }
    return all;
  }

  it('a second overflow flush after the short paint does not wipe or duplicate the straddled line', async () => {
    const term = new TestTerminal(COLS, ROWS);
    tui = new TUI(term, {
      wideLines: true,
      preserveScrollbackOnRedraw: true,
    });
    const comp = new MutableComponent();
    tui.addChild(comp);
    tui.start();
    await wait(30);
    await term.flush();

    // Streaming: paragraph straddles, tool + progress bars below. All live
    // (nothing flushed yet).
    await frame(term, comp, [
      'HIST-0',
      RULE_TOP,
      PARA,
      RULE_BOT,
      'TOOL-RUNNING',
      'BAR-0',
      'BAR-1',
      'OLDCHROME',
    ]);

    // Shrink flush: bars vanish, TOOL finalizes -> capture excludes PARA,
    // erase stops under its painted rows, tail skip -> short paint. Flushed
    // lines leave the live component, as the reconciler does.
    tui.writeStaticLines(['HIST-0', 'TOOL-DONE']);
    await frame(term, comp, [RULE_TOP, PARA, RULE_BOT, 'OLDCHROME']);

    // Second overflow flush lands directly on the short-paint bookkeeping:
    // the thinking block finalizes while a table streams in below.
    tui.writeStaticLines([RULE_TOP, PARA, RULE_BOT]);
    await frame(term, comp, [
      ...Array.from({ length: 9 }, (_, i) => `TABLE-${i}`),
      'OLDCHROME',
    ]);

    // Settle.
    await frame(term, comp, [
      ...Array.from({ length: 9 }, (_, i) => `TABLE-${i}`),
      'NEWCHROME',
    ]);

    const all = bufferLines(term);
    const joined = all.join('\n').replace(/\n/g, '');
    // The straddled paragraph survives intact, exactly once: an overstated
    // anchor lets the second flush's up-move climb over its painted rows
    // and wipe them; an over-walked capture re-emits a second copy.
    expect(joined.split('SENT-0 the quick').length - 1).toBe(1);
    expect(
      joined.split('SENT-7 the quick brown fox jumps over the lazy dog.')
        .length - 1
    ).toBe(1);
    for (let i = 0; i < 9; i++) {
      expect(all.filter((l) => l === `TABLE-${i}`).length).toBe(1);
    }
    expect(all.filter((l) => /^TABLE-\d+[^\d\s]/.test(l))).toEqual([]);
  });

  it('a cursor parked above trailing chrome does not shrink the captured window', async () => {
    const term = new TestTerminal(COLS, ROWS);
    tui = new TUI(term, {
      wideLines: true,
      preserveScrollbackOnRedraw: true,
    });
    const comp = new MutableComponent();
    tui.addChild(comp);
    tui.start();
    await wait(30);
    await term.flush();

    // The prompt input carries the cursor marker; two chrome rows render
    // below it, so the hardware cursor parks two rows above the frame end.
    const LIVE = Array.from({ length: 12 }, (_, i) => `LINE-${i}`);
    await frame(term, comp, [
      'HIST-0',
      ...LIVE,
      `PROMPT${CURSOR_MARKER}`,
      'HINT-ROW',
      'OLDCHROME',
    ]);

    // Flush while the live region overflows. The capture window is bounded
    // by the on-screen extent measured from the frame end; measuring it
    // from the cursor would leave the oldest visible rows erased but
    // uncaptured — wiped from scrollback.
    tui.writeStaticLines(['HIST-0', LIVE.slice(0, 6).join('\n')]);
    await frame(term, comp, [
      ...LIVE.slice(6),
      `PROMPT${CURSOR_MARKER}`,
      'HINT-ROW',
      'OLDCHROME',
    ]);

    // Settle.
    await frame(term, comp, [
      ...LIVE.slice(6),
      'PROMPT',
      'HINT-ROW',
      'NEWCHROME',
    ]);

    const all = bufferLines(term);
    for (const line of LIVE) {
      expect(all.filter((l) => l === line).length).toBe(1);
    }
    expect(all.filter((l) => l === 'HINT-ROW').length).toBeLessThanOrEqual(2);
  });

  it('a diff render touching the straddled line between flushes neither wipes nor duplicates its painted rows', async () => {
    const term = new TestTerminal(COLS, ROWS);
    tui = new TUI(term, {
      wideLines: true,
      preserveScrollbackOnRedraw: true,
    });
    const comp = new MutableComponent();
    tui.addChild(comp);
    tui.start();
    await wait(30);
    await term.flush();

    // Taller than the viewport (~16 rows at 60 cols): its top rows are in
    // scrollback by the time of the flush, so a relative up-move to its
    // frame start clamps at the screen top and lands short.
    const PARA_BIG = Array.from(
      { length: 16 },
      (_, i) => `SENT-${i} the quick brown fox jumps over the lazy dog.`
    ).join(' ');

    // Streaming: paragraph straddles, tool + bars below.
    await frame(term, comp, [
      'HIST-0',
      RULE_TOP,
      PARA_BIG,
      RULE_BOT,
      'TOOL-RUNNING',
      'BAR-0',
      'BAR-1',
      'OLDCHROME',
    ]);

    // Shrink flush -> capture excludes PARA_BIG -> short paint, its
    // on-screen rows left painted above the owned boundary.
    tui.writeStaticLines(['HIST-0', 'TOOL-DONE']);
    await frame(term, comp, [RULE_TOP, PARA_BIG, RULE_BOT, 'OLDCHROME']);

    // Ordinary diff render touching the straddled line: the thinking
    // stream grows it by a token. The change's physical row falls in the
    // owned band; the differential path's relative up-move cannot reach
    // it and would land short, wiping the painted rows.
    const PARA_GROWN = PARA_BIG + ' EXTRA-TOKEN';
    await frame(term, comp, [RULE_TOP, PARA_GROWN, RULE_BOT, 'OLDCHROME']);

    // The grown block finalizes while a table streams in.
    tui.writeStaticLines([RULE_TOP, PARA_GROWN, RULE_BOT]);
    await frame(term, comp, [
      ...Array.from({ length: 9 }, (_, i) => `TABLE-${i}`),
      'OLDCHROME',
    ]);
    await frame(term, comp, [
      ...Array.from({ length: 9 }, (_, i) => `TABLE-${i}`),
      'NEWCHROME',
    ]);

    const all = bufferLines(term);
    const joined = all.join('\n').replace(/\n/g, '');
    // The grown form commits exactly once. The pre-growth form's rows were
    // partially committed to immutable scrollback before the growth, so at
    // most one whole, ordered stale copy may remain (variant-E contract);
    // fused or torn rows mean the diff path climbed into the painted band.
    expect(joined.split('EXTRA-TOKEN').length - 1).toBe(1);
    expect(joined.split('SENT-0 the quick').length - 1).toBeLessThanOrEqual(2);
    expect(
      joined.split('SENT-15 the quick brown fox jumps over the lazy dog.')
        .length - 1
    ).toBeGreaterThanOrEqual(1);
    for (let i = 0; i < 9; i++) {
      expect(all.filter((l) => l === `TABLE-${i}`).length).toBe(1);
    }
    expect(all.filter((l) => /^TABLE-\d+[^\d\s]/.test(l))).toEqual([]);
    expect(all.filter((l) => l === 'TOOL-DONE').length).toBe(1);
  });
});
