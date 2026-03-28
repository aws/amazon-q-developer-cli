/**
 * Integration tests for word deletion (Alt+D kill-word, Ctrl+W backward-kill-word).
 *
 * Verifies emacs semantics:
 * - Alt+D deletes from cursor to end of word (preserves trailing space)
 * - Ctrl+W deletes from cursor backward to start of previous word
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const ALT_D = '\x1bd';
const ALT_F = '\x1bf';
const CTRL_A = '\x01';
const CTRL_W = '\x17';

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

function flattenSnapshot(tc: TestCase): string {
  return tc.getSnapshot().join(' ').replace(/\s+/g, ' ');
}

const WIDTH = 60;
const HEIGHT = 20;

describe('Word deletion (Alt+D / Ctrl+W)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Alt+D deletes word forward, preserving trailing space', async () => {
    testCase = await TestCase.builder()
      .withTestName('delete-word-forward')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world foo');
    await testCase.sleepMs(200);

    // Move to start, then Alt+D to delete "hello"
    await send(testCase, CTRL_A);
    await send(testCase, ALT_D);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    // "hello" deleted, but space preserved → " world foo" remains
    expect(screenText).not.toContain('hello');
    expect(screenText).toContain('world foo');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+W deletes word backward', async () => {
    testCase = await TestCase.builder()
      .withTestName('delete-word-backward')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world foo');
    await testCase.sleepMs(200);

    // Cursor at end. Ctrl+W should delete "foo" backward
    await send(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('hello world');
    expect(screenText).not.toContain('foo');

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+D from mid-word deletes rest of word only', async () => {
    testCase = await TestCase.builder()
      .withTestName('delete-word-forward-mid')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Move to start, forward one word (past "hello"), skip the space, land on "w"
    // Then Alt+D should delete "world"
    await send(testCase, CTRL_A);
    await send(testCase, ALT_F); // cursor after "hello"

    // Move one char forward to skip the space and land on 'w'
    await testCase.sendKeys('\x1b[C'); // right arrow
    await testCase.sleepMs(100);

    await send(testCase, ALT_D);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('hello');
    expect(screenText).not.toContain('world');

    await exitCleanly(testCase);
  }, 20000);
});
