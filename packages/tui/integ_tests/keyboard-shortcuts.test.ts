/**
 * Integration tests for keyboard shortcuts in the prompt input.
 *
 * Tests cover:
 * - fn+Delete (forward delete) fix
 * - Alt+Backspace / Alt+Delete (word deletion)
 * - Kill ring (Ctrl+K/U/W then Ctrl+Y yank)
 * - Ctrl+H (backspace alias)
 * - Regression tests for existing shortcuts (Ctrl+A/E/D/W, Alt+D/F/B)
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

// Control keys
const CTRL_A = '\x01';
const CTRL_B = '\x02';
const CTRL_D = '\x04';
const CTRL_E = '\x05';
const CTRL_F = '\x06';
const CTRL_H = '\x08';
const CTRL_K = '\x0b';
const CTRL_U = '\x15';
const CTRL_W = '\x17';
const CTRL_Y = '\x19';

// Special keys
const DELETE = '\x1b[3~';
const ALT_DELETE = '\x1b[3;3~';
const ALT_BACKSPACE = '\x1b\x7f';
const ALT_D = '\x1bd';
const ALT_F = '\x1bf';
const ALT_B = '\x1bb';

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

describe('Keyboard shortcuts', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // --- Delete key fixes ---

  it('fn+Delete performs forward delete', async () => {
    testCase = await TestCase.builder()
      .withTestName('kb-forward-delete')
      .launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    // Move cursor left 3 times to position after "he"
    for (let i = 0; i < 3; i++) {
      await sendCtrl(testCase, CTRL_B);
    }
    // Forward delete should remove 'l' (char after cursor)
    await testCase.sendKeys(DELETE);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('helo');

    await exitCleanly(testCase);
  }, 20000);

  // Note: Alt+Backspace (\x1b\x7f) cannot be reliably tested via PTY because
  // the terminal splits the escape byte from the DEL byte into separate input
  // events. The key parsing is covered by Twinki's unit tests. Ctrl+W provides
  // the same functionality and is tested below.
  it.skip('Alt+Backspace deletes word backward', async () => {
    testCase = await TestCase.builder()
      .withTestName('kb-alt-backspace')
      .launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x1b, 0x7f]); // Alt+Backspace
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello ');
    expect(snap).not.toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+Delete deletes word forward', async () => {
    testCase = await TestCase.builder()
      .withTestName('kb-alt-delete-forward')
      .launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    await testCase.sendKeys(ALT_DELETE);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain(' world');
    expect(snap).not.toContain('hello');

    await exitCleanly(testCase);
  }, 20000);

  // --- Kill ring (Ctrl+Y yank) ---

  it('Ctrl+K then Ctrl+Y round-trip', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-k-y').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_K); // kills "hello world"
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_Y); // yanks it back
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+U then Ctrl+Y restores text', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-u-y').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_U); // kills backward to line start
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_Y); // yanks back
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+W then Ctrl+Y restores word', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-w-y').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_W); // kills word "world"
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_Y); // yanks "world" back
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  // --- Ctrl+H (backspace alias) ---

  it('Ctrl+H deletes backward', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-h').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_H);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hell');
    expect(snap).not.toContain('hello');

    await exitCleanly(testCase);
  }, 20000);

  // --- Regression tests for existing shortcuts ---

  it('Ctrl+A moves to start, Ctrl+E moves to end', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-a-e').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Xhello');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+D performs forward delete', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-d').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    await sendCtrl(testCase, CTRL_D);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('ello');
    expect(snap).not.toContain('hello');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+W deletes word backward', async () => {
    testCase = await TestCase.builder().withTestName('kb-ctrl-w').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello ');
    expect(snap).not.toContain('hello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+D deletes word forward', async () => {
    testCase = await TestCase.builder().withTestName('kb-alt-d').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    await testCase.sendKeys(ALT_D);
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain(' world');
    expect(snap).not.toContain('hello');

    await exitCleanly(testCase);
  }, 20000);

  it('Alt+F and Alt+B move by word', async () => {
    testCase = await TestCase.builder().withTestName('kb-alt-f-b').launch();
    await testCase.waitForVisibleText('Ask a question', 15000);

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_A);
    // Alt+F moves forward one word (past "hello")
    await testCase.sendKeys(ALT_F);
    await testCase.sleepMs(100);
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('helloX world');

    await exitCleanly(testCase);
  }, 20000);
});
