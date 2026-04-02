/**
 * E2E tests for /copy command — copies last assistant response to clipboard.
 */

import { afterEach, describe, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('/copy command', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('copies last assistant response and shows success alert', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('copy-command-success')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // Push a mock assistant response
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Here is a multi-line response\nwith several lines\nfor testing copy.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send a prompt to trigger the response
    await testCase.sendKeys('test prompt');
    await testCase.sleepMs(100);
    await testCase.sendKeys('\r');

    // Wait for the response to render and processing to finish
    await testCase.waitForText('multi-line response', 10000);
    await testCase.waitForIdle(15000);
    await testCase.sleepMs(500);

    // Type /copy and execute
    const cmd = '/copy';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');

    // Verify success alert appears
    await testCase.waitForText('Copied to clipboard', 10000);

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 45000);

  it('shows error when no response exists', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('copy-command-no-response')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Type /copy without any prior conversation
    const cmd = '/copy';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');

    // Verify error alert appears
    await testCase.waitForText('No response to copy', 10000);

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('appears in slash command autocomplete', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('copy-command-autocomplete')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Type /cop to trigger autocomplete
    await testCase.sendKeys('/cop');
    await testCase.sleepMs(500);

    // Verify /copy appears in autocomplete
    await testCase.waitForText('copy', 5000);
    await testCase.waitForText('Copy last response to clipboard', 5000);

    // Exit cleanly
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});
