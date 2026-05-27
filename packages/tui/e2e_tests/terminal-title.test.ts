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

    // Use /title to trigger a fresh OSC write
    await testCase.sendKeys('/title');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // The toast should show a cwd-based title proving initTerminalTitle() ran
    await testCase.waitForText('Current title: kiro:', 5000);

    // Verify an actual OSC 0 sequence was emitted (not just the toast)
    const titlesBeforeExit = extractTitleWrites(ptyOutput.join(''));
    expect(titlesBeforeExit.some((t) => t.startsWith('kiro: '))).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();

    // Verify the reset sequence (empty OSC 0) was emitted on exit
    const allTitles = extractTitleWrites(ptyOutput.join(''));
    expect(allTitles[allTitles.length - 1]).toBe('');
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

    // The most recent OSC title should no longer be the override,
    // but should still be a valid "kiro: ..." title
    const titles = extractTitleWrites(ptyOutput.join(''));
    const lastTitle = titles[titles.length - 1];
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

  it('toggling setting off mid-session clears the terminal title', async () => {
    const ptyOutput: string[] = [];

    testCase = await E2ETestCase.builder()
      .withTestName('terminal-title-disable-mid-session')
      .withTerminal({ width: 80, height: 24 })
      .withGlobalSettings({ 'chat.terminalTitle': true })
      .launch();

    testCase.onPtyData((data) => ptyOutput.push(data));
    await testCase.waitForText('ask a question', 15000);

    // Confirm title was set on boot
    await testCase.sendKeys('/title');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Current title: kiro:', 5000);

    // Open /settings display and navigate to Terminal title (5th item)
    await testCase.sendKeys('/settings display');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Terminal title', 5000);

    await testCase.sendKeys('\x1b[B');
    await testCase.sendKeys('\x1b[B');
    await testCase.sendKeys('\x1b[B');
    await testCase.sendKeys('\x1b[B');
    await testCase.sleepMs(100);

    // Toggle off
    await testCase.pressEnter();
    await testCase.sleepMs(200);

    // Close the panel
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    // Verify an empty OSC reset sequence was emitted (clears the title)
    const titles = extractTitleWrites(ptyOutput.join(''));
    const lastTitle = titles[titles.length - 1];
    expect(lastTitle).toBe('');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});
