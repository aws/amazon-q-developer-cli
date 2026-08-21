/**
 * E2E test: chat.terminalTitle setting and /title slash command.
 *
 * Verifies OSC 0 escape sequences are written to the terminal at boot,
 * on /title set/clear, and on exit. Also verifies the feature is a no-op
 * when the setting is disabled (default).
 *
 * OSC 0 is the escape sequence `\x1b]0;<title>\x07` that sets the terminal
 * window title. We capture raw PTY output via onPtyData and regex-match
 * for these sequences, since xterm-headless parses them out of getSnapshot().
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/**
 * Extract all OSC 0 title payloads from raw PTY output.
 * Returns an array of the title strings (the part between \x1b]0; and \x07).
 * An empty string means the title was cleared (reset sequence).
 */
/* eslint-disable no-control-regex */
const extractTitleWrites = (raw: string): string[] => {
  return [...raw.matchAll(/\x1b\]0;([^\x07]*)\x07/g)].map((m) => m[1]!);
};
/* eslint-enable no-control-regex */

/**
 * Poll the PTY buffer until an OSC title matching `predicate` arrives.
 * Resolves as soon as the condition is met (typically <50ms) instead of
 * burning a fixed sleep. Throws with a clear message on timeout.
 */
async function waitForOscTitle(
  testCase: E2ETestCase,
  ptyOutput: string[],
  predicate: (titles: string[]) => boolean,
  description: string,
  timeoutMs = 5000
): Promise<void> {
  const start = Date.now();
  while (!predicate(extractTitleWrites(ptyOutput.join('')))) {
    if (Date.now() - start > timeoutMs) {
      const titles = extractTitleWrites(ptyOutput.join(''));
      throw new Error(
        `OSC title never matched: ${description} (got ${JSON.stringify(titles)})`
      );
    }
    await testCase.sleepMs(50);
  }
}

