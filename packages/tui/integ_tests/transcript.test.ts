/**
 * Integration tests for /transcript command.
 *
 * Tests cover:
 * - Shows error alert when no conversation messages exist
 * - Opens pager with conversation content when messages exist
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

async function typeSlowly(tc: TestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

describe('/transcript', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('shows error when no messages exist', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-empty')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, '/transcript');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const state = await testCase.getStore();
    expect(state.transientAlert?.message).toBe('No conversation to display');
    expect(state.transientAlert?.status).toBe('error');
  }, 30000);

  it('opens less with conversation markdown and quits with q', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-with-messages')
      .withEnv({ PAGER: 'less' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Send a user message so the conversation is non-empty
    await typeSlowly(testCase, 'hello from the test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Open transcript — less takes over the PTY
    await typeSlowly(testCase, '/transcript');
    await testCase.pressEnter();

    // Wait for less to render the serialized markdown (longer timeout for CI)
    await testCase.waitForVisibleText('## User', 10000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('## User');
    expect(snapshot).toContain('hello from the test');

    // Quit less
    await testCase.sendKeys('q');

    // Should return to the normal TUI prompt
    await testCase.waitForVisibleText('ask a question', 10000);
  }, 30000);
});
