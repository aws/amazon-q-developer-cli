/**
 * E2E tests for /chat slash command (session loading).
 *
 * Uses testCase.launchAcpHelper() to create a real session with history via a
 * separate ACP connection, then verifies the TUI can load and display it.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Chat Command', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('loads a previous session and displays its history', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('chat-command-load')
      .withGlobalAgentConfig('test-agent', {
        name: 'test-agent',
        description: 'Test agent for e2e',
        tools: ['@builtin'],
      })
      .launch();

    // Create a session with history via a separate ACP connection
    const acp = await testCase.launchAcpHelper();
    const sessionId = await acp.newSession();

    // Switch to custom agent
    await acp.setSessionMode(sessionId, 'test-agent');

    await acp.pushResponse(sessionId, [
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'The answer is 4.' } } },
    ]);
    await acp.pushResponse(sessionId, null);
    await acp.prompt(sessionId, 'What is 2+2?');
    await acp.terminateSession(sessionId);
    await acp.close();

    // Load the session via /chat in the TUI
    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    for (const char of '/chat') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(50);
    }
    await testCase.pressEnter();

    await testCase.waitForText('What is 2+2?', 10000);
    await testCase.pressEnter();

    await testCase.sleepMs(2000);

    try {
      await testCase.waitForText('What is 2+2?', 60000);
      await testCase.waitForText('The answer is 4.', 5000);
    } catch (e) {
      console.log('FAILED snapshot:\n' + testCase.getSnapshotFormatted());
      const store = await testCase.getStore();
      console.log('Store messages:', JSON.stringify(store.messages, null, 2));
      console.log('Store sessionId:', store.sessionId);
      throw e;
    }

    const store = await testCase.getStore();
    expect(store.messages.some((m) => m.content.includes('What is 2+2?'))).toBe(true);
    expect(store.messages.some((m) => m.content.includes('The answer is 4.'))).toBe(true);
    // Verify system delimiter message was added
    expect(store.messages.some((m) => m.role === 'system' && m.content.includes('Loaded session'))).toBe(true);
    // Verify session ID was updated
    expect(store.sessionId).toBe(sessionId);
    // Verify agent is set (persisted from session creation)
    expect(store.currentAgent).not.toBeNull();
    expect(store.currentAgent!.name).toBe('test-agent');
  }, 120000);
});