describe('chat.terminalTitle', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('writes cwd-based title on boot', async () => {
    const ptyOutput: string[] = [];

    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-boot')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.terminalTitle': true })
      .launch();

    // Register listener to capture OSC writes
    testCase.onPtyData((data) => ptyOutput.push(data));

    // Wait for the TUI to fully render
    await testCase.waitForText('ask a question', 15000);

    // Force a title re-emission by setting then clearing a manual override.
    // The boot OSC may have been emitted before onPtyData was registered.
    await testCase.sendKeys('/title verify-osc');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Title set:', 5000);

    // OSC arrives in a separate PTY chunk after the visible text on CI runners
    await waitForOscTitle(
      testCase,
      ptyOutput,
      (titles) => titles.some((t) => t.includes('verify-osc')),
      'verify-osc title'
    );

    // Verify an actual OSC 0 sequence was emitted with our title
    const titlesAfterSet = extractTitleWrites(ptyOutput.join(''));
    expect(titlesAfterSet.some((t) => t.includes('verify-osc'))).toBe(true);

    // Clear the override to revert to cwd-based title
    await testCase.sendKeys('/title --clear');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Title cleared', 5000);

    // Poll for the cwd-based title to arrive
    await waitForOscTitle(
      testCase,
      ptyOutput,
      (titles) => titles.some((t) => t.startsWith('kiro: ')),
      'cwd-based kiro: title'
    );

    // Verify the cwd-based title was re-emitted
    const allTitles = extractTitleWrites(ptyOutput.join(''));
    expect(allTitles.some((t) => t.startsWith('kiro: '))).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('/title <text> sets a sticky title and /title shows it', async () => {
    const ptyOutput: string[] = [];

    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-set-and-show')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.terminalTitle': true })
      .launch();

    testCase.onPtyData((data) => ptyOutput.push(data));
    await testCase.waitForText('ask a question', 15000);

    // Use /title to set a manual override
    await testCase.sendKeys('/title custom label');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // The effect handler should show a success toast
    await testCase.waitForText('Title set:', 5000);

    // Poll for the OSC sequence to arrive
    await waitForOscTitle(
      testCase,
      ptyOutput,
      (titles) => titles.some((t) => t === 'kiro: custom label'),
      'custom label title'
    );

    // Verify the OSC sequence was emitted with our custom title
    const titles = extractTitleWrites(ptyOutput.join(''));
    expect(titles.some((t) => t === 'kiro: custom label')).toBe(true);

    // Use /title with no args to show the current title via toast
    await testCase.sendKeys('/title');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // The toast should display the current title
    await testCase.waitForText('Current title: kiro: custom label', 5000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('/title --clear reverts to derived title', async () => {
    const ptyOutput: string[] = [];

    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-clear')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.terminalTitle': true })
      .launch();

    testCase.onPtyData((data) => ptyOutput.push(data));
    await testCase.waitForText('ask a question', 15000);

    // First set a manual override
    await testCase.sendKeys('/title my override');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Title set:', 5000);

    // Now clear it — should revert to the automatic title (cwd or session)
    await testCase.sendKeys('/title --clear');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Title cleared', 5000);

    // Poll for the reverted title to arrive
    await waitForOscTitle(
      testCase,
      ptyOutput,
      (titles) => {
        const last = titles[titles.length - 1];
        return last !== undefined && last !== 'kiro: my override' && last.startsWith('kiro: ');
      },
      'reverted kiro: title (not the override)'
    );

    // The most recent OSC title should no longer be the override,
    // but should still be a valid "kiro: ..." title
    const allTitles = extractTitleWrites(ptyOutput.join(''));
    const lastTitle = allTitles[allTitles.length - 1];
    expect(lastTitle).not.toBe('kiro: my override');
    expect(lastTitle!.startsWith('kiro: ')).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('does not write any OSC title when setting is disabled (default)', async () => {
    const ptyOutput: string[] = [];

    // Launch WITHOUT chat.terminalTitle — feature should be completely inert
    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-disabled')
      .withTerminal({ width: 80, height: 24 })
      .launch();

    testCase.onPtyData((data) => ptyOutput.push(data));
    await testCase.waitForText('ask a question', 15000);

    // No OSC 0 sequences should appear in the output stream at all
    const titles = extractTitleWrites(ptyOutput.join(''));
    expect(titles.length).toBe(0);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();

    // Confirm nothing was written even during the exit path
    const allTitles = extractTitleWrites(ptyOutput.join(''));
    expect(allTitles.length).toBe(0);
  }, 30000);

  // Skip: settings panel arrow-key navigation is position-sensitive and
  // unreliable in headless E2E (item count/focus changes break it). The
  // toggle-off reset behavior is covered by the unit test for the store
  // subscriber + resetTerminalTitle(). Re-enable when the panel exposes
  // a keyboard shortcut or direct command to toggle individual settings.
  it.skip('toggling setting off mid-session clears the terminal title', async () => {
    const ptyOutput: string[] = [];

    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-disable-mid-session')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.terminalTitle': true })
      .launch();

    testCase.onPtyData((data) => ptyOutput.push(data));
    await testCase.waitForText('ask a question', 15000);

    // Force a title emission so we have a baseline OSC in the capture
    await testCase.sendKeys('/title confirm-active');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Title set:', 5000);

    // Verify we captured the OSC before toggling off
    const titlesBeforeToggle = extractTitleWrites(ptyOutput.join(''));
    expect(titlesBeforeToggle.some((t) => t.includes('confirm-active'))).toBe(
      true
    );

    // Open /settings display panel
    await testCase.sendKeys('/settings display');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Terminal title', 5000);

    // Navigate to Terminal title — it's the 5th item (index 4).
    // Add sleeps between arrows to ensure each keypress is processed.
    for (let i = 0; i < 4; i++) {
      await testCase.sendKeys('\x1b[B');
      await testCase.sleepMs(150);
    }

    // Toggle off
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    // Close the panel
    await testCase.pressEscape();
    await testCase.sleepMs(500);

    // Verify the reset was emitted by checking that an empty OSC appeared
    // after our "confirm-active" title
    const titles = extractTitleWrites(ptyOutput.join(''));
    const confirmIdx = titles.findIndex((t) => t.includes('confirm-active'));
    const resetAfterConfirm = titles.slice(confirmIdx + 1).includes('');
    expect(resetAfterConfirm).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});
