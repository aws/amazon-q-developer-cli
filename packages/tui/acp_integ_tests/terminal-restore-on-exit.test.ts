/**
 * Terminal modes must be restored on every exit path.
 *
 * The TUI enables focus reporting (\x1b[?1004h) and bracketed paste
 * (\x1b[?2004h) on mount. The quit paths (/quit, /exit, double-Ctrl-C) call
 * process.exit() directly, which skips React effect cleanup and beforeExit,
 * while SIGTERM/SIGHUP terminate through the signal handlers. If teardown is
 * not wired to fire on all of these, the modes stay on and leak into the
 * parent shell (e.g. \x1b[I / \x1b[O injected into the next program's stdin).
 * This drives each exit path under a real PTY and asserts the disable
 * sequences reach the terminal.
 */

import { afterEach, describe, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const DISABLE_FOCUS = '\x1b[?1004l';
const DISABLE_BRACKETED_PASTE = '\x1b[?2004l';

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

// Focus tracking is POSIX-only (it opens /dev/tty), so the disable sequence
// this suite waits for is never written on Windows, and SIGTERM/SIGHUP are
// not delivered as catchable signals there.
describe.skipIf(process.platform === 'win32')(
  'terminal restore on exit',
  () => {
    let tc: AcpTestCase | null = null;
    let cwd: string | null = null;

    afterEach(async () => {
      if (tc) await tc.cleanup();
      tc = null;
      if (cwd) rmSync(cwd, { recursive: true, force: true });
      cwd = null;
    });

    const cases: Array<{
      name: string;
      slug: string;
      quit: (tc: AcpTestCase) => Promise<void> | void;
    }> = [
      {
        name: '/quit',
        slug: 'quit',
        quit: async (t) => {
          await t.sendKeys('/quit');
          await t.pressEnter();
        },
      },
      {
        name: '/exit',
        slug: 'exit',
        quit: async (t) => {
          await t.sendKeys('/exit');
          await t.pressEnter();
        },
      },
      {
        name: 'double Ctrl-C',
        slug: 'double-ctrl-c',
        quit: (t) => t.pressCtrlCTwice(),
      },
      {
        name: 'SIGTERM',
        slug: 'sigterm',
        quit: (t) => t.sendSignal('SIGTERM'),
      },
      {
        name: 'SIGHUP',
        slug: 'sighup',
        quit: (t) => t.sendSignal('SIGHUP'),
      },
    ];

    for (const { name, slug, quit } of cases) {
      it(`restores focus and bracketed paste after ${name}`, async () => {
        cwd = mkdtempSync(join(tmpdir(), `kiro-terminal-restore-${slug}-`));
        tc = new AcpTestCase({
          testName: `terminal-restore-${slug}`,
          cwd,
          terminalSize: { width: 120, height: 40 },
        });
        setupHandshake(tc);
        await tc.launch();
        await tc.mock.awaitConnection();
        await tc.waitForVisibleText('ask a question', 10000);

        await quit(tc);

        await tc.waitForRawOutput([DISABLE_FOCUS, DISABLE_BRACKETED_PASTE]);
        await tc.expectExit();
      }, 45000);
    }
  }
);
