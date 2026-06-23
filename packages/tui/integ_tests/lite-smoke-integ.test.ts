/**
 * Lite mode shares TUI's PromptInput component, so all input-handling
 * mechanisms (word movement, kill ring, undo, reverse search, multiline, ...)
 * must work identically. These smoke tests verify no crash and basic
 * mechanism functionality when running in lite mode.
 */

import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import {
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

// Control keys
const CTRL_A = '\x01';
const CTRL_J = '\x0a'; // newline
const CTRL_K = '\x0b';
const CTRL_R = '\x12';
const CTRL_W = '\x17';
const CTRL_Y = '\x19';
const CTRL_UNDERSCORE = '\x1f'; // undo

const UP_ARROW = '\x1b[A';

async function send(tc: TestCase, key: string) {
  await tc.sendKeys(key);
  await tc.sleepMs(100);
}

// These cases leave text in the prompt; the exit ladder needs the three Ctrl+C
// presses SPACED (a burst gets coalesced and the first only clears input), so
// this differs from the shared exitLiteInteg() burst exit.
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
  trackCleanup(() => testCase);

  it('launches in lite mode, shows prompt, exits cleanly', async () => {
    testCase = await launchLiteInteg('lite-smoke-lifecycle');

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
    expect(store.messages).toHaveLength(0);

    await testCase.sendKeys('hi');
    await testCase.sleepMs(100);
    await exitCleanly(testCase);
  }, 30000);

  // Ctrl+A/Ctrl+K kill-line then restore: Ctrl+Y yanks the kill ring, Ctrl+_
  // undoes the kill. Same preamble, only the restore key differs.
  it.each([
    { restoreKey: CTRL_Y, label: 'Ctrl+Y yanks' },
    { restoreKey: CTRL_UNDERSCORE, label: 'Ctrl+_ undoes' },
  ])(
    'Ctrl+A to start, Ctrl+K kills, $label kill-line back',
    async ({ restoreKey }) => {
      testCase = await launchLiteInteg('lite-smoke-keyboard', {
        terminal: { width: 60, height: 20 },
      });

      await testCase.sendKeys('hello world');
      await testCase.sleepMs(200);

      await send(testCase, CTRL_A);
      await send(testCase, CTRL_K);
      await testCase.sleepMs(200);

      const afterKill = flattenSnapshot(testCase);
      expect(afterKill).not.toContain('hello world');

      await send(testCase, restoreKey);
      await testCase.sleepMs(200);

      const afterRestore = flattenSnapshot(testCase);
      expect(afterRestore).toContain('hello world');

      await exitCleanly(testCase);
    },
    30000
  );

  it('Ctrl+J creates newline, Up arrow navigates between lines', async () => {
    testCase = await launchLiteInteg('lite-smoke-multiline');

    await testCase.sendKeys('line1');
    await testCase.sleepMs(100);
    await send(testCase, CTRL_J);
    await testCase.sendKeys('line2');
    await testCase.sleepMs(300);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('line1');
    expect(snap).toContain('line2');

    await testCase.sendKeys(UP_ARROW);
    await testCase.sleepMs(100);
    await testCase.sendKeys('X');
    await testCase.sleepMs(200);

    const snapAfter = testCase.getSnapshot().join('\n');
    expect(snapAfter).toMatch(/line1.*X|lineX1|line1X|Xline1/);
    expect(snapAfter).toContain('line2');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+W deletes word backward', async () => {
    testCase = await launchLiteInteg('lite-smoke-word-deletion', {
      terminal: { width: 60, height: 20 },
    });

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(200);

    await send(testCase, CTRL_W);
    await testCase.sleepMs(200);

    const screenText = flattenSnapshot(testCase);
    expect(screenText).toContain('hello');
    expect(screenText).not.toContain('hello world');

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+R opens reverse-i-search prompt', async () => {
    testCase = await launchLiteInteg('lite-smoke-reverse-search');

    await testCase.sendKeys('hello world');
    await testCase.sleepMs(150);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await send(testCase, CTRL_R);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain("(reverse-i-search)`':");

    await testCase.sendKeys('hello');
    await testCase.sleepMs(200);

    const snapAfter = testCase.getSnapshot().join('\n');
    expect(snapAfter).toContain('hello world');

    await testCase.pressEscape();
    await testCase.sleepMs(100);

    await exitCleanly(testCase);
  }, 30000);

  it('typing /cl syncs commandInputValue and does not crash', async () => {
    testCase = await launchLiteInteg('lite-smoke-slash-autocomplete');

    for (const char of '/cl') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    expect(store.commandInputValue).toBe('/cl');

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('/cl');

    await exitCleanly(testCase);
  }, 30000);
});
