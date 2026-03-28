/**
 * Integration tests for word movement (Alt+F forward-word, Alt+B backward-word).
 *
 * Verifies emacs semantics:
 * - Alt+F stops right after the last letter of the word
 * - Alt+B stops right before the first letter of the word
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const ALT_F = '\x1bf';
const ALT_B = '\x1bb';
const CTRL_A = '\x01';

async function send(tc: TestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(100);
}

async function exitCleanly(tc: TestCase) {
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.expectExit();
}

const WIDTH = 60;
const HEIGHT = 20;

describe('Word movement (Alt+F / Alt+B)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Alt+F stops right after the last letter of the word', async () => {
    testCase = await TestCase.builder()
      .withTestName('word-forward')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const origin = testCase.getCursorPosition();

    // Type "hello world"
    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Go back to start
    await send(testCase, CTRL_A);

    // Alt+F should land right after "hello" (offset 5 from origin)
    await send(testCase, ALT_F);
    const afterFirst = testCase.getCursorPosition();
    expect(afterFirst.x).toBe(origin.x + 5);

    // Second Alt+F should land right after "world" (offset 11 from origin)
    await send(testCase, ALT_F);
    const afterSecond = testCase.getCursorPosition();
    expect(afterSecond.x).toBe(origin.x + 11);

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+B stops at the first letter of the word', async () => {
    testCase = await TestCase.builder()
      .withTestName('word-backward')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const origin = testCase.getCursorPosition();

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Cursor is at end (offset 11). Alt+B should land on 'w' (offset 6)
    await send(testCase, ALT_B);
    const afterFirst = testCase.getCursorPosition();
    expect(afterFirst.x).toBe(origin.x + 6);

    // Second Alt+B should land on 'h' (offset 0)
    await send(testCase, ALT_B);
    const afterSecond = testCase.getCursorPosition();
    expect(afterSecond.x).toBe(origin.x);

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+F then Alt+B round-trips to same position', async () => {
    testCase = await TestCase.builder()
      .withTestName('word-roundtrip')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const origin = testCase.getCursorPosition();

    await testCase.sendKeys('foo bar baz');
    await testCase.sleepMs(200);

    await send(testCase, CTRL_A);

    // Alt+F to end of "foo", then Alt+B back to start of "foo"
    await send(testCase, ALT_F);
    const afterForward = testCase.getCursorPosition();
    expect(afterForward.x).toBe(origin.x + 3);

    await send(testCase, ALT_B);
    const afterBack = testCase.getCursorPosition();
    expect(afterBack.x).toBe(origin.x);

    await exitCleanly(testCase);
  }, 20000);
});
