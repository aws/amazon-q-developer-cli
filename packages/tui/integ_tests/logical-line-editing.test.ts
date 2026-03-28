/**
 * Integration tests for logical-line editing (Ctrl+A, Ctrl+E, Ctrl+K, Ctrl+U).
 *
 * These tests use a 30x20 terminal so that a single logical line wraps across
 * multiple visual lines, then verify that Ctrl+A/E/K/U operate on the full
 * logical line (delimited by \n), NOT the visual (wrapped) line.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const CTRL_A = '\x01';
const CTRL_E = '\x05';
const CTRL_F = '\x06';
const CTRL_J = '\x0a'; // newline
const CTRL_K = '\x0b';
const CTRL_U = '\x15';

async function sendCtrl(tc: TestCase, key: string) {
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

/** Join snapshot rows and collapse whitespace for assertions that span wrapped lines. */
function flattenSnapshot(tc: TestCase): string {
  return tc.getSnapshot().join(' ').replace(/\s+/g, ' ');
}

// 30x20 terminal — input text wider than 30 chars wraps to the next visual row.
const WIDTH = 30;
const HEIGHT = 20;

describe('Logical-line editing with visual wrapping', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // "the quick brown fox jumps over the lazy dog" (44 chars)
  // Wraps across two visual rows in a 30-col terminal.

  it('Ctrl+A moves cursor to start of logical line, not visual line', async () => {
    testCase = await TestCase.builder()
      .withTestName('logical-ctrl-a-wrap')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Record the cursor position before any input — this is where the input area starts.
    const origin = testCase.getCursorPosition();

    await testCase.sendKeys('the quick brown fox jumps over the lazy dog');
    await testCase.sleepMs(300);

    // Cursor is now on the second visual row (past the wrap).
    // Ctrl+A should jump back to the origin — start of the logical line.
    await sendCtrl(testCase, CTRL_A);

    const after = testCase.getCursorPosition();
    expect(after.x).toBe(origin.x);
    expect(after.y).toBe(origin.y);

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+E moves cursor to end of logical line, not visual line', async () => {
    testCase = await TestCase.builder()
      .withTestName('logical-ctrl-e-wrap')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('the quick brown fox jumps over the lazy dog');
    await testCase.sleepMs(300);

    // Record cursor at end of input — this is where Ctrl+E should return to.
    const endPos = testCase.getCursorPosition();

    // Go to start, then Ctrl+E should go back to end of logical line.
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_E);

    const after = testCase.getCursorPosition();
    expect(after.x).toBe(endPos.x);
    expect(after.y).toBe(endPos.y);

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+K kills to end of logical line, not visual line', async () => {
    testCase = await TestCase.builder()
      .withTestName('logical-ctrl-k-wrap')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // "please delete this entire long line\nkeep this"
    await testCase.sendKeys('please delete this entire long line');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('keep this');
    await testCase.sleepMs(200);

    // Navigate to start of first logical line, then forward 10 chars ("please del")
    await sendCtrl(testCase, CTRL_A); // start of "keep this"
    await testCase.sendKeys('\x1b[D'); // left arrow into first line
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A); // start of first line
    for (let i = 0; i < 10; i++) {
      await sendCtrl(testCase, CTRL_F);
    }

    // Ctrl+K kills rest of first logical line, preserves second line
    await sendCtrl(testCase, CTRL_K);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('please del');
    expect(screenText).not.toContain('entire long');
    expect(screenText).toContain('keep this');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+U kills to beginning of logical line, not visual line', async () => {
    testCase = await TestCase.builder()
      .withTestName('logical-ctrl-u-wrap')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // "keep this\ndelete most of this very long line"
    await testCase.sendKeys('keep this');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('delete most of this very long line');
    await testCase.sleepMs(200);

    // Move back 4 chars (cursor before "line")
    for (let i = 0; i < 4; i++) {
      await testCase.sendKeys('\x1b[D');
      await testCase.sleepMs(50);
    }

    // Ctrl+U kills from start of second logical line to cursor
    await sendCtrl(testCase, CTRL_U);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('keep this');
    expect(screenText).toContain('line');
    expect(screenText).not.toContain('delete most');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+A/E do not alter content', async () => {
    testCase = await TestCase.builder()
      .withTestName('logical-ctrl-ae-no-mutation')
      .withTerminal({ width: WIDTH, height: HEIGHT })
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('the quick brown fox jumps over the lazy dog');
    await testCase.sleepMs(200);

    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_E);
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_E);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('the quick brown fox jumps over the lazy dog');

    await exitCleanly(testCase);
  }, 20000);
});
