/**
 * Integration tests for newline-insertion shortcuts in the prompt.
 *
 * Ctrl+J, Shift+Enter, and Alt+Enter all insert a literal newline instead of
 * submitting the prompt — see PromptInput's newline branch
 * (`key.return && (key.meta || key.shift)` for Shift/Alt+Enter, and the
 * dedicated Ctrl+J = \n handling). Being able to tell these apart from a plain
 * Enter is what makes multi-line composition work; conflating them is the class
 * of bug that got the lite landing reverted (#3227).
 *
 * Encodings are chosen to be reliable over the plain xterm PTY the integ
 * harness uses (Kitty protocol flag is off there):
 *   - Ctrl+J     -> raw \x0a
 *   - Shift+Enter-> xterm modifyOtherKeys `CSI 27 ; 2 ; 13 ~` (\x1b[27;2;13~)
 *   - Alt+Enter  -> ESC + CR (\x1b\r)
 * All three parse without depending on Kitty mode (covered by twinki
 * keys.test.ts).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const CTRL_J = '\x0a'; // Ctrl+J — newline
const SHIFT_ENTER = '\x1b[27;2;13~'; // xterm modifyOtherKeys: keycode 13, mod 2 (shift)
const SHIFT_ENTER_CSI_U = '\x1b[13;2u'; // CSI-u: codepoint 13, mod 2 (shift) — sent by tmux extended-keys
const ALT_ENTER = '\x1b\r'; // ESC + CR — parses to shift+enter / newline

async function exitCleanly(tc: TestCase) {
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.expectExit();
}

/**
 * Type "first line", press the given newline key, type "second line", and
 * assert both lines coexist (i.e. the key inserted a newline, did NOT submit)
 * and that the cursor landed on the second visual row.
 */
async function expectInsertsNewline(tc: TestCase, newlineKey: string) {
  await tc.waitForVisibleText('ask a question', 15000);

  await tc.sendKeys('first line');
  await tc.sleepMs(100);
  await tc.sendKeys(newlineKey);
  await tc.sleepMs(100);
  await tc.sendKeys('second line');
  await tc.sleepMs(200);

  // Both lines present — the key inserted a newline rather than submitting
  // (a submit would have cleared the input and started a turn).
  const snap = tc.getSnapshot().join('\n');
  expect(snap).toContain('first line');
  expect(snap).toContain('second line');

  // Cursor is on the second line: typing 'X' appends there, first line intact.
  await tc.sendKeys('X');
  await tc.sleepMs(150);
  const snap2 = tc.getSnapshot().join('\n');
  expect(snap2).toContain('second lineX');
  expect(snap2).toContain('first line');
}

describe('Newline-insertion shortcuts', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Ctrl+J inserts a newline instead of submitting', async () => {
    testCase = await TestCase.builder().withTestName('newline-ctrl-j').launch();
    await expectInsertsNewline(testCase, CTRL_J);
    await exitCleanly(testCase);
  }, 20000);

  it('Shift+Enter inserts a newline instead of submitting', async () => {
    testCase = await TestCase.builder()
      .withTestName('newline-shift-enter')
      .launch();
    await expectInsertsNewline(testCase, SHIFT_ENTER);
    await exitCleanly(testCase);
  }, 20000);

  it('Shift+Enter (CSI-u encoding) inserts a newline instead of submitting', async () => {
    testCase = await TestCase.builder()
      .withTestName('newline-shift-enter-csi-u')
      .launch();
    await expectInsertsNewline(testCase, SHIFT_ENTER_CSI_U);
    await exitCleanly(testCase);
  }, 20000);

  it('Alt+Enter inserts a newline instead of submitting', async () => {
    testCase = await TestCase.builder()
      .withTestName('newline-alt-enter')
      .launch();
    await expectInsertsNewline(testCase, ALT_ENTER);
    await exitCleanly(testCase);
  }, 20000);
});
