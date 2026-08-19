/**
 * Repro for the v2+lite field report: with `chat.preserveScrollback` on, a
 * response streamed taller than the viewport was re-emitted into scrollback
 * when it finalized into <Static> — the whole turn appeared twice, with the
 * stale footer chrome sandwiched between the copies. Runs the real Rust
 * binary (V2 engine) with streamed AssistantResponseEvent chunks so the
 * overflow happens live, exactly as in the field.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

const PARAS_PER_TURN = 14;

function turnBody(turn: number): string[] {
  return Array.from(
    { length: PARAS_PER_TURN },
    (_, i) => `MSGPARA-${turn}-${i} reply paragraph with steady filler text\n\n`
  );
}

describe('v2 lite: streamed overflow finalize is exactly-once', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('tall streamed turns land in scrollback exactly once each', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 80, height: 14 })
      .withTestName('v2-lite-stream-finalize-once')
      .withLite()
      .withGlobalSettings({ 'chat.preserveScrollback': true })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    // Two turns: turn 2's flush lands on a scrollback that already
    // overflowed once — the field session shape.
    for (const turn of [1, 2]) {
      for (const c of `question number ${turn}`) {
        await testCase.sendKeys(c);
      }
      await testCase.pressEnter();
      // Stream chunk-by-chunk (real frames between chunks) so the live
      // region overflows the 14-row viewport WHILE streaming; the finalize
      // then takes the overflow-flush path.
      for (const chunk of turnBody(turn)) {
        await testCase.pushSendMessageResponse(
          [
            {
              kind: 'event',
              data: {
                kind: 'AssistantResponseEvent',
                data: { content: chunk },
              },
            },
          ],
          { silent: true }
        );
        await testCase.sleepMs(40);
      }
      await testCase.pushSendMessageResponse(null);
      await testCase.waitForText(
        `MSGPARA-${turn}-${PARAS_PER_TURN - 1}`,
        10000
      );
      await testCase.sleepMs(800);
    }

    // Full buffer = scrollback + viewport. Every paragraph of BOTH turns
    // must appear exactly once — the bug re-emitted entire finalized turns.
    const all = testCase.getSnapshot();
    const bad: string[] = [];
    for (const turn of [1, 2]) {
      for (let i = 0; i < PARAS_PER_TURN; i++) {
        const marker = `MSGPARA-${turn}-${i} `;
        const n = all.filter((l) => l.includes(marker)).length;
        if (n !== 1) bad.push(`${marker.trim()}=${n}`);
      }
    }
    console.log(
      'duplicate/missing markers:',
      bad.length === 0 ? 'none' : bad.join(' ')
    );
    expect(bad).toEqual([]);

    // Ordering survives the repaint seam: turn 1 wholly above turn 2.
    const idxOf = (needle: string) => all.findIndex((l) => l.includes(needle));
    expect(idxOf(`MSGPARA-1-${PARAS_PER_TURN - 1}`)).toBeLessThan(
      idxOf('MSGPARA-2-0')
    );
  }, 120000);

  it('a growing overflow thinking row settles into scrollback exactly once', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 80, height: 14 })
      .withTestName('v2-lite-thinking-finalize-once')
      .withLite()
      .withGlobalSettings({
        'chat.preserveScrollback': true,
        'chat.verbosity.lite': { showThinkingContent: true },
      })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);
    for (const c of 'reason at length') await testCase.sendKeys(c);
    await testCase.pressEnter();

    const chunks = Array.from(
      { length: 18 },
      (_, i) =>
        `${i === 0 ? 'THINK-ONCE ' : ''}${i === 17 ? '\nTHINK-TAIL-ONCE ' : ''}reasoning sentence ${i} has enough steady filler to wrap the live block. `
    );
    for (const chunk of chunks.slice(0, -1)) {
      await testCase.pushSendMessageResponse(
        [
          {
            kind: 'event',
            data: { kind: 'ReasoningEvent', data: { text: chunk } },
          },
        ],
        { silent: true }
      );
      await testCase.sleepMs(40);
    }

    // Complete immediately after final growth to share one renderer window.
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'ReasoningEvent',
            data: { text: chunks[chunks.length - 1] },
          },
        },
      ],
      { silent: true }
    );
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(1000);

    const all = testCase.getSnapshot();
    const rendered = all.join('');
    for (const marker of ['THINK-ONCE', 'THINK-TAIL-ONCE']) {
      expect(rendered.split(marker).length - 1).toBe(1);
    }
  }, 120000);
});
