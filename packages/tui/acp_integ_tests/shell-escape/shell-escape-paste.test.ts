/**
 * Bracketed paste must work after a TTY shell escape.
 *
 * Reproduces the bug where `!vim` (or `!less`) resets bracketed paste mode
 * and the TUI never re-enables it. When the user pastes after that, the
 * terminal sends raw text WITHOUT bracketed paste markers, so each newline
 * is treated as Enter and the input is auto-submitted line-by-line.
 *
 * Verifies the TUI re-enables bracketed paste after a TTY shell escape by
 * checking the raw PTY output for the re-enable sequence (\x1b[?2004h)
 * after vim's alt screen exit.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from '../shared/AcpTestCase';
import { defaultKasModes } from '../shared/default-agent';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: { sessionCapabilities: {} },
  }));
  tc.mock.on('session/new', () => ({
    sessionId: 's1',
    modes: defaultKasModes(),
  }));
}

describe('shell escape bracketed paste', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    cwd = null;
  });

  it('re-enables bracketed paste after !vim exits', async () => {
    cwd = mkdtempSync(join(tmpdir(), 'kiro-shell-paste-'));
    tc = new AcpTestCase({
      testName: 'paste-after-shell-escape',
      cwd,
      terminalSize: { width: 120, height: 40 },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Run !vim -c q — opens vim and immediately quits.
    // vim disables bracketed paste on exit.
    await tc.sendKeys('!vim -c q');
    await tc.pressEnter();
    await tc.sleepMs(1500);

    // Wait for prompt to return
    await tc.waitForVisibleText('ask a question', 10000);

    // The TTY shell escape path writes \x1b[?1049l (leave alt screen)
    // after vim exits. Bracketed paste (\x1b[?2004h) must be re-enabled
    // AFTER that point. Find the last alt-screen-exit in the raw PTY
    // output and check that a bracketed-paste-enable follows it.
    const allOutput = tc.getOutput();
    const altScreenExit = allOutput.lastIndexOf('\x1b[?1049l');
    expect(altScreenExit).toBeGreaterThan(-1); // sanity: vim used alt screen

    const afterAltExit = allOutput.slice(altScreenExit);
    expect(afterAltExit.includes('\x1b[?2004h')).toBe(true);
  }, 45000);
});
