import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * A static flush that fires mid-stream captures the live stream tip in
 * whatever partial form it had at capture time (e.g. cut mid-word). By the
 * repaint, the tip has grown and new rows may sit between it and the footer.
 * The captured rows must still dedup against the tail — the stale tip is a
 * prefix of its grown form — instead of being re-committed to scrollback
 * above the tail's newer copy, which duplicated the whole captured block
 * with the older copy ending mid-word.
 */
describe('overflow flush mid-stream (stream advances across the flush)', () => {
  const COLS = 60;
  const ROWS = 10;

  const REASON = Array.from({ length: 5 }, (_, i) => `REASON-${i}`);
  const BODY = Array.from({ length: 12 }, (_, i) => `ANSWER-${i}`);
  const TIP_PARTIAL = 'ANSWER-TIP cached at session creatio';
  const TIP_FULL = 'ANSWER-TIP cached at session creation so even';
  const EXTRA = ['ANSWER-EXTRA refreshing the file will not fix it'];

  interface Phase {
    history: string[];
    live: string[];
    streaming: boolean;
  }

  async function run(phases: Phase[]) {
    const terminal = new TestTerminal(COLS, ROWS);
    let setPhase!: (p: Phase) => void;
    const App = () => {
      const [phase, set] = useState<Phase>(phases[0]!);
      setPhase = set;
      return (
        <Box flexDirection="column">
          <Static items={phase.history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {phase.live.map((line, i) => (
              <Text key={i}>{line}</Text>
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
      return all;
    } finally {
      inst.unmount();
    }
  }

  it('a stream tip that grows across the flush does not duplicate the captured block', async () => {
    const all = await run([
      // Streaming: reasoning + partial answer, live region taller than the
      // viewport so the flush takes the capture/erase path.
      { history: ['HIST-0'], live: [...REASON, ...BODY, TIP_PARTIAL], streaming: true },
      // Reasoning finalizes while the answer streams on: the same commit
      // flushes static lines AND advances the tip past the captured form.
      { history: ['HIST-0', REASON.join('\n')], live: [...BODY, TIP_FULL, ...EXTRA], streaming: true },
      // Answer finalizes.
      {
        history: ['HIST-0', REASON.join('\n'), [...BODY, TIP_FULL, ...EXTRA].join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    const bad: string[] = [];
    for (const line of [...REASON, ...BODY, TIP_FULL, ...EXTRA]) {
      const n = all.filter((l) => l === line).length;
      if (n !== 1) bad.push(`${line}=${n}`);
    }
    expect(bad).toEqual([]);
    // The stale mid-word tip must not be committed alongside its grown form.
    expect(all.filter((l) => l === TIP_PARTIAL).length).toBe(0);
  });

  it('a vanished row that prefixes an unrelated tail row is not dropped', async () => {
    // 'Reading file' vanishes between capture and repaint while a DIFFERENT
    // row extending it streams in elsewhere. The captured copy is the only
    // copy; an unscoped prefix allowance would dedup it against the
    // lookalike and commit it nowhere. Position scoping keeps it.
    const DONE_ROW = 'Reading file';
    const LOOKALIKE = 'Reading file src/foo.ts and more';
    const final = [...BODY, TIP_FULL, LOOKALIKE];
    const all = await run([
      { history: ['HIST-0'], live: [...REASON, ...BODY, DONE_ROW, TIP_PARTIAL], streaming: true },
      { history: ['HIST-0', REASON.join('\n')], live: [...BODY, TIP_FULL, LOOKALIKE], streaming: true },
      {
        history: ['HIST-0', REASON.join('\n'), final.join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    // No loss: the vanished row survives via its captured copy, and every
    // final row is committed at least once (duplication stays bounded).
    expect(all.filter((l) => l === DONE_ROW).length).toBe(1);
    for (const line of [...REASON, ...final]) {
      expect(all.filter((l) => l === line).length).toBeGreaterThanOrEqual(1);
    }
  });

  it('a captured row absent from the tail survives via the lossless fallback', async () => {
    // A transient status row vanishes between capture and repaint: its
    // captured copy is the only copy anywhere and must be preserved even
    // though the gaps-allowed matcher finds every row around it.
    const STATUS = 'STATUS working on it';
    const final = [...BODY, TIP_FULL, ...EXTRA];
    const all = await run([
      { history: ['HIST-0'], live: [...REASON, ...BODY, STATUS, TIP_PARTIAL], streaming: true },
      { history: ['HIST-0', REASON.join('\n')], live: [...BODY, TIP_FULL, ...EXTRA], streaming: true },
      {
        history: ['HIST-0', REASON.join('\n'), final.join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    expect(all.filter((l) => l === STATUS).length).toBe(1);
    for (const line of [...REASON, ...final]) {
      expect(all.filter((l) => l === line).length).toBeGreaterThanOrEqual(1);
    }
  });

  it('a flush that diverges mid-block (gapped runs) loses nothing', async () => {
    // One reasoning line is rewritten as it finalizes, splitting the
    // presented match into two runs with a gap. Gapped runs make the
    // grown-tip slot ambiguous, so the allowance must stay off: the tip
    // may duplicate (bounded) but no row may be lost.
    const X_STREAMED = 'X reading the config file';
    const X_FINAL = 'X read the config file (3 lines)';
    const REWRITTEN = ['REASON-0', 'REASON-1', 'REASON-2', X_FINAL, 'REASON-3', 'REASON-4'];
    const final = [...BODY, TIP_FULL, ...EXTRA];
    const all = await run([
      {
        history: ['HIST-0'],
        live: ['REASON-0', 'REASON-1', 'REASON-2', X_STREAMED, 'REASON-3', 'REASON-4', ...BODY, TIP_PARTIAL],
        streaming: true,
      },
      { history: ['HIST-0', REWRITTEN.join('\n')], live: [...BODY, TIP_FULL, ...EXTRA], streaming: true },
      {
        history: ['HIST-0', REWRITTEN.join('\n'), final.join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    for (const line of [...REWRITTEN, ...final]) {
      expect(all.filter((l) => l === line).length).toBeGreaterThanOrEqual(1);
    }
  });

  it('a newly streamed row extending a transient below-tip row commits exactly once', async () => {
    // The tip is byte-identical across the flush; a new row streams in at
    // the slot of a transient status row and happens to extend its bytes.
    // Growth and replacement are the same byte transformation here, so the
    // stale short form must not commit alongside the newcomer; the status
    // row itself is transient (never finalized) and follows chrome
    // semantics. Durable content stays loss-free throughout.
    const TIP_STEADY = 'ANSWER-TIP finished streaming already';
    const STATUS = 'Thinking';
    const NEWCOMER = 'Thinking about the config file';
    const final = [...BODY, TIP_STEADY, NEWCOMER];
    const all = await run([
      { history: ['HIST-0'], live: [...REASON, ...BODY, TIP_STEADY, STATUS], streaming: true },
      { history: ['HIST-0', REASON.join('\n')], live: [...BODY, TIP_STEADY, NEWCOMER, STATUS], streaming: true },
      {
        history: ['HIST-0', REASON.join('\n'), final.join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    const bad: string[] = [];
    for (const line of [...REASON, TIP_STEADY, NEWCOMER]) {
      const n = all.filter((l) => l === line).length;
      if (n !== 1) bad.push(`${line}=${n}`);
    }
    expect(bad).toEqual([]);
    // Body rows may hit the pre-existing paint-lag fallback (bounded
    // duplication) when the inserted row grows the frame; they must
    // never be lost.
    for (const line of BODY) {
      expect(all.filter((l) => l === line).length).toBeGreaterThanOrEqual(1);
    }
  });

  it('an empty captured tip cannot consume the prefix allowance or lose content', async () => {
    // The tip was an empty row at capture (paragraph break mid-stream).
    // Empty prefixes every row, so the allowance must reject it; the cost
    // is bounded duplication, never loss.
    const final = [...BODY, TIP_FULL, ...EXTRA];
    const all = await run([
      { history: ['HIST-0'], live: [...REASON, ...BODY, ''], streaming: true },
      { history: ['HIST-0', REASON.join('\n')], live: [...BODY, TIP_FULL, ...EXTRA], streaming: true },
      {
        history: ['HIST-0', REASON.join('\n'), final.join('\n')],
        live: [],
        streaming: false,
      },
    ]);

    for (const line of [...REASON, ...final]) {
      expect(all.filter((l) => l === line).length).toBeGreaterThanOrEqual(1);
    }
  });
});
