import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Long-session repro for the lite flush / newline-wave reports. The small
 * viewport + streamed delta chunks are load-bearing: they force the
 * live→static flush + writeStaticLines overflow-erase path where the "wave of
 * newlines after a response" was emitted. The blank-run scan only covers the
 * content region above the prompt divider (empty rows below it aren't a wave).
 */
describe('lite long-session flush/newline repro', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  /** Index of the prompt divider row (the long ─ rule above the status bar). */
  function dividerIndex(lines: string[]): number {
    for (let i = lines.length - 1; i >= 0; i--) {
      const t = lines[i]!.trim();
      if (t.length > 20 && /^[─-]+$/.test(t)) return i;
    }
    return lines.length;
  }

  /** Longest blank-row run INTERNAL to the content region (excludes trailing padding / below-prompt rows). */
  function maxInternalBlankRun(lines: string[]): { run: number; at: number } {
    const end = dividerIndex(lines);
    const content = lines.slice(0, end);
    // last non-blank index in content region
    let last = -1;
    for (let i = content.length - 1; i >= 0; i--) {
      if (content[i]!.trim() !== '') {
        last = i;
        break;
      }
    }
    let run = 0;
    let best = 0;
    let bestAt = -1;
    for (let i = 0; i <= last; i++) {
      if (content[i]!.trim() === '') {
        run++;
        if (run > best) {
          best = run;
          bestAt = i - run + 1;
        }
      } else {
        run = 0;
      }
    }
    return { run: best, at: bestAt };
  }

  // Chunk generators. mixed: 3 turn shapes (thought, thought+tool, markdown);
  // tall: a 40-line response far taller than the viewport.
  function mixedChunks(i: number, resp: string): string[] {
    return i % 3 === 2
      ? [
          `## Heading ${i} ${resp}\n\n`,
          `First paragraph of the answer with some length to it.\n\n`,
          `Second paragraph continues the explanation here.\n\n`,
          '```ts\n',
          `const v${i} = ${i};\n`,
          '```\n',
        ]
      : [`Answer ${resp}: `, `did the thing `, `for turn ${i}.`];
  }

  async function seedMixedPreamble(tc: TestCase, i: number): Promise<void> {
    const shape = i % 3;
    if (shape !== 2) {
      await tc.mockSessionUpdate({
        type: AgentEventType.Thought,
        id: `th-${i}`,
        content: {
          type: ContentType.Text,
          text: `Reasoning step for turn ${i}.\nSecond reasoning line.`,
        },
      });
    }
    if (shape === 1) {
      await tc.mockSessionUpdate({
        type: AgentEventType.ToolCall,
        id: `tool-${i}`,
        name: 'Shell',
        kind: 'shell',
        args: { command: `run step ${i}` },
      });
      await tc.mockSessionUpdate({
        type: AgentEventType.ToolCallFinished,
        id: `tool-${i}`,
        result: { status: 'success', output: `step ${i} ok` },
      });
    }
  }

  function tallChunks(turn: number): string[] {
    return [
      ...Array.from(
        { length: 40 },
        (_, i) =>
          `Line ${i + 1} of tall response turn ${turn} with enough text to be a real row.\n`
      ),
      `END_TURN_${turn}_ZZ`,
    ];
  }

  /**
   * Drive `turns` streamed turns and assert the worst INTERNAL blank run stays
   * <= 2. A legitimate section break is 1 blank row; 2 can happen around headed
   * blocks; >2 consecutive internal blanks is the reported "newline wave".
   * trackDup additionally asserts no response marker is duplicated in scrollback.
   */
  async function runBlankWaveScenario(opts: {
    testName: string;
    terminal: { width: number; height: number };
    launchTimeout: number;
    turns: number;
    chunkDelayMs: number;
    settleMs: number;
    seedPreamble?: (tc: TestCase, i: number) => Promise<void>;
    chunksFor: (i: number, resp: string) => string[];
    trackDup?: boolean;
  }): Promise<void> {
    const tc = await launchLiteInteg(opts.testName, {
      terminal: opts.terminal,
      timeout: opts.launchTimeout,
    });
    testCase = tc;

    let worstRun = 0;
    const respMarkers: string[] = [];

    for (let i = 1; i <= opts.turns; i++) {
      const resp = `ZR${i}ZR`; // delimited marker (no substring collisions)
      await opts.seedPreamble?.(tc, i);

      // Stream the response as delta chunks (same id) — grows the live region,
      // then flushes to <Static> on completeTurn.
      const cid = `c-${i}`;
      for (const ch of opts.chunksFor(i, resp)) {
        await tc.mockSessionUpdate({
          type: AgentEventType.Content,
          id: cid,
          content: { type: ContentType.Text, text: ch },
        });
        await tc.sleepMs(opts.chunkDelayMs);
      }
      respMarkers.push(resp);

      await tc.typeAndSubmit(`question number ${i}`);
      await tc.completeTurn();
      await tc.sleepMs(opts.settleMs);

      const { run } = maxInternalBlankRun(tc.getSnapshot());
      if (run > worstRun) worstRun = run;
    }

    expect(worstRun).toBeLessThanOrEqual(2);

    if (opts.trackDup) {
      const finalSnap = tc.getSnapshot();
      for (const m of respMarkers) {
        expect(finalSnap.filter((l) => l.includes(m)).length).toBe(1);
      }
    }

    await exitLiteInteg(tc);
  }

  it('streamed turns in a small viewport: no wave / dup / drop', async () => {
    await runBlankWaveScenario({
      testName: 'lite-longsession-flush-repro',
      terminal: { width: 90, height: 24 },
      launchTimeout: 90000,
      turns: 12,
      chunkDelayMs: 20,
      settleMs: 160,
      seedPreamble: seedMixedPreamble,
      chunksFor: mixedChunks,
      trackDup: true,
    });
  }, 120000);

  // Tall response overflows the viewport while streaming, exercising
  // writeStaticLines' overflow-erase branch (liveRows > terminal.rows → \x1b[3J
  // + erase). Driven twice so turn 2 lands on a scrollback that already
  // overflowed once.
  it('tall streamed response (overflows viewport) flushes without a blank wave', async () => {
    await runBlankWaveScenario({
      testName: 'lite-longsession-tall-stream',
      terminal: { width: 90, height: 20 },
      launchTimeout: 60000,
      turns: 2,
      chunkDelayMs: 8,
      settleMs: 250,
      chunksFor: (i) => tallChunks(i),
    });
  }, 90000);
});
