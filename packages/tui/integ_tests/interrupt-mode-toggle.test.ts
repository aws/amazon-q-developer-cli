/**
 * Integration tests for the follow-up mode toggle keybinding (Ctrl+S).
 *
 * Tests cover:
 * - Ctrl+S toggles activeInterruptMode from 'steering' to 'queuing'
 * - Ctrl+S toggles back from 'queuing' to 'steering'
 * - Toggle works both when idle and when input is present
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const CTRL_S = '\x13';

async function sendCtrl(tc: TestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(150);
}

describe('Interrupt mode toggle (Ctrl+S)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Ctrl+S toggles mode from steering to queuing', async () => {
    testCase = await TestCase.builder()
      .withTestName('follow-up-toggle-to-queuing')
      .withTimeout(15000)
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Verify initial mode is steering
    const initialStore = await testCase.getStore();
    expect(initialStore.activeInterruptMode).toBe('steer');

    // Press Ctrl+S to toggle
    await sendCtrl(testCase, CTRL_S);

    // Verify mode switched to queuing
    const afterToggle = await testCase.getStore();
    expect(afterToggle.activeInterruptMode).toBe('queue');
  }, 20000);

  it('Ctrl+S toggles mode back from queuing to steering', async () => {
    testCase = await TestCase.builder()
      .withTestName('follow-up-toggle-round-trip')
      .withTimeout(15000)
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Toggle to queuing
    await sendCtrl(testCase, CTRL_S);
    const afterFirst = await testCase.getStore();
    expect(afterFirst.activeInterruptMode).toBe('queue');

    // Toggle back to steering
    await sendCtrl(testCase, CTRL_S);
    const afterSecond = await testCase.getStore();
    expect(afterSecond.activeInterruptMode).toBe('steer');
  }, 20000);

  it('Ctrl+S works when there is text in the input', async () => {
    testCase = await TestCase.builder()
      .withTestName('follow-up-toggle-with-input')
      .withTimeout(15000)
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Type some text first
    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);

    // Toggle mode
    await sendCtrl(testCase, CTRL_S);

    // Verify mode switched
    const afterToggle = await testCase.getStore();
    expect(afterToggle.activeInterruptMode).toBe('queue');

    // Verify the input text is preserved (toggle shouldn't clear it)
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');
  }, 20000);
});
