import { describe, it, expect } from 'vitest';
import React, { useState, useLayoutEffect } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Repro for thinking-block duplication observed in the lite UI: a reasoning
 * block stays in the live region while a table streams below it, the live
 * region far exceeds the viewport, and a mid-turn static flush triggers the
 * capture/erase path. The captured reasoning rows must not be re-committed
 * to scrollback when their content survives into the repaint or the final
 * static flush.
 */
describe('overflow flush with a persistent block above a streaming tail', () => {
  const COLS = 60;
  const ROWS = 10;

  const RULE_TOP = '--- thinking ---------------------------------------';
  const RULE_BOT = '----------------------------------------------------';
  const THOUGHT = Array.from({ length: 4 }, (_, i) => `THOUGHT-${i}`);
  const THINK = [RULE_TOP, ...THOUGHT, RULE_BOT];
  const TABLE = Array.from({ length: 14 }, (_, i) => `TABLE-${i}`);

  interface Phase {
    history: string[];
    live: string[];
    streaming: boolean;
  }

  async function run(phases: Phase[], opts?: { wideLines?: boolean }) {
    const terminal = new TestTerminal(COLS, ROWS);
    let setPhase!: (p: Phase) => void;
    const App = () => {
      const [phase, set] = useState<Phase>(phases[0]!);
      setPhase = set;
      return (
        <Box flexDirection="column">
          <Static items={phase.history}>{(h) => <Text wrap="overflow">{h}</Text>}</Static>
          <Box flexDirection="column">
            {phase.live.map((line, i) => (
              <Text key={i} wrap="overflow">
                {line}
              </Text>
            ))}
            <Text>{phase.streaming ? 'OLDCHROME' : 'NEWCHROME'}</Text>
          </Box>
        </Box>
      );
    };
    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      preserveScrollbackOnRedraw: true,
      wideLines: opts?.wideLines,
    });
    try {
      for (let i = 1; i < phases.length; i++) {
        await wait(40);
        await terminal.flush();
        setPhase(phases[i]!);
      }
      await wait(60);
      await terminal.flush();
      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        const line = (buf.getLine(i)?.translateToString(true) ?? '').trim();
        if (line) all.push(line);
      }
      return all;
    } finally {
      inst.unmount();
    }
  }

  function countDupes(all: string[], lines: string[]): string[] {
    const bad: string[] = [];
    for (const line of lines) {
      const n = all.filter((l) => l === line).length;
      if (n !== 1) bad.push(`${line}=${n}`);
    }
    return bad;
  }

  it('variant A: unrelated early flush captures, thinking flushes later mid-stream', async () => {
    const all = await run([
      // Turn streaming: thinking + partial table, live region overflows.
      {
        history: ['HIST-0'],
        live: [...THINK, ...TABLE.slice(0, 6)],
        streaming: true,
      },
      // A small unrelated row finalizes mid-turn -> capture fires here.
      {
        history: ['HIST-0', 'TOOL-ROW'],
        live: [...THINK, ...TABLE.slice(0, 8)],
        streaming: true,
      },
      // Thinking block finalizes AFTER the capture while the table streams.
      {
        history: ['HIST-0', 'TOOL-ROW', THINK.join('\n')],
        live: [...TABLE.slice(0, 11)],
        streaming: true,
      },
      // Table finalizes.
      {
        history: ['HIST-0', 'TOOL-ROW', THINK.join('\n'), TABLE.join('\n')],
        live: [],
        streaming: false,
      },
    ]);
    expect(countDupes(all, [...THINK, ...TABLE])).toEqual([]);
  });

  it('variant B: thinking flush itself captures while its body grew since last paint', async () => {
    const BODY_PARTIAL = 'THOUGHT-LAST generating verification ta';
    const BODY_FULL = 'THOUGHT-LAST generating verification table now';
    const THINK_OLD = [RULE_TOP, ...THOUGHT, BODY_PARTIAL, RULE_BOT];
    const THINK_NEW = [RULE_TOP, ...THOUGHT, BODY_FULL, RULE_BOT];
    const all = await run([
      {
        history: ['HIST-0'],
        live: [...THINK_OLD, ...TABLE.slice(0, 6)],
        streaming: true,
      },
      // Thinking finalizes with a GROWN body row (differs from painted form).
      {
        history: ['HIST-0', THINK_NEW.join('\n')],
        live: [...TABLE.slice(0, 10)],
        streaming: true,
      },
      {
        history: ['HIST-0', THINK_NEW.join('\n'), TABLE.join('\n')],
        live: [],
        streaming: false,
      },
    ]);
    const bad = countDupes(all, [...THINK_NEW, ...TABLE]);
    expect(bad).toEqual([]);
    expect(all.filter((l) => l === BODY_PARTIAL).length).toBe(0);
  });

  it('variant C: whole turn flushes at once at turn end (thinking + table)', async () => {
    const all = await run([
      {
        history: ['HIST-0'],
        live: [...THINK, ...TABLE.slice(0, 6)],
        streaming: true,
      },
      {
        history: ['HIST-0'],
        live: [...THINK, ...TABLE.slice(0, 10)],
        streaming: true,
      },
      {
        history: ['HIST-0'],
        live: [...THINK, ...TABLE],
        streaming: true,
      },
      // Everything flushes in one commit at turn end.
      {
        history: ['HIST-0', [...THINK, ...TABLE].join('\n')],
        live: [],
        streaming: false,
      },
    ]);
    expect(countDupes(all, [...THINK, ...TABLE])).toEqual([]);
  });

  // The lite UI's thinking body is ONE logical row soft-wrapped across many
  // physical rows (wideLines mode). While the table streams below it, the
  // body's top physical rows scroll off; at capture the body row straddles
  // the viewport top and is captured whole. The repaint must not re-commit
  // the whole paragraph mid-table when its content survives into the tail
  // or the eventual static flush.
  const PARA_SENTENCES = Array.from(
    { length: 8 },
    (_, i) => `SENT-${i} the quick brown fox jumps over the lazy dog.`
  );
  // ~8 sentences x ~50 chars = ~400 chars = ~7 physical rows at 60 cols.
  const PARA = PARA_SENTENCES.join(' ');
  const PARA_THINK = [RULE_TOP, PARA, RULE_BOT];

  it('variant D: wide straddling body row, thinking flushes mid-stream', async () => {
    const all = await run(
      [
        // Body straddles the viewport top: 3 + ~7 + tables > 10 rows.
        {
          history: ['HIST-0'],
          live: [...PARA_THINK, ...TABLE.slice(0, 4)],
          streaming: true,
        },
        // Thinking finalizes while the table streams -> flush + capture.
        {
          history: ['HIST-0', PARA_THINK.join('\n')],
          live: [...TABLE.slice(0, 8)],
          streaming: true,
        },
        {
          history: ['HIST-0', PARA_THINK.join('\n'), TABLE.join('\n')],
          live: [],
          streaming: false,
        },
      ],
      { wideLines: true }
    );
    // The paragraph soft-wraps, so count occurrences of its head sentence
    // across the joined buffer instead of exact row matches.
    const joined = all.join('\n').replace(/\n/g, '');
    const headCount = joined.split('SENT-0 the quick').length - 1;
    expect(headCount).toBe(1);
    expect(countDupes(all, TABLE)).toEqual([]);
  });

  it('variant E: wide straddling body row grows across the flush', async () => {
    const PARA_OLD = PARA_SENTENCES.slice(0, 7).join(' ') + ' SENT-7 the qui';
    const all = await run(
      [
        {
          history: ['HIST-0'],
          live: [RULE_TOP, PARA_OLD, RULE_BOT, ...TABLE.slice(0, 4)],
          streaming: true,
        },
        // Body grew between the last paint and the flush commit.
        {
          history: ['HIST-0', PARA_THINK.join('\n')],
          live: [...TABLE.slice(0, 8)],
          streaming: true,
        },
        {
          history: ['HIST-0', PARA_THINK.join('\n'), TABLE.join('\n')],
          live: [],
          streaming: false,
        },
      ],
      { wideLines: true }
    );
    const joined = all.join('\n').replace(/\n/g, '');
    // The straddling row's top rows were physically committed to scrollback
    // before the flush and cannot be unwritten, so its stale pre-growth form
    // may remain as ONE whole, correctly ordered copy above the finalized
    // block. The finalized form must appear exactly once, tables must not
    // duplicate, and no row may be cut mid-word into another row's cells.
    const finalTail = 'SENT-7 the quick brown fox jumps over the lazy dog.';
    expect(joined.split(finalTail).length - 1).toBe(1);
    const headCount = joined.split('SENT-0 the quick').length - 1;
    expect(headCount).toBeLessThanOrEqual(2);
    expect(countDupes(all, TABLE)).toEqual([]);
    // Mangling signature of the original bug: a table cell fused with a
    // fragment of the paragraph on one physical row.
    expect(all.filter((l) => /^TABLE-\d+[^\d\s]/.test(l))).toEqual([]);
  });

  // The transcript geometry: at capture the paragraph row straddles the
  // viewport top (top physical rows already committed in scrollback), the
  // capture is fired by an UNRELATED flush (no runs cover the paragraph),
  // and the rows below the paragraph fill the viewport so the repaint tail
  // starts below the paragraph's frame position. The captured-whole
  // paragraph then cannot dedup and is re-committed in full, duplicating
  // the part already in scrollback.
  it('variant G: unrelated flush captures while the wide body row straddles the viewport top', async () => {
    const all = await run(
      [
        // RULE_TOP(1) + PARA(~7) + RULE_BOT(1) + T0..5(6) + chrome(1) = ~16
        // physical rows: the paragraph straddles the viewport top.
        {
          history: ['HIST-0'],
          live: [RULE_TOP, PARA, RULE_BOT, ...TABLE.slice(0, 6)],
          streaming: true,
        },
        // An unrelated row finalizes -> capture fires; the table grows so
        // the repaint tail (last 10 rows) starts below the paragraph.
        {
          history: ['HIST-0', 'TOOL-ROW'],
          live: [RULE_TOP, PARA, RULE_BOT, ...TABLE.slice(0, 8)],
          streaming: true,
        },
        // Thinking finalizes, then the table.
        {
          history: ['HIST-0', 'TOOL-ROW', PARA_THINK.join('\n')],
          live: [...TABLE.slice(0, 10)],
          streaming: true,
        },
        {
          history: [
            'HIST-0',
            'TOOL-ROW',
            PARA_THINK.join('\n'),
            TABLE.join('\n'),
          ],
          live: [],
          streaming: false,
        },
      ],
      { wideLines: true }
    );
    const joined = all.join('\n').replace(/\n/g, '');
    const headCount = joined.split('SENT-0 the quick').length - 1;
    expect(headCount).toBe(1);
    expect(countDupes(all, TABLE)).toEqual([]);
  });

  // The capture computes its boundary over the OLD frame; the repaint tail
  // walks the NEW one. Growth below the straddling row pushes it out of the
  // tail (variants D/E/G); shrinkage — progress bars dropping out as a tool
  // settles in the same commit that flushes — pulls it back in, and the
  // tail would paint a second copy under the rows the erase left painted.
  it('variant H: rows below the straddling row shrink across the flush', async () => {
    const all = await run(
      [
        // PARA straddles; a running tool with progress bars sits below it.
        {
          history: ['HIST-0'],
          live: [RULE_TOP, PARA, RULE_BOT, 'TOOL-RUNNING', 'BAR-0', 'BAR-1'],
          streaming: true,
        },
        // The tool settles: bars vanish and the tool row finalizes in the
        // same commit -> flush + capture while the region below PARA shrinks.
        {
          history: ['HIST-0', 'TOOL-DONE'],
          live: [RULE_TOP, PARA, RULE_BOT],
          streaming: true,
        },
        // Thinking finalizes.
        {
          history: ['HIST-0', 'TOOL-DONE', PARA_THINK.join('\n')],
          live: [],
          streaming: false,
        },
      ],
      { wideLines: true }
    );
    const joined = all.join('\n').replace(/\n/g, '');
    const headCount = joined.split('SENT-0 the quick').length - 1;
    expect(headCount).toBe(1);
    expect(all.filter((l) => l === 'TOOL-DONE').length).toBe(1);
    // The vanished transients between RULE_BOT and the tail break suffix
    // contiguity, so RULE_BOT's captured copy is preserved above them while
    // the tail paints its committed twin: dropping a non-suffix match would
    // reorder scrollback, so the dedup accepts one bounded duplicate row.
    expect(
      all.filter((l) => l === RULE_BOT).length
    ).toBeLessThanOrEqual(2);
    // Transient bars may survive once via the captured copy, never twice.
    for (const bar of ['BAR-0', 'BAR-1']) {
      expect(all.filter((l) => l === bar).length).toBeLessThanOrEqual(1);
    }
  });

  // After a straddled-line skip the tail paints fewer rows than the
  // viewport. The post-paint anchor must describe that short paint: a
  // full-height anchor overstates the cursor's screen row, and the NEXT
  // overflow flush's relative up-move would climb over the straddled
  // line's painted rows and wipe them.
  it('variant I: a second overflow flush consumes the post-skip anchor', async () => {
    const all = await run(
      [
        {
          history: ['HIST-0'],
          live: [RULE_TOP, PARA, RULE_BOT, 'TOOL-RUNNING', 'BAR-0', 'BAR-1'],
          streaming: true,
        },
        // Shrink flush -> capture excludes PARA, tail skip -> short paint.
        {
          history: ['HIST-0', 'TOOL-DONE'],
          live: [RULE_TOP, PARA, RULE_BOT],
          streaming: true,
        },
        // Second overflow flush lands on the short-paint anchor directly:
        // no intermediate append render heals the viewport bookkeeping.
        {
          history: ['HIST-0', 'TOOL-DONE', PARA_THINK.join('\n')],
          live: [...TABLE.slice(0, 9)],
          streaming: true,
        },
        {
          history: [
            'HIST-0',
            'TOOL-DONE',
            PARA_THINK.join('\n'),
            TABLE.join('\n'),
          ],
          live: [],
          streaming: false,
        },
      ],
      { wideLines: true }
    );
    const joined = all.join('\n').replace(/\n/g, '');
    expect(joined.split('SENT-0 the quick').length - 1).toBe(1);
    // The paragraph's final sentence must survive the second flush intact —
    // a wiped straddle region loses its tail rows.
    expect(
      joined.split('SENT-7 the quick brown fox jumps over the lazy dog.')
        .length - 1
    ).toBe(1);
    expect(all.filter((l) => l === 'TOOL-DONE').length).toBe(1);
    expect(countDupes(all, TABLE)).toEqual([]);
    expect(all.filter((l) => /^TABLE-\d+[^\d\s]/.test(l))).toEqual([]);
  });

  // writeStaticLines runs synchronously during each React commit, but the
  // paint is deferred to process.nextTick. Two commits in one task (state
  // set inside a layout effect chains a second commit) therefore flush
  // static twice before the erased screen repaints. The first flush fires
  // the capture; the second lands in the flushedCount-only accounting with
  // no runs tying its rows to the captured live rows they replace.
  it('variant F: second flush lands between capture and repaint (chained commit)', async () => {
    const terminal = new TestTerminal(COLS, ROWS);
    interface P {
      history: string[];
      live: string[];
      streaming: boolean;
      chain?: { history: string[]; live: string[] };
    }
    const phases: P[] = [
      // Turn streaming: thinking + partial table overflow the viewport.
      {
        history: ['HIST-0'],
        live: [...THINK, ...TABLE.slice(0, 6)],
        streaming: true,
      },
      // One task, two commits: TOOL-ROW flushes (capture fires, screen
      // erased), then the layout effect immediately commits the thinking
      // flush before the nextTick repaint runs.
      {
        history: ['HIST-0', 'TOOL-ROW'],
        live: [...THINK, ...TABLE.slice(0, 8)],
        streaming: true,
        chain: {
          history: ['HIST-0', 'TOOL-ROW', THINK.join('\n')],
          live: [...TABLE.slice(0, 9)],
        },
      },
      // Table finalizes.
      {
        history: ['HIST-0', 'TOOL-ROW', THINK.join('\n'), TABLE.join('\n')],
        live: [],
        streaming: false,
      },
    ];
    let setPhase!: (p: P) => void;
    const App = () => {
      const [phase, set] = useState<P>(phases[0]!);
      setPhase = set;
      useLayoutEffect(() => {
        if (phase.chain) {
          set({ ...phase, ...phase.chain, chain: undefined });
        }
      }, [phase]);
      return (
        <Box flexDirection="column">
          <Static items={phase.history}>{(h) => <Text wrap="overflow">{h}</Text>}</Static>
          <Box flexDirection="column">
            {phase.live.map((line, i) => (
              <Text key={i} wrap="overflow">
                {line}
              </Text>
            ))}
            <Text>{phase.streaming ? 'OLDCHROME' : 'NEWCHROME'}</Text>
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
      for (let i = 1; i < phases.length; i++) {
        await wait(40);
        await terminal.flush();
        setPhase(phases[i]!);
      }
      await wait(60);
      await terminal.flush();
      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        const line = (buf.getLine(i)?.translateToString(true) ?? '').trim();
        if (line) all.push(line);
      }
      expect(countDupes(all, [...THINK, ...TABLE, 'TOOL-ROW'])).toEqual([]);
    } finally {
      inst.unmount();
    }
  });
});
