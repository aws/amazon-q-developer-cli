import { afterEach, describe, it } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

describe('shell escape (!command)', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
    if (cwd) {
      rmSync(cwd, {
        recursive: true,
        force: true,
        maxRetries: 5,
        retryDelay: 100,
      });
    }
    cwd = null;
  });

  // The `!` escape runs entirely locally in the TUI; no ACP prompt handler
  // is needed. A clean tmpdir cwd keeps shell-init noise (e.g. mise
  // warnings) off the PTY so output assertions stay meaningful. SHELL is
  // pinned to bash because the interactive tests use bash-only syntax and
  // the escape spawns `$SHELL -c`.
  async function launch(
    testName: string,
    terminalSize: { width: number; height: number } = { width: 80, height: 24 }
  ): Promise<AcpTestCase> {
    cwd = mkdtempSync(join(tmpdir(), 'kiro-shell-escape-'));
    tc = new AcpTestCase({
      testName,
      cwd,
      terminalSize,
      extraEnv: { SHELL: '/bin/bash' },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    return tc;
  }

  // The typed command stays visible on screen after submit, so a plain
  // substring wait can match the command line instead of real output.
  // Poll for a screen line matching the predicate to avoid that.
  async function waitForLine(
    t: AcpTestCase,
    predicate: (line: string) => boolean,
    timeoutMs = 10000
  ): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      if (t.getSnapshot().some(predicate)) return;
      await t.sleepMs(50);
    }
    throw new Error(
      `Timeout waiting for matching line. Screen was:\n${t.getSnapshot().join('\n')}`
    );
  }

  it('executes !echo and shows output', async () => {
    const t = await launch('shell-escape-echo');

    await t.sendKeys('!echo hello_shell');
    await t.sleepMs(100);
    await t.pressEnter();

    // Output line, not the echoed command line
    await waitForLine(
      t,
      (l) => l.includes('hello_shell') && !l.includes('echo')
    );

    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('runs the command in the session cwd', async () => {
    const t = await launch('shell-escape-cwd');
    // A marker file in the tmpdir cwd; `!ls` output showing it proves the
    // command ran in the session cwd. The tmpdir path itself is unusable
    // as a discriminator: it wraps across screen lines and also appears
    // in the status bar.
    writeFileSync(join(cwd!, 'cwd-marker-file.txt'), '');

    await t.sendKeys('!ls');
    await t.sleepMs(100);
    await t.pressEnter();

    await waitForLine(t, (l) => l.includes('cwd-marker-file.txt'));

    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('handles empty ! gracefully', async () => {
    const t = await launch('shell-escape-empty');

    await t.sendKeys('!');
    await t.sleepMs(100);
    await t.pressEnter();

    // Should return to prompt without error
    await t.sleepMs(500);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('shows error for failed commands', async () => {
    const t = await launch('shell-escape-fail');

    await t.sendKeys('!false');
    await t.sleepMs(100);
    await t.pressEnter();

    // The output is durable proof that the fast command started, so the
    // following false-state wait cannot match the initial idle state.
    await waitForLine(t, (l) => l.includes('[exit code: 1]'));
    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('executes long-running command', async () => {
    const t = await launch('shell-escape-long');

    await t.sendKeys('!sleep 1 && echo done');
    await t.sleepMs(100);
    await t.pressEnter();

    // Output line, not the echoed command line (which also contains "done")
    await waitForLine(t, (l) => l.includes('done') && !l.includes('sleep'));

    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('handles command with spaces', async () => {
    const t = await launch('shell-escape-spaces');

    await t.sendKeys('!echo "hello world"');
    await t.sleepMs(100);
    await t.pressEnter();

    await waitForLine(
      t,
      (l) => l.includes('hello world') && !l.includes('echo')
    );

    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.waitForVisibleText('ask a question', 10000);
  }, 30000);

  // Skip on linux CI — `read -p` in shell escape has a known PTY forwarding
  // issue on ubuntu runners where the echoed output doesn't appear. The
  // "multiple lines of interactive input" test covers the same code path
  // and passes reliably on all platforms.
  (process.platform === 'linux' || process.platform === 'win32' ? it.skip : it)(
    'accepts interactive input via read',
    async () => {
      const t = await launch('shell-escape-interactive', {
        width: 120,
        height: 30,
      });

      const cmd =
        process.platform === 'win32'
          ? '!$prompt = "Name"; $name = Read-Host $prompt; Write-Output "Hello $name"'
          : '!prompt=Name; read -p "${prompt}: " name && echo "Hello $name"';
      await t.sendKeys(cmd);
      await t.sleepMs(200);
      await t.pressEnter();

      // Should see the prompt from read
      await t.waitForVisibleText('Name:', 30000);

      // Type the response
      await t.sendKeys('Kiro');
      await t.sleepMs(200);
      await t.pressEnter();

      // Should see the echoed greeting
      await t.waitForVisibleText('Hello Kiro', 30000);

      await t.waitForStore((s) => s.isShellEscape === false, 15000);
      await t.waitForVisibleText('ask a question', 15000);
    },
    90000
  );

  it('Ctrl-C cancels a running shell escape command', async () => {
    const t = await launch('shell-escape-ctrlc', { width: 120, height: 30 });

    await t.sendKeys('!sleep 30');
    await t.sleepMs(200);
    await t.pressEnter();

    // Wait for the command to start running
    await t.waitForStore((s) => s.isShellEscape === true, 5000);
    await t.sleepMs(500);

    await t.pressCtrlC();

    // Should return to prompt (not exit Kiro)
    await t.waitForStore((s) => s.isShellEscape === false, 20000);
    await t.waitForVisibleText('ask a question', 20000);
  }, 60000);

  (process.platform === 'win32' ? it.skip : it)(
    'accepts multiple lines of interactive input',
    async () => {
      // Wider terminal prevents the long command from wrapping and
      // interfering with prompt detection in the screen buffer.
      const t = await launch('shell-escape-multi-input', {
        width: 120,
        height: 30,
      });

      // Use variables for prompt strings so the literal prompt text we wait
      // for doesn't appear in the typed command (which stays visible on
      // screen and would cause the wait to match prematurely).
      const cmd =
        process.platform === 'win32'
          ? '!$a = Read-Host "Prompt1"; $b = Read-Host "Prompt2"; Write-Output "$a and $b"'
          : '!P=Prompt; read -p "${P}1: " a && read -p "${P}2: " b && echo "$a and $b"';
      await t.sendKeys(cmd);
      await t.sleepMs(200);
      await t.pressEnter();

      // First prompt only appears when the first read actually runs
      await t.waitForVisibleText('Prompt1:', 20000);
      await t.sendKeys('foo');
      await t.sleepMs(200);
      await t.pressEnter();

      await t.waitForVisibleText('Prompt2:', 20000);
      await t.sendKeys('bar');
      await t.sleepMs(200);
      await t.pressEnter();

      // Combined output only appears if both reads captured input
      await t.waitForVisibleText('foo and bar', 20000);

      await t.waitForStore((s) => s.isShellEscape === false, 15000);
      await t.waitForVisibleText('ask a question', 15000);
    },
    60000
  );
});
