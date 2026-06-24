/**
 * E2E test for P409238957: Cancel clears isProcessing to prevent state desync.
 *
 * Verifies that cancelling a prompt (Ctrl+C) correctly resets isProcessing,
 * allowing subsequent prompts to be sent instead of getting stuck in
 * "Prompt already in progress".
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Cancel state recovery (P409238957)', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('clears isProcessing after cancel, allowing a new prompt', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('cancel-state-recovery')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push a streaming response that never closes — keeps the turn open
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Working on it...' },
        },
      },
    ]);
    // Deliberately NO null push — stream stays open so isProcessing stays true

    // Send prompt
    await testCase.sendKeys('first prompt');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for isProcessing to become true via store (not PTY text)
    await testCase.waitForStoreCondition((s) => s.isProcessing === true, 10000);

    // Cancel with Ctrl+C
    await testCase.pressCtrlC();
    await testCase.sleepMs(500);

    // The fix: isProcessing must clear after cancel
    const afterCancel = await testCase.waitForStoreCondition(
      (s) => !s.isProcessing,
      5000
    );
    expect(afterCancel.isProcessing).toBe(false);

    // Verify we can send a second prompt — push a complete response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Second response OK' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('second prompt');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Second prompt was accepted — verify via store that new messages arrived.
    // This proves we're not stuck in "Prompt already in progress".
    const afterSecond = await testCase.waitForStoreCondition(
      (s) => s.messages.length > afterCancel.messages.length,
      10000
    );
    expect(afterSecond.messages.length).toBeGreaterThan(
      afterCancel.messages.length
    );
  }, 60000);
});
