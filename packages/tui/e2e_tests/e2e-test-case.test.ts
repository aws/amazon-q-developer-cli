/**
 * Basic E2E test validating E2ETestCase can spawn CLI and establish IPC.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('E2ETestCase', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('spawns CLI and establishes agent IPC connection', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('basic-connectivity')
      .launch();

    // Wait for TUI to render
    await testCase.waitForText('ask a question', 10000);

    // Exit cleanly
    await testCase.sendKeys([0x03, 0x03]); // Double Ctrl+C
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  it('can inject mock response via IPC', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mock-response-injection')
      .launch();

    // Wait for TUI to render
    console.log('Waiting for TUI to render...');
    await testCase.waitForText('ask a question', 10000);
    
    // Wait for session to be initialized
    const sessionId = await testCase.getSessionId();
    console.log('Session ID:', sessionId);
    console.log('TUI rendered, pushing mock response...');

    // Push a mock response before sending a prompt
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello from mock!' } } },
    ]);
    console.log('Mock response pushed, signaling end...');
    // Signal end of response
    await testCase.pushSendMessageResponse(null);
    console.log('End signaled, sending keys...');

    // Send a prompt
    await testCase.sendKeys('hi');
    await testCase.sleepMs(100);
    await testCase.sendKeys('\r');
    console.log('Keys sent, waiting for response...');

    // Wait for the mock response to appear
    await testCase.waitForText('Hello from mock!', 10000);

    // Test getSnapshot - should return rendered terminal screen
    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());
    expect(snapshot.some(line => line.includes('Hello from mock!'))).toBe(true);

    // Exit cleanly
    await testCase.sendKeys([0x03, 0x03]); // Double Ctrl+C
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});
