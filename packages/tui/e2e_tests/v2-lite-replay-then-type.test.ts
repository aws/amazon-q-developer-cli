/**
 * Repro for the v2+lite field report: after a session replay
 * (`runSessionLoad` — the /rewind and /chat <id> path), newly typed input
 * rendered AT THE OLD USER ROW's position, replacing it on screen, and only
 * corrected once the response streamed in. Store/persistence stay correct
 * (reload shows clean history) — paint-position only.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

const LONG_BODY = Array.from(
  { length: 10 },
  (_, i) =>
    `HISTPARA-${i} detailed reply paragraph with wrapped filler content that exceeds the terminal width for row accounting`
).join('\n\n');

describe('v2 lite: session replay then new input', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('typed input lands below replayed history and never replaces the old user row', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 80, height: 14 })
      .withTestName('v2-lite-replay-then-type')
      .withLite()
      .withGlobalSettings({ 'chat.preserveScrollback': true })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    // Two turns so the fork point replays a multi-turn history.
    const turns: Array<[string, string]> = [
      [
        'FIRSTQUESTION about the service',
        'EARLY reply body\n\nEARLY second paragraph',
      ],
      ['OLDQUESTION why ninety seconds', LONG_BODY],
    ];
    for (const [prompt, reply] of turns) {
      await testCase.pushSendMessageResponse([
        {
          kind: 'event',
          data: { kind: 'AssistantResponseEvent', data: { content: reply } },
        },
      ]);
      await testCase.pushSendMessageResponse(null);
      for (const c of prompt) {
        await testCase.sendKeys(c);
      }
      await testCase.pressEnter();
      await testCase.sleepMs(900);
    }
    await testCase.waitForText('HISTPARA-9', 10000);

    // Fork from the LAST prompt: replays the full history through
    // runSessionLoad into a fresh lite <Static> (clear-token reset).
    for (const c of '/rewind') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(1500);
    await testCase.waitForText('Fork from', 10000);
    // Newest-first picker: first row = OLDQUESTION turn.
    await testCase.pressEnter();
    await testCase.sleepMs(2000);
    await testCase.waitForText('HISTPARA-9', 15000);
    await testCase.sleepMs(800);

    // The replay must contain its PROMPT rows, not just reply bodies. The
    // live-echo dedupe consumed a replayed prompt whose text matched a
    // recently typed message, replaying the turn as an orphaned response
    // ("the replay renders responses without prompts").
    const afterReplay = testCase.getSnapshot();
    const countRows = (needle: string) =>
      afterReplay.filter((l) => l.includes(needle)).length;
    // Original session paint + replayed copy = 2 each.
    expect(countRows('You: FIRSTQUESTION')).toBe(2);
    expect(countRows('You: OLDQUESTION')).toBe(2);

    // Type new input with the response held back; inspect the gap window.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'NEWRESPONSE ack' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    for (const c of 'BRANDNEWINPUT sup') {
      await testCase.sendKeys(c);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(600);

    const during = testCase.getSnapshot();
    const oldQDuring = during.filter((l) => l.includes('OLDQUESTION')).length;
    const supDuring = during.filter((l) => l.includes('BRANDNEWINPUT')).length;

    await testCase.waitForText('NEWRESPONSE', 10000);
    await testCase.sleepMs(600);

    const all = testCase.getSnapshot();
    const idxOf = (needle: string) => all.findIndex((l) => l.includes(needle));

    // Old user row must survive the submit window.
    expect(oldQDuring).toBeGreaterThanOrEqual(1);
    // New input exists during the window and lands BELOW the replayed history.
    expect(supDuring).toBeGreaterThanOrEqual(1);
    expect(idxOf('BRANDNEWINPUT')).toBeGreaterThan(idxOf('HISTPARA-9'));
  }, 90000);
});
