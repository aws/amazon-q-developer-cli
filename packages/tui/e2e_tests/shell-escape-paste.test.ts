/**
 * E2E test: bracketed paste must work after a TTY shell escape.
 *
 * Reproduces the bug where `!vim` (or `!less`) resets bracketed paste mode
 * and the TUI never re-enables it. When the user pastes after that, the
 * terminal sends raw text WITHOUT bracketed paste markers, so each newline
 * is treated as Enter and the input is auto-submitted line-by-line.
 *
 * The test verifies that the TUI re-enables bracketed paste after a TTY
 * shell escape by checking the PTY output for the re-enable sequence
 * (\x1b[?2004h) after vim's alt screen exit.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Bracketed paste after shell escape', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('re-enables bracketed paste after !vim exits', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-after-shell-escape')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Capture PTY output to check for the re-enable sequence
    const ptyOutput: string[] = [];
    testCase.onPtyData((data) => ptyOutput.push(data));

    // Run !vim -c q — opens vim and immediately quits.
    // vim disables bracketed paste on exit.
    await testCase.sendKeys('!vim -c q');
    await testCase.pressEnter();
    await testCase.sleepMs(1500);

    // Wait for prompt to return
    await testCase.waitForText('ask a question', 10000);

    // executeShellEscapeTTY writes \x1b[?1049l (leave alt screen) after
    // vim exits. The fix should re-enable bracketed paste (\x1b[?2004h)
    // AFTER that point. Find the last alt-screen-exit and check that
    // a bracketed-paste-enable follows it.
    const allOutput = ptyOutput.join('');
    const altScreenExit = allOutput.lastIndexOf('\x1b[?1049l');
    expect(altScreenExit).toBeGreaterThan(-1); // sanity: vim used alt screen

    const afterAltExit = allOutput.slice(altScreenExit);
    const reEnabled = afterAltExit.includes('\x1b[?2004h');
    expect(reEnabled).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 45000);
});
