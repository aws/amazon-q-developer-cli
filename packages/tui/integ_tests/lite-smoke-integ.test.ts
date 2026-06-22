/**
 * Smoke tests for BOTH-classified integ tests in lite mode.
 *
 * Since lite mode shares the same PromptInput component as TUI mode, all
 * input-handling mechanisms (word movement, kill ring, undo, reverse search,
 * multiline, etc.) should work identically. These smoke tests verify no crash
 * and basic mechanism functionality when running in lite mode.
 *
 * Scenarios covered:
 *   1. basic-lifecycle — launch, verify prompt, clean exit
 *   2. keyboard-shortcuts — Ctrl+A, Ctrl+K, kill ring
 *   3. multiline-input — Ctrl+J newline, arrow navigation
 *   4. word-deletion — Ctrl+W backward-kill-word
 *   5. word-movement — Alt+F / Alt+B cursor movement
 *   6. reverse-search — Ctrl+R search UI
 *   7. slash-command-autocomplete — /cl triggers menu
 *   8. undo — Ctrl+_ restores killed text
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { launchLiteInteg } from '../e2e_tests/lite/helpers/integ-lifecycle';

// Control keys
const CTRL_A = '\x01';
const CTRL_J = '\x0a'; // newline
const CTRL_K = '\x0b';
const CTRL_R = '\x12';
const CTRL_W = '\x17';
const CTRL_Y = '\x19';
const CTRL_UNDERSCORE = '\x1f'; // undo

// Alt keys
const ALT_F = '\x1bf';
const ALT_B = '\x1bb';

const UP_ARROW = '\x1b[A';

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

describe('Lite mode smoke tests (BOTH-classified integ)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // 1. basic-lifecycle
  it('launches in lite mode, shows prompt, exits cleanly', async () => {
    testCase = await launchLiteInteg('lite-smoke-lifecycle');

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
    expect(store.messages).toHaveLength(0);

    // Type text then Ctrl+C to clear, then exit
    await testCase.sendKeys('hi');
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 30000);

  // 2. keyboard-shortcuts — Ctrl+A, Ctrl+K, Ctrl+Y
  it('Ctrl+A moves to start, Ctrl+K kills, Ctrl+Y yanks', async () => {
    testCase = await launchLiteInteg('lite-smoke-keyboard');

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Move to start
    await send(testCase, CTRL_A);
    // Kill to end of line
    await send(testCase, CTRL_K);
    await testCase.sleepMs(200);

    // Text should be gone from screen
    const afterKill = flattenSnapshot(testCase);
    expect(afterKill).not.toContain('hello world');

    // Yank it back
    await send(testCase, CTRL_Y);
    await testCase.sleepMs(200);

    const afterYank = flattenSnapshot(testCase);
    expect(afterYank).toContain('hello world');

    await exitCleanly(testCase);
  }, 30000);

  // 3. multiline-input — Ctrl+J newline, arrow navigation
  it('Ctrl+J creates newline, Up arrow navigates between lines', async () => {
    testCase = await launchLiteInteg('lite-smoke-multiline');

    // Create multi-line: "line1\nline2"
    await testCase.sendKeys('line1');
    await testCase.sleepMs(100);
    await send(testCase, CTRL_J);
    await testCase.sendKeys('line2');
    await testCase.sleepMs(300);

    // Verify both lines visible on screen
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('line1');
    expect(snap).toContain('line2');

    // Navigate up and type X to confirm cursor moves to line1
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(100);
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    const snapAfter = testCase.getSnapshot().join('\n');
    // X should be inserted into line1
    expect(snapAfter).toMatch(/line1.*X|lineX1|line1X|Xline1/);
    // line2 should still be present
    expect(snapAfter).toContain('line2');

    await exitCleanly(testCase);
  }, 30000);

  // 4. word-deletion — Ctrl+W
  it('Ctrl+W deletes word backward', async () => {
    testCase = await launchLiteInteg('lite-smoke-word-deletion', {
      terminal: { width: 60, height: 20 },
    });

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Ctrl+W deletes "world" backward
    await send(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('hello');
    expect(screenText).not.toContain('hello world');

    await exitCleanly(testCase);
  }, 30000);

  // 5. word-movement — Alt+F / Alt+B
  it('Alt+F and Alt+B move cursor by word', async () => {
    testCase = await launchLiteInteg('lite-smoke-word-movement', {
      terminal: { width: 60, height: 20 },
    });

    const origin = testCase.getCursorPosition();

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    // Alt+B should move back to start of "world" (offset 6 from origin)
    await send(testCase, ALT_B);
    const afterBack = testCase.getCursorPosition();
    expect(afterBack.x).toBe(origin.x + 6);

    // Alt+F should move forward to end of "world" (offset 11 from origin)
    await send(testCase, ALT_F);
    const afterForward = testCase.getCursorPosition();
    expect(afterForward.x).toBe(origin.x + 11);

    await exitCleanly(testCase);
  }, 30000);

  // 6. reverse-search — Ctrl+R
  it('Ctrl+R opens reverse-i-search prompt', async () => {
    testCase = await launchLiteInteg('lite-smoke-reverse-search');

    // Submit a command to create history
    await testCase.sendKeys('hello world');
    await testCase.sleepMs(150);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Enter reverse search
    await send(testCase, CTRL_R);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain("(reverse-i-search)`':");

    // Type a query and verify match
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    const snapAfter = testCase.getSnapshot().join('\n');
    expect(snapAfter).toContain('hello world');

    // Exit search
    await testCase.pressEscape();
    await testCase.sleepMs(100);

    await exitCleanly(testCase);
  }, 30000);

  // 7. slash-command-autocomplete
  it('typing /cl syncs commandInputValue and does not crash', async () => {
    testCase = await launchLiteInteg('lite-smoke-slash-autocomplete');

    // Type a slash command prefix slowly to trigger autocomplete
    for (const char of '/cl') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);

    // Verify the store received the input (PromptInput syncs to commandInputValue)
    const store = await testCase.getStore();
    expect(store.commandInputValue).toBe('/cl');

    // Verify the typed text is visible in the terminal
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('/cl');

    await exitCleanly(testCase);
  }, 30000);

  // 8. undo — Ctrl+_ restores killed text
  it('Ctrl+_ undoes a kill-line operation', async () => {
    testCase = await launchLiteInteg('lite-smoke-undo', {
      terminal: { width: 60, height: 20 },
    });

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
  }, 30000);
});
