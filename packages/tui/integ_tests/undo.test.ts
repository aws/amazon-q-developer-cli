/**
 * Integration tests for undo (Ctrl+_ / Ctrl+/).
 *
 * Both C-_ and C-/ send byte 0x1F in most terminals.
 * Undo should restore the previous input state.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const CTRL_UNDERSCORE = '\x1f'; // C-_ / C-/
const CTRL_A = '\x01';
const CTRL_K = '\x0b';
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

describe('Undo (Ctrl+_)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('undoes a kill-line operation', async () => {
    testCase = await TestCase.builder()
      .withTestName('undo-kill-line')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Kill from start of line
    await send(testCase, CTRL_A);
    await send(testCase, CTRL_K);
    await testCase.sleepMs(200);

    // Text should be gone
    const afterKill = flattenSnapshot(testCase);
    expect(afterKill).not.toContain('hello world');

    // Undo should restore it
    await send(testCase, CTRL_UNDERSCORE);
    await testCase.sleepMs(200);

    const afterUndo = flattenSnapshot(testCase);
    expect(afterUndo).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('undoes a backward-kill-word operation', async () => {
    testCase = await TestCase.builder()
      .withTestName('undo-kill-word')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Ctrl+W deletes "world"
    await send(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const afterKill = flattenSnapshot(testCase);
    expect(afterKill).not.toContain('world');

    // Undo restores "world"
    await send(testCase, CTRL_UNDERSCORE);
    await testCase.sleepMs(200);

    const afterUndo = flattenSnapshot(testCase);
    expect(afterUndo).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('multiple undos walk back through history', async () => {
    testCase = await TestCase.builder()
      .withTestName('undo-multiple')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('aaa');
    await testCase.sleepMs(200);

    // Kill "aaa"
    await send(testCase, CTRL_A);
    await send(testCase, CTRL_K);
    await testCase.sleepMs(200);

    await testCase.sendKeys('bbb');
    await testCase.sleepMs(200);

    // Kill "bbb"
    await send(testCase, CTRL_A);
    await send(testCase, CTRL_K);
    await testCase.sleepMs(200);

    const afterKills = flattenSnapshot(testCase);
    expect(afterKills).not.toContain('bbb');

    // First undo restores "bbb"
    await send(testCase, CTRL_UNDERSCORE);
    await testCase.sleepMs(200);
    const afterUndo1 = flattenSnapshot(testCase);
    expect(afterUndo1).toContain('bbb');

    // Second undo restores empty state (before "bbb" was typed)
    await send(testCase, CTRL_UNDERSCORE);
    await testCase.sleepMs(200);
    const afterUndo2 = flattenSnapshot(testCase);
    expect(afterUndo2).not.toContain('bbb');

    // Type something so Ctrl+C can exit cleanly
    await testCase.sendKeys('x');
    await testCase.sleepMs(100);

    await exitCleanly(testCase);
  }, 30000);

  it('undo is no-op when stack is empty', async () => {
    testCase = await TestCase.builder()
      .withTestName('undo-empty-stack')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Undo on fresh prompt with no edits should be a no-op
    await send(testCase, CTRL_UNDERSCORE);
    await send(testCase, CTRL_UNDERSCORE);
    await testCase.sleepMs(200);

    // Prompt should still work — type and verify
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('hello');

    await exitCleanly(testCase);
  }, 20000);
});
