/**
 * Integration test for zero-width Unicode character input rendering.
 *
 * Reproduces the "pyramid" / duplicated-lines bug where pasting text
 * containing zero-width characters (e.g. U+200E left-to-right mark)
 * causes the input box to render duplicate lines when typing.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

async function exitCleanly(tc: TestCase) {
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.expectExit();
}

describe('Zero-width character input', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('does not produce duplicate lines when typing after zero-width chars', async () => {
    testCase = await TestCase.builder()
      .withTestName('zero-width-no-pyramid')
      .withTerminal({ width: 60, height: 20 })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Send text containing U+200E (left-to-right mark) — the exact repro string.
    // This is a zero-width char: string length 1, visual width 0.
    const textWithZeroWidth = '\u200Ecrates/agent/src/agent/agent_config/load.rs';
    await testCase.sendKeys(textWithZeroWidth);
    await testCase.sleepMs(300);

    // Type a few more characters (the repro says pressing '?' triggers it)
    await testCase.sendKeys('???');
    await testCase.sleepMs(300);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');

    // The text should appear exactly once — no pyramid/duplication
    const needle = 'agent_config/load.rs???';
    const occurrences = snapshot.filter((line) =>
      line.includes('agent_config/load.rs')
    ).length;
    expect(occurrences).toBe(1);

    // The full text should be present
    expect(screenText).toContain(needle);

    await exitCleanly(testCase);
  }, 20000);
});
