/**
 * Integration test verifying that zero-width Unicode characters are stripped
 * from input, preventing rendering mismatches (pyramid/duplicate lines).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

async function exitCleanly(tc: TestCase) {
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.sleepMs(100);
  await tc.pressCtrlC();
  await tc.expectExit();
}

describe('Zero-width character input', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('pasting path with U+200E then typing does not duplicate lines', async () => {
    testCase = await TestCase.builder()
      .withTestName('zero-width-pyramid-repro')
      .withTerminal({ width: 60, height: 20 })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Exact repro: paste a path with leading U+200E, then type '?' repeatedly
    await testCase.sendKeys('\u200Epackages/tui/src/utils/input-editing.ts');
    await testCase.sleepMs(300);

    for (let i = 0; i < 5; i++) {
      await testCase.sendKeys('?');
      await testCase.sleepMs(100);
    }
    await testCase.sleepMs(300);

    const snapshot = testCase.getSnapshot();

    // The text "input-editing.ts" should appear on exactly one line.
    // The pyramid bug causes it to appear on multiple lines.
    const matchingLines = snapshot.filter((l) =>
      l.includes('input-editing.ts')
    );
    expect(matchingLines.length).toBe(1);

    // The full text should be on screen
    const screen = snapshot.join('\n');
    expect(screen).toContain('input-editing.ts?????');

    await exitCleanly(testCase);
  }, 20000);
});
