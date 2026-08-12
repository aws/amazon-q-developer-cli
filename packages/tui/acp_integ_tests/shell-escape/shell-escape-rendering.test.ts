import { describe, it, expect, afterEach } from 'bun:test';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from '../../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from '../shared/default-agent';

describe('shell escape rendering', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    cwd = null;
  });

  it('clears "Running..." after a redirected command with no output', async () => {
    cwd = mkdtempSync(join(tmpdir(), 'kiro-shell-escape-'));
    tc = new AcpTestCase({
      testName: 'shell-escape-redirect',
      cwd,
      extraEnv: { SHELL: '/bin/bash' },
    });
    tc.mock.on('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: { sessionCapabilities: {} },
    }));
    tc.mock.on('session/new', () => ({
      sessionId: 's1',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Redirect keeps the PTY silent (the regression under test) while the
    // marker file proves the command ran. The command finishes in
    // milliseconds, so polling for the transient isShellEscape=true state
    // is racy; wait on durable signals only.
    const marker = join(cwd, 'ran.txt');
    await tc.sendKeys(`!echo hi > ${marker}`);
    await tc.pressEnter();

    const start = Date.now();
    while (!existsSync(marker)) {
      if (Date.now() - start > 10000) throw new Error('command never ran');
      await tc.sleepMs(50);
    }
    await tc.waitForStore((s) => s.isShellEscape === false, 10000);
    await tc.sleepMs(200);

    expect(tc.getSnapshot().join('\n')).not.toContain('Running...');
  }, 30000);
});
