/**
 * E2E test: chat.disableWrap setting disables the StatusBar chrome
 * and renders messages with overflow wrapping.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('chat.disableWrap setting', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('renders without StatusBar chrome and with spacing between prompt and response', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('disable-wrap-no-statusbar')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.disableWrap': true })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello from the assistant' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Hello from the assistant', 10000);

    const snapshot = testCase.getSnapshot();

    // Response text should start without the StatusBar's 2-char indent (bar + margin)
    const responseLine = snapshot.find(line => line.includes('Hello from the assistant'));
    expect(responseLine).toBeDefined();
    const textStart = responseLine!.indexOf('Hello from the assistant');
    expect(textStart).toBeLessThanOrEqual(1);

    // There should be at least one blank line between user prompt and response
    const userLineIdx = snapshot.findIndex(line => line.includes('hello'));
    const responseLineIdx = snapshot.findIndex(line => line.includes('Hello from the assistant'));
    expect(responseLineIdx - userLineIdx).toBeGreaterThanOrEqual(2);

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  it('renders long lines with overflow wrapping (no word-wrap breaks)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('disable-wrap-overflow')
      .withTerminal({ width: 40, height: 24 })
      .withGlobalSettings({ 'chat.disableWrap': true })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    // Send a message longer than terminal width (40 cols).
    // With overflow, the terminal soft-wraps at exactly column 40.
    const longText = 'ABCDEFGHIJ'.repeat(6); // 60 chars, no spaces
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: longText } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('ABCDEFGHIJ', 10000);

    // The 60-char string should soft-wrap across 2 rows at the terminal boundary.
    const snapshot = testCase.getSnapshot();
    const firstRow = snapshot.find(line => line.includes('ABCDEFGHIJ'.repeat(4)));
    const secondRow = snapshot.find(line =>
      line.includes('ABCDEFGHIJ'.repeat(2)) && !line.includes('ABCDEFGHIJ'.repeat(3))
    );
    expect(firstRow).toBeDefined();
    expect(secondRow).toBeDefined();

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});
