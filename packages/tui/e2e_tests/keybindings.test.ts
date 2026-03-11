/**
 * E2E tests for readline/emacs keybindings in the prompt input.
 *
 * These tests verify that control key sequences are correctly wired
 * through Ink's input parsing to the PromptInput handler. Unit tests
 * cover the editing functions in isolation; these tests verify the
 * full keypress → terminal render pipeline in a real PTY.
 *
 * Important: control bytes must be sent individually with delays between
 * them, otherwise the PTY may batch them into a single chunk and Ink's
 * parseKeypress will only process the first byte.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

// Control key bytes
const CTRL_A = '\x01';
const CTRL_D = '\x04';
const CTRL_E = '\x05';
const CTRL_F = '\x06';
const CTRL_K = '\x0b';
const CTRL_U = '\x15';
const CTRL_W = '\x17';
const CTRL_T = '\x14';

/**
 * Exit cleanly: clear any remaining input with Ctrl+U first (so Ctrl+C
 * doesn't just clear text), then send double Ctrl+C as a single chunk
 * (the pattern all other e2e tests use).
 */
async function exitCleanly(tc: E2ETestCase) {
  // Clear any remaining input: Ctrl+A (beginning) + Ctrl+K (kill to end)
  await tc.sendKeys(CTRL_A);
  await tc.sleepMs(50);
  await tc.sendKeys(CTRL_K);
  await tc.sleepMs(100);
  // Double Ctrl+C as single chunk — matches pressCtrlCTwice()
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}

/** Send a control key with a small delay after for processing */
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

  it('Ctrl+D deletes character under cursor', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-d')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    // Move to beginning, then delete first char
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_D);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('ello');
    expect(screenText).not.toContain('hello');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+A moves to beginning, Ctrl+E moves to end', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-a-e')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('abcdef');
    await testCase.sleepMs(200);

    // Ctrl+A to beginning, Ctrl+D to delete first char
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_D);
    await testCase.sleepMs(200);

    let snapshot = testCase.getSnapshot();
    let screenText = snapshot.join('\n');
    expect(screenText).toContain('bcdef');

    // Ctrl+E to end, type 'X'
    await sendCtrl(testCase, CTRL_E);
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    snapshot = testCase.getSnapshot();
    screenText = snapshot.join('\n');
    expect(screenText).toContain('bcdefX');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+K kills to end of line', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-k')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Move to beginning, forward 5 chars (after "hello"), then kill to end
    await sendCtrl(testCase, CTRL_A);
    for (let i = 0; i < 5; i++) {
      await sendCtrl(testCase, CTRL_F);
    }
    await sendCtrl(testCase, CTRL_K);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('hello');
    expect(screenText).not.toContain('world');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+U kills to beginning of line', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-u')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Move to beginning, forward 6 chars (after "hello "), then kill to beginning
    await sendCtrl(testCase, CTRL_A);
    for (let i = 0; i < 6; i++) {
      await sendCtrl(testCase, CTRL_F);
    }
    await sendCtrl(testCase, CTRL_U);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('world');
    expect(screenText).not.toContain('hello');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+W deletes word backward', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-w')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    await sendCtrl(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('hello');
    expect(screenText).not.toContain('world');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+T transposes characters', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-t')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.sendKeys('ab');
    await testCase.sleepMs(200);

    await sendCtrl(testCase, CTRL_T);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('ba');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+D does not interrupt agent processing, and can exit the app', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('keybind-ctrl-d-no-cancel')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Queue a response but don't close the stream — agent stays in processing state
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'done' } } },
    ], { silent: true });

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
