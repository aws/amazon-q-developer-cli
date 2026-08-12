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

import { afterEach, describe, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from '../../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from '../shared/default-agent';

const ALT_SCREEN_EXIT = '\x1b[?1049l';
const ENABLE_BRACKETED_PASTE = '\x1b[?2004h';

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

async function waitForTerminalModeRestore(
  tc: AcpTestCase,
  outputStart: number,
  timeoutMs = 10000
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const commandOutput = tc.getOutput().slice(outputStart);
    const altScreenExit = commandOutput.lastIndexOf(ALT_SCREEN_EXIT);
    if (
      altScreenExit >= 0 &&
      commandOutput
        .slice(altScreenExit + ALT_SCREEN_EXIT.length)
        .includes(ENABLE_BRACKETED_PASTE)
    ) {
      return;
    }
    await tc.sleepMs(50);
  }
  throw new Error('terminal modes were not restored after the shell escape');
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
    const outputStart = tc.getOutput().length;
    await tc.sendKeys('!vim -c q');
    await tc.pressEnter();

    // The TTY shell escape path writes \x1b[?1049l (leave alt screen)
    // after vim exits. Bracketed paste (\x1b[?2004h) must be re-enabled
    // after that point. Poll the raw PTY output because the prompt remains
    // visible while the synchronous child process is still exiting.
    await waitForTerminalModeRestore(tc, outputStart);
    await tc.waitForVisibleText('ask a question', 10000);
  }, 45000);
});
