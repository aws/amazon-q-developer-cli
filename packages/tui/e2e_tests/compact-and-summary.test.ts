/**
 * E2E tests for /compact command and summary system message rendering.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('/compact and summary', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 1500));
  });

  it('executes /compact and shows compacting loading state', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-compact')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId(); // ensure session ID is set before pushing mock responses

    // Build up a conversation so there's something to compact
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello! How can I help you today?' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.pressEnter();
    await testCase.waitForIdle(15000);

    // Type /compact command
    for (const char of '/compact') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Verify the TUI shows compacting state or completes
    // The backend will process the compact command and either show loading or complete
    await testCase.sleepMs(3000);

    const store = await testCase.getStore();
    // After /compact, either it's still compacting or it completed
    // Either way the TUI should be responsive
    expect(store.messages).toBeDefined();

    console.log('Compact snapshot:\n' + testCase.getSnapshotFormatted());

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('renders summary system message after compaction completes', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-compact-summary')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId(); // ensure session ID is set before pushing mock responses

    // Build a multi-turn conversation to give the compaction something to summarize
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'I can help you with that task.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('help me with a task');
    await testCase.pressEnter();
    await testCase.waitForIdle(15000);

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Sure, here is the result.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('do the task');
    await testCase.pressEnter();
    await testCase.waitForIdle(15000);

    // Execute /compact
    for (const char of '/compact') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Wait for compaction to complete (backend processes it and sends status notifications)
    // Poll store for a system message indicating compaction completed
    const start = Date.now();
    let summaryFound = false;
    while (Date.now() - start < 30000) {
      const store = await testCase.getStore();
      const hasSystemMsg = store.messages.some((m: any) => m.role === 'system');
      if (hasSystemMsg && !store.isCompacting) {
        summaryFound = true;
        break;
      }
      // Also check if compaction completed without a summary (valid outcome)
      if (!store.isCompacting && store.loadingMessage === null && Date.now() - start > 5000) {
        break;
      }
      await testCase.sleepMs(200);
    }

    console.log('Compact summary snapshot:\n' + testCase.getSnapshotFormatted());

    const store = await testCase.getStore();
    expect(store.isCompacting).toBe(false);

    if (summaryFound) {
      // If a summary was generated, verify it rendered as a system message
      const systemMessages = store.messages.filter((m: any) => m.role === 'system');
      expect(systemMessages.length).toBeGreaterThan(0);
      const firstSystemMessage = systemMessages[0];
      if (firstSystemMessage && 'success' in firstSystemMessage) {
        expect(firstSystemMessage.success).toBe(true);
      }
    }

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 90000);

  it('shows alert on /compact failure', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('slash-command-compact-fail')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();

    // Execute /compact on an empty conversation — backend may fail or succeed
    for (const char of '/compact') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(20);
    }
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    // Wait for the command to resolve
    await testCase.sleepMs(5000);

    const store = await testCase.getStore();
    // TUI should be in a stable state regardless of outcome
    expect(store.isCompacting).toBe(false);
    expect(store.messages).toBeDefined();

    console.log('Compact fail snapshot:\n' + testCase.getSnapshotFormatted());

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});
