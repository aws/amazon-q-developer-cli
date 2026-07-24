/**
 * The Kitty keyboard protocol stack must be balanced on exit.
 *
 * The protocol enable (`CSI > 1 u`) is a stack PUSH on the terminal's
 * keyboard-mode stack; `CSI < u` pops one entry. If the TUI pushes more
 * entries than it pops — e.g. by re-pushing on every resize — leftover
 * entries keep the enhanced protocol active in the parent shell after exit,
 * where Ctrl+C arrives as `CSI 99;5u` instead of 0x03 and the shell becomes
 * uninterruptible.
 *
 * A PTY cannot report the emulated terminal's mode state, but the state is
 * fully determined by the byte stream, so this suite replays every kitty
 * push/pop from the raw captured output (via the harness's getKittyStack)
 * and asserts net stack depth is zero once the process has exited.
 * TERM_PROGRAM=WezTerm forces the known-kitty force-enable path; PTY resizes
 * exercise the mode re-assert path.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

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

// Keyboard-protocol negotiation and signal-based exits are POSIX-only.
describe.skipIf(process.platform === 'win32')(
  'kitty keyboard stack balance on exit',
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
      resize: boolean;
      quit: (tc: AcpTestCase) => Promise<void> | void;
    }> = [
      {
        name: '/quit after resizes',
        slug: 'quit-resized',
        resize: true,
        quit: async (t) => {
          await t.sendKeys('/quit');
          await t.pressEnter();
        },
      },
      {
        name: '/quit without resizes',
        slug: 'quit-plain',
        resize: false,
        quit: async (t) => {
          await t.sendKeys('/quit');
          await t.pressEnter();
        },
      },
      {
        name: 'double Ctrl-C after resizes',
        slug: 'double-ctrl-c-resized',
        resize: true,
        quit: (t) => t.pressCtrlCTwice(),
      },
      {
        name: 'SIGTERM after resizes',
        slug: 'sigterm-resized',
        resize: true,
        quit: (t) => t.sendSignal('SIGTERM'),
      },
      {
        name: 'SIGHUP after resizes',
        slug: 'sighup-resized',
        resize: true,
        quit: (t) => t.sendSignal('SIGHUP'),
      },
    ];

    for (const { name, slug, resize, quit } of cases) {
      it(`leaves the kitty stack empty after ${name}`, async () => {
        cwd = mkdtempSync(join(tmpdir(), `kiro-kitty-stack-${slug}-`));
        tc = new AcpTestCase({
          testName: `kitty-stack-${slug}`,
          cwd,
          terminalSize: { width: 120, height: 40 },
          extraEnv: { TERM_PROGRAM: 'WezTerm' },
        });
        setupHandshake(tc);
        await tc.launch();
        await tc.mock.awaitConnection();
        await tc.waitForVisibleText('ask a question', 10000);

        if (resize) {
          tc.resize(121, 40);
          await tc.sleepMs(200);
          tc.resize(120, 40);
          await tc.sleepMs(200);
        }

        await quit(tc);

        await tc.waitForRawOutput(['\x1b[<u']);
        await tc.expectExit();

        const { pushes, pops, depth } = tc.getKittyStack();
        // The protocol must actually have been negotiated, otherwise the
        // balance assertion is vacuous.
        expect(pushes).toBeGreaterThanOrEqual(1);
        // Exact push/pop balance: depth alone bottoms out at zero, so an
        // over-pop (destroying a host process's stack entries) would hide.
        expect(pops).toBe(pushes);
        expect(depth).toBe(0);
      }, 45000);
    }
  }
);
