/**
 * E2E test: Timeout-triggered cancel clears backend state for subsequent prompts.
 *
 * Simulates a slow backend by not pushing any mock response for the first prompt,
 * causing the TUI's initial-response timeout to fire. Verifies that:
 * 1. The timeout sends session/cancel to the backend
 * 2. The backend clears pending_prompt_response
 * 3. A subsequent prompt is accepted without "Prompt already in progress" error
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Timeout cancel recovery', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('recovers from timeout and accepts a new prompt', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('timeout-cancel-recovery')
      .withEnv({
        KIRO_INITIAL_RESPONSE_TIMEOUT_MS: '3000',
      })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // 1. Send first prompt — no mock response, so the backend blocks
    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition((s) => s.isProcessing === true, 5000);

    // 2. Wait for timeout to fire and clear isProcessing
    await testCase.waitForStoreCondition((s) => !s.isProcessing, 15000);

    // 3. Send second prompt — no mock events pushed, nothing can
    //    accidentally complete the first request.
    await testCase.sendKeys('second message');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // 4. Wait for the round-trip to the backend
    await testCase.sleepMs(2000);

    // 5. THE KEY ASSERTION: "Prompt already in progress" must NOT appear.
    //    Without the cancel fix, ACP rejects the second prompt and the
    //    TUI renders this error visibly in the terminal.
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('Prompt already in progress');
  }, 60000);
});
