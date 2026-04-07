/**
 * Integration tests for Ctrl+R reverse incremental search.
 *
 * Tests cover:
 * - Entering/exiting reverse search mode
 * - Incremental search matching
 * - Cycling through older matches with repeated Ctrl+R
 * - No-match behavior
 * - Backspace editing the search query
 * - Exit via various keys (Ctrl+A, Ctrl+E, Right arrow, Enter)
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

const CTRL_A = '\x01';
const CTRL_E = '\x05';
const CTRL_R = '\x12';
const RIGHT_ARROW = '\x1b[C';
const UP_ARROW = '\x1b[A';
const BACKSPACE = '\x7f';

async function sendCtrl(tc: TestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(100);
}

async function exitCleanly(tc: TestCase) {
  // Clear any input first
  await sendCtrl(tc, CTRL_A);
  await tc.sleepMs(50);
  await sendCtrl(tc, '\x0b'); // Ctrl+K
  await tc.sleepMs(100);
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}

/** Submit a command (adds to history) and wait for prompt to return */
async function submitCommand(tc: TestCase, cmd: string) {
  await tc.sendKeys(cmd);
  await tc.sleepMs(100);
  await tc.pressEnter();
  await tc.sleepMs(500);
}

describe('Reverse incremental search (Ctrl+R)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('shows reverse-i-search prompt on Ctrl+R', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-enter')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await sendCtrl(testCase, CTRL_R);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain("(reverse-i-search)`': ");

    // Exit reverse search before cleanup
    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('finds matching history entry', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-basic-match')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');
    await submitCommand(testCase, 'foo bar');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain("(reverse-i-search)`hello': hello world");

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('progressively narrows search', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-progressive')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'cd packages/core');
    await submitCommand(testCase, 'cd packages/tui');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('cd');
    await testCase.sleepMs(200);

    // Should match most recent: "cd packages/tui"
    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('cd packages/tui');

    // Narrow further
    await testCase.sendKeys(' packages/c');
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('cd packages/core');

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+R again cycles to older match', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-cycle')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'echo alpha');
    await submitCommand(testCase, 'echo beta');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('echo');
    await testCase.sleepMs(200);

    // First match: most recent "echo beta"
    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('echo beta');

    // Ctrl+R again: older match "echo alpha"
    await sendCtrl(testCase, CTRL_R);
    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('echo alpha');

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('no match keeps last matched result visible', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-no-match')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    // Type something that won't match
    await testCase.sendKeys('xyz');
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    // Should still show the last matched result
    expect(snap).toContain('hello world');
    // But query should show the failing search
    expect(snap).toContain('helloxyz');

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+A exits search and moves cursor to beginning', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-exit-ctrl-a')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    // Exit with Ctrl+A
    await sendCtrl(testCase, CTRL_A);

    const snap = testCase.getSnapshot().join('\n');
    // Should no longer show reverse-i-search prompt
    expect(snap).not.toContain('reverse-i-search');
    // Input should be the matched line
    expect(snap).toContain('hello world');

    // Verify cursor is at beginning by typing a char
    await testCase.sendKeys('Z');
    await testCase.sleepMs(200);
    const snap2 = testCase.getSnapshot().join('\n');
    expect(snap2).toContain('Zhello world');

    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+E exits search and moves cursor to end', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-exit-ctrl-e')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    // Exit with Ctrl+E
    await sendCtrl(testCase, CTRL_E);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain('reverse-i-search');
    expect(snap).toContain('hello world');

    // Verify cursor is at end by typing a char
    await testCase.sendKeys('Z');
    await testCase.sleepMs(200);
    const snap2 = testCase.getSnapshot().join('\n');
    expect(snap2).toContain('hello worldZ');

    await exitCleanly(testCase);
  }, 20000);

  it('Right arrow exits search with cursor at match position', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-exit-right')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('world');
    await testCase.sleepMs(200);

    // Exit with Right arrow
    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(100);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain('reverse-i-search');
    expect(snap).toContain('hello world');

    // Verify cursor is at match position + 1 (right arrow accepts then moves forward one)
    // "world" starts at offset 6 in "hello world", right arrow advances to 7
    const cursorPos = testCase.getCursorPosition();
    const snapLines = testCase.getSnapshot();
    // Find the input row (last row containing "hello world" — the prompt, not conversation)
    let inputRowIdx = -1;
    for (let i = snapLines.length - 1; i >= 0; i--) {
      if (snapLines[i]!.includes('hello world')) { inputRowIdx = i; break; }
    }
    expect(inputRowIdx).not.toBe(-1);
    const helloIdx = snapLines[inputRowIdx]!.indexOf('hello ');
    expect(cursorPos.x).toBe(helloIdx + 6 + 1); // matchPos=6, +1 for right arrow

    await exitCleanly(testCase);
  }, 20000);

  it('Enter exits search and submits the matched line', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-exit-enter')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    // Press Enter to submit
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const snap = testCase.getSnapshot().join('\n');
    // Should no longer show reverse-i-search
    expect(snap).not.toContain('reverse-i-search');

    // Exit — the submitted command triggers mock processing, use Ctrl+C to cancel
    await tc_exitAfterSubmit(testCase);
  }, 20000);

  it('Backspace edits the search query', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-backspace')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');
    await submitCommand(testCase, 'help me');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('help');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('help me');

    // Backspace once: "help" -> "hel"
    await testCase.sendKeys(BACKSPACE);
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    // "hel" matches "help me" (most recent containing "hel")
    expect(snap).toContain("(reverse-i-search)`hel':");

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('Escape accepts search and keeps matched line', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-escape-accept')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    // Type something first
    await testCase.sendKeys('my current input');
    await testCase.sleepMs(200);

    // Enter reverse search
    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    // Escape to accept (not abort — keeps matched line)
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain('reverse-i-search');
    // Should show the matched line, NOT the original input
    expect(snap).toContain('hello world');
    expect(snap).not.toContain('my current input');

    await exitCleanly(testCase);
  }, 20000);

  it('backspace after failed search re-finds previous match', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-backspace-after-fail')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    // Type chars that won't match: "hello" -> "hellosss"
    await testCase.sendKeys('sss');
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    // Should still show hello world (no-match keeps last result)
    expect(snap).toContain('hello world');

    // Backspace once: "hellosss" -> "helloss" — still no match
    await testCase.sendKeys(BACKSPACE);
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    // Should still show hello world, not lose the match
    expect(snap).toContain('hello world');

    // Backspace twice more: "helloss" -> "hello" — should match again
    await testCase.sendKeys(BACKSPACE);
    await testCase.sleepMs(100);
    await testCase.sendKeys(BACKSPACE);
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain("(reverse-i-search)`hello': hello world");

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('up arrow after accepting search navigates relative to matched entry', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-up-after-accept')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    // Build history: oldest to newest
    await submitCommand(testCase, 'first command');
    await submitCommand(testCase, 'hello world');
    await submitCommand(testCase, 'how are you');

    // Search for "hello"
    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('hello world');

    // Accept with right arrow (exits search, cursor at match pos)
    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(200);

    // Now press up — should show "first command" (the entry before "hello world")
    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('first command');

    await exitCleanly(testCase);
  }, 20000);

  it('double Ctrl+R reuses last search string', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-double-ctrl-r')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'echo alpha');
    await submitCommand(testCase, 'echo beta');

    // First search: type "echo", find "echo beta"
    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('echo');
    await testCase.sleepMs(200);

    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('echo beta');

    // Accept with Escape
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    // Start a new search with Ctrl+R, then immediately Ctrl+R again
    // Should reuse "echo" as the query
    await sendCtrl(testCase, CTRL_R);
    await testCase.sleepMs(100);
    await sendCtrl(testCase, CTRL_R);
    await testCase.sleepMs(200);

    snap = testCase.getSnapshot().join('\n');
    // Should have reused "echo" and found a match
    expect(snap).toContain("(reverse-i-search)`echo':");
    expect(snap).toContain('echo');

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('Ctrl+R with no matching query shows empty result', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-empty-history')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    // Enter reverse search and type something unlikely to match
    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('zzzzxqwv9999');
    await testCase.sleepMs(200);

    const snap = testCase.getSnapshot().join('\n');
    // Should show the query in the prompt without crashing
    expect(snap).toContain("(reverse-i-search)`zzzzxqwv9999':");

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);

  it('cursor is positioned at the match start within the matched line', async () => {
    testCase = await TestCase.builder()
      .withTestName('rsearch-cursor-at-match')
      .launch();
    await testCase.waitForVisibleText('ask a question', 10000);

    await submitCommand(testCase, 'hello world');

    await sendCtrl(testCase, CTRL_R);
    await testCase.sendKeys('world');
    await testCase.sleepMs(200);

    // The prompt renders: (reverse-i-search)`world': hello world
    // Cursor should be on the "w" of "world" in "hello world"
    const snap = testCase.getSnapshot();
    const cursorPos = testCase.getCursorPosition();

    // Find the row containing "hello " to locate the matched line text
    const searchRow = snap.findIndex((r) => r.includes("reverse-i-search") && r.includes('hello'));
    expect(searchRow).not.toBe(-1);
    const row = snap[searchRow]!;

    // "world" starts at offset 6 in "hello world"
    const helloIdx = row.indexOf('hello ');
    expect(helloIdx).not.toBe(-1);
    expect(cursorPos.x).toBe(helloIdx + 6);

    await testCase.pressEscape();
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 20000);
});

/** Helper to exit after a command was submitted (mock ACP may be processing) */
async function tc_exitAfterSubmit(tc: TestCase) {
  // Cancel any processing
  await tc.pressCtrlC();
  await tc.sleepMs(300);
  // Clear input and exit
  await sendCtrl(tc, CTRL_A);
  await tc.sleepMs(50);
  await sendCtrl(tc, '\x0b'); // Ctrl+K
  await tc.sleepMs(100);
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}
