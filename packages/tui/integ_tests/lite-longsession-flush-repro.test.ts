import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Long-session repro for the lite flush / newline-wave reports. To hit the
 * real bug the driver needs a SMALL viewport (content overflows so the
 * live→static flush + writeStaticLines overflow-erase path fire) and STREAMED
 * delta chunks (the live region grows a multi-row block before flushing to
 * <Static> — where the "wave of newlines after a response" was emitted). The
 * blank-run scan is restricted to the content region above the prompt divider
 * so empty viewport rows below the prompt aren't miscounted as a wave.
 */
describe('lite long-session flush/newline repro', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /** Index of the prompt divider row (the long ─ rule above the status bar). */
  function dividerIndex(lines: string[]): number {
    for (let i = lines.length - 1; i >= 0; i--) {
      const t = lines[i]!.trim();
      if (t.length > 20 && /^[─-]+$/.test(t)) return i;
    }
    return lines.length;
  }

  /**
   * Longest run of blank rows that is INTERNAL to the content region (has a
   * non-blank row both before and after it). Trailing padding and the
   * empty viewport below the prompt are excluded.
   */
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

  it('streamed turns in a small viewport: no wave / dup / drop', async () => {
    testCase = await launchLiteInteg('lite-longsession-flush-repro', {
      terminal: { width: 90, height: 24 },
      timeout: 90000,
    });

    const TURNS = 12;
    let worstRun = 0;
    let worstTurn = -1;
    const findings: string[] = [];
    const respMarkers: string[] = [];

    for (let i = 1; i <= TURNS; i++) {
      const resp = `ZR${i}ZR`; // delimited response-only marker (no substring collisions)
      const shape = i % 3;

      if (shape !== 2) {
        await testCase.mockSessionUpdate({
          type: AgentEventType.Thought,
          id: `th-${i}`,
          content: {
            type: ContentType.Text,
            text: `Reasoning step for turn ${i}.\nSecond reasoning line.`,
          },
        });
      }
      if (shape === 1) {
        await testCase.mockSessionUpdate({
          type: AgentEventType.ToolCall,
          id: `tool-${i}`,
          name: 'Shell',
          kind: 'shell',
          args: { command: `run step ${i}` },
        });
        await testCase.mockSessionUpdate({
          type: AgentEventType.ToolCallFinished,
          id: `tool-${i}`,
          result: { status: 'success', output: `step ${i} ok` },
        });
      }

      // Stream the response as delta chunks (same id) — exercises the live
      // region's growing block, then the flush to <Static> on completeTurn.
      const chunks =
        shape === 2
          ? [
              `## Heading ${i} ${resp}\n\n`,
              `First paragraph of the answer with some length to it.\n\n`,
              `Second paragraph continues the explanation here.\n\n`,
              '```ts\n',
              `const v${i} = ${i};\n`,
              '```\n',
            ]
          : [`Answer ${resp}: `, `did the thing `, `for turn ${i}.`];
      const cid = `c-${i}`;
      for (const ch of chunks) {
        await testCase.mockSessionUpdate({
          type: AgentEventType.Content,
          id: cid,
          content: { type: ContentType.Text, text: ch },
        });
        await testCase.sleepMs(20);
      }
      respMarkers.push(resp);

      await testCase.typeAndSubmit(`question number ${i}`);
      await testCase.completeTurn();
      await testCase.sleepMs(160);

      const snap = testCase.getSnapshot();
      const { run, at } = maxInternalBlankRun(snap);
      if (run > worstRun) {
        worstRun = run;
        worstTurn = i;
      }
      if (run > 2) {
        findings.push(
          `turn ${i} (shape ${shape}): ${run} internal blanks @${at}`
        );
        console.log(
          `\n[WAVE?] turn ${i} shape ${shape}: ${run} internal blanks @${at}\n` +
            testCase.getSnapshotFormatted()
        );
      }
    }

    // Duplication across full scrollback (getSnapshot includes scrollback).
    const finalSnap = testCase.getSnapshot();
    for (const m of respMarkers) {
      const count = finalSnap.filter((l) => l.includes(m)).length;
      if (count > 1) findings.push(`DUP: ${m} x${count}`);
    }

    console.log(
      `[repro] turns=${TURNS} worstInternalBlankRun=${worstRun}@turn${worstTurn} findings=${findings.length}`
    );
    if (findings.length) {
      console.log('[findings]\n' + findings.join('\n'));
    }

    await exitLiteInteg(testCase);

    // A legitimate section break is a single blank row; 2 can happen around
    // headed/structural blocks. >2 internal consecutive blanks = a wave.
    expect(worstRun).toBeLessThanOrEqual(2);
    expect(findings).toEqual([]);
  }, 120000);
  it('tall streamed response (overflows viewport) flushes without a blank wave', async () => {
    testCase = await launchLiteInteg('lite-longsession-tall-stream', {
      terminal: { width: 90, height: 20 },
      timeout: 60000,
    });

    // Stream a response far taller than the 20-row viewport, one line per
    // chunk. While streaming, the live region's physical height exceeds the
    // terminal — exercising writeStaticLines' overflow-erase branch
    // (liveRows > terminal.rows → writes \x1b[3J + erase). The live→static
    // flush on completeTurn is the moment a "wave of newlines after a
    // response" was reported. Drive it twice so a second turn lands on top of
    // a scrollback that already overflowed once.
    for (let turn = 1; turn <= 2; turn++) {
      const cid = `tall-${turn}`;
      for (let line = 1; line <= 40; line++) {
        await testCase.mockSessionUpdate({
          type: AgentEventType.Content,
          id: cid,
          content: {
            type: ContentType.Text,
            text: `Line ${line} of tall response turn ${turn} with enough text to be a real row.\n`,
          },
        });
        await testCase.sleepMs(8);
      }
      await testCase.mockSessionUpdate({
        type: AgentEventType.Content,
        id: cid,
        content: { type: ContentType.Text, text: `END_TURN_${turn}_ZZ` },
      });
      await testCase.typeAndSubmit(`tall question ${turn}`);
      await testCase.completeTurn();
      await testCase.sleepMs(250);

      const snap = testCase.getSnapshot();
      const { run, at } = maxInternalBlankRun(snap);

      console.log(
        `[tall-stream] turn ${turn} maxInternalBlankRun=${run}@${at}`
      );
      if (run > 2) {
        console.log(testCase.getSnapshotFormatted());
      }
      expect(run).toBeLessThanOrEqual(2);
    }

    await exitLiteInteg(testCase);
  }, 90000);
});
