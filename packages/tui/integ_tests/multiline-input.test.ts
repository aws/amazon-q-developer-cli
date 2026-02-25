/**
 * Integration tests for multi-line input editing in the prompt.
 *
 * Tests cover:
 * - Down arrow / Ctrl+N not clearing input when not browsing history
 * - Up/Down arrow navigation in multi-line input (via Alt+Enter newlines)
 * - Ctrl+A / Ctrl+E visual-line-aware movement in multi-line input
 * - Ctrl+K / Ctrl+U visual-line-aware kill in multi-line input
 * - History navigation still works correctly after the fix
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

// Control key bytes
const CTRL_A = '\x01';
const CTRL_E = '\x05';
const CTRL_F = '\x06';
const CTRL_J = '\x0a'; // Ctrl+J = newline
const CTRL_K = '\x0b';
const CTRL_N = '\x0e';
const CTRL_P = '\x10';
const CTRL_U = '\x15';

const DOWN_ARROW = '\x1b[B';
const UP_ARROW = '\x1b[A';

/** Send a control key with a small delay after for processing */
async function sendCtrl(tc: TestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(100);
}

async function exitCleanly(tc: TestCase) {
  // Clear all input: select-all equivalent by going to absolute start (Home)
  // then killing everything. For multi-line, we need multiple Ctrl+U to clear all lines.
  // Simpler: just send Ctrl+C to clear input, then Ctrl+C twice to exit.
  await tc.pressCtrlC(); // clears current input
  await tc.sleepMs(100);
  await tc.pressCtrlC(); // starts exit sequence
  await tc.sleepMs(100);
  await tc.pressCtrlC(); // confirms exit
  await tc.expectExit();
}

describe('Multi-line input editing', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Down arrow does not clear input when not browsing history', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-down-no-clear')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Press Down arrow — should NOT clear input
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+N does not clear input when not browsing history', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-n-no-clear')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.sendKeys('some text here');
    await testCase.sleepMs(200);

    // Press Ctrl+N — should NOT clear input
    await sendCtrl(testCase, CTRL_N);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('some text here');

    await exitCleanly(testCase);
  }, 20000);

  it('multi-line input: Up/Down arrows navigate between visual lines', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-up-down-nav')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create multi-line input: "line1\nline2"
    await testCase.sendKeys('line1');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J); // newline
    await testCase.sendKeys('line2');
    await testCase.sleepMs(200);

    // Cursor is at end of line2. Press Up — should move to line1
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(200);

    // Type 'X' to verify cursor is on line1
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    // 'X' should be inserted into line1, not line2
    expect(screenText).toContain('line1');
    expect(screenText).toContain('line2');
    // line1 should have X somewhere in it
    expect(screenText).toMatch(/line1.*X|lineX1|line1X|Xline1/);

    await exitCleanly(testCase);
  }, 20000);

  it('multi-line input: Down arrow on last line is no-op (preserves input)', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-down-last-line')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create multi-line input
    await testCase.sendKeys('first');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('second');
    await testCase.sleepMs(200);

    // Already on last line, press Down — should be no-op
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('first');
    expect(screenText).toContain('second');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+A moves to start of current visual line, not beginning of input', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-a')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create multi-line input: "hello\nworld"
    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('world');
    await testCase.sleepMs(200);

    // Cursor is at end of "world". Ctrl+A should go to start of "world" line
    await sendCtrl(testCase, CTRL_A);
    // Type 'Z' — should appear at start of second line
    await testCase.sendKeys('Z');
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('hello');
    expect(screenText).toContain('Zworld');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+E moves to end of current visual line, not end of input', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-e')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create multi-line input: "hello\nworld"
    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('world');
    await testCase.sleepMs(200);

    // Move up to first line
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(100);

    // Ctrl+E should go to end of first line ("hello"), not end of entire input
    await sendCtrl(testCase, CTRL_E);
    await testCase.sendKeys('Y');
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('helloY');
    expect(screenText).toContain('world');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+K kills to end of current visual line only', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-k')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create: "abcdef\nghijkl"
    await testCase.sendKeys('abcdef');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('ghijkl');
    await testCase.sleepMs(200);

    // Move up to first line
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(100);

    // Move to beginning of first line, then forward 3 chars (after "abc")
    await sendCtrl(testCase, CTRL_A);
    for (let i = 0; i < 3; i++) {
      await sendCtrl(testCase, CTRL_F);
    }

    // Ctrl+K should kill "def" only, preserving second line
    await sendCtrl(testCase, CTRL_K);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('abc');
    expect(screenText).not.toContain('def');
    expect(screenText).toContain('ghijkl');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+U kills to beginning of current visual line only', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-u')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create: "abcdef\nghijkl"
    await testCase.sendKeys('abcdef');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('ghijkl');
    await testCase.sleepMs(200);

    // Cursor is at end of "ghijkl". Move back 2 chars to be after "ghij"
    await sendCtrl(testCase, CTRL_A);
    for (let i = 0; i < 4; i++) {
      await sendCtrl(testCase, CTRL_F);
    }

    // Ctrl+U should kill "ghij" only, preserving first line and "kl"
    await sendCtrl(testCase, CTRL_U);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('abcdef');
    expect(screenText).toContain('kl');
    expect(screenText).not.toContain('ghij');

    await exitCleanly(testCase);
  }, 20000);

  it('history navigation still works: Up then Down returns to empty', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-history-still-works')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Submit a command to create history
    await testCase.sendKeys('previous command');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Press Up to recall history
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(200);

    let snapshot = testCase.getSnapshot();
    let screenText = snapshot.join('\n');
    expect(screenText).toContain('previous command');

    // Press Down to go past history — should clear back to empty
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(200);

    snapshot = testCase.getSnapshot();
    screenText = snapshot.join('\n');
    // Should show placeholder again (empty input)
    expect(screenText).toContain('ask a question');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 20000);

  it('Ctrl+P / Ctrl+N work same as Up/Down for multi-line navigation', async () => {
    testCase = await TestCase.builder()
      .withTestName('multiline-ctrl-p-n')
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Create multi-line input
    await testCase.sendKeys('top');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_J);
    await testCase.sendKeys('bottom');
    await testCase.sleepMs(200);

    // Ctrl+P should move up
    await sendCtrl(testCase, CTRL_P);
    await testCase.sendKeys('!');
    await testCase.sleepMs(200);

    let snapshot = testCase.getSnapshot();
    let screenText = snapshot.join('\n');
    expect(screenText).toContain('top');
    expect(screenText).toContain('bottom');

    // Ctrl+N should move back down — input should still be intact
    await sendCtrl(testCase, CTRL_N);
    await testCase.sleepMs(200);

    snapshot = testCase.getSnapshot();
    screenText = snapshot.join('\n');
    expect(screenText).toContain('top');
    expect(screenText).toContain('bottom');

    await exitCleanly(testCase);
  }, 20000);
});
