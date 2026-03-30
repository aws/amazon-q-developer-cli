/**
 * Reproduction test for P404854010: Typing too quickly drops characters.
 *
 * When keypresses arrive faster than React can re-render, the PromptInput
 * handler reads stale segments/cursor from the closure and drops characters.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

async function typeAndCheck(
  tc: E2ETestCase,
  text: string,
  delayMs: number,
): Promise<string> {
  for (const char of text) {
    await tc.sendKeys(char);
    if (delayMs > 0) await tc.sleepMs(delayMs);
  }
  await tc.sleepMs(200);
  const store = await tc.getStore();
  return store.commandInputValue;
}

describe('Fast Typing (P404854010)', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('10ms between chars — no dropped characters', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('fast-type-10ms')
      .withTimeout(30000)
      .launch();
    await testCase.waitForText('ask a question', 10000);

    const text = 'the quick brown fox';
    const actual = await typeAndCheck(testCase, text, 10);
    console.log(`[10ms] expected="${text}" actual="${actual}"`);
    expect(actual).toBe(text);
  }, 60000);

  it('5ms between chars — no dropped characters', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('fast-type-5ms')
      .withTimeout(30000)
      .launch();
    await testCase.waitForText('ask a question', 10000);

    const text = 'fast slow fast again';
    const actual = await typeAndCheck(testCase, text, 5);
    console.log(`[5ms] expected="${text}" actual="${actual}"`);
    expect(actual).toBe(text);
  }, 60000);
});
