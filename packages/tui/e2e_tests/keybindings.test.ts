/**
 * E2E tests for keybindings that require a real ACP session.
 *
 * Pure input editing keybindings (Ctrl+A/E/K/U/W/T/D-delete) are covered
 * by integration tests in integ_tests/multiline-input.test.ts. This file
 * only contains tests that exercise the full agent pipeline.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

const CTRL_A = '\x01';
const CTRL_D = '\x04';
const CTRL_K = '\x0b';

async function exitCleanly(tc: E2ETestCase) {
  await tc.sendKeys(CTRL_A);
  await tc.sleepMs(50);
  await tc.sendKeys(CTRL_K);
  await tc.sleepMs(100);
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}

async function sendCtrl(tc: E2ETestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(100);
}

describe('Keybindings', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Ctrl+D does not interrupt agent processing, and can exit the app', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-d-no-cancel')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Queue a response but don't close the stream — agent stays in processing state
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: { kind: 'AssistantResponseEvent', data: { content: 'done' } },
        },
      ],
      { silent: true }
    );

    // Send a prompt to start agent processing
    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the agent to enter processing state
    await testCase.waitForText('Thinking', 10000);

    // Send Ctrl+D while agent is processing — should NOT cancel or show exit hint
    await sendCtrl(testCase, CTRL_D);
    await testCase.sleepMs(300);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain('again to exit');
    // Agent should still be processing
    expect(snap).toContain('Thinking');

    // Close the stream so agent finishes
    await testCase.pushSendMessageResponse(null);
    await testCase.waitForIdle();

    // After idle, double Ctrl+D on empty input should exit
    await testCase.sleepMs(300);
    await sendCtrl(testCase, CTRL_D);
    await testCase.sleepMs(200);
    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Press Ctrl+C or Ctrl+D again to exit');

    // Exit cleanly via Ctrl+C (proven pattern)
    await exitCleanly(testCase);
  }, 30000);
});
