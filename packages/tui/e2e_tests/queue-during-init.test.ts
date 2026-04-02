/**
 * E2E sanity check: user input typed before session initialization completes
 * should be queued and sent automatically once the session is ready,
 * rather than throwing "cannot send prompt without an active session".
 *
 * NOTE: This is a best-effort smoke test. On a fast machine, initialization
 * may complete before the keystrokes arrive, so the queuing path isn't
 * guaranteed to be exercised. The unit tests in message-queue.test.ts are
 * the source of truth for the queuing-during-init behavior.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Queue input during initialization', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('queues early input and sends it after session is ready', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('queue-during-init')
      .launch();

    // Type and submit immediately — don't wait for "ask a question"
    // The session may not be ready yet, so this should be queued.
    await testCase.sendKeys('hello from early input');
    await testCase.sleepMs(50);
    await testCase.sendKeys('\r');

    // Now wait for init to complete
    await testCase.waitForText('ask a question', 15000);

    // Wait for session to be established
    const sessionId = await testCase.getSessionId(10000);
    expect(sessionId).toBeTruthy();

    // Push a mock response for the queued prompt
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Got your early message!' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    // The queued message should have been sent and the response should appear
    await testCase.waitForText('Got your early message!', 15000);

    // Verify the user message appears in the conversation
    const store = await testCase.getStore();
    expect(store.messages.some((m) => m.content.includes('hello from early input'))).toBe(true);
    expect(store.messages.some((m) => m.content.includes('Got your early message!'))).toBe(true);

    // No error should have occurred
    expect(store.agentError).toBeNull();

    await testCase.sendKeys([0x03, 0x03]);
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 60000);
});
