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

describe('shell escape output collapse', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    cwd = null;
  });

  // Clean tmpdir cwd keeps shell-init noise off the PTY; stray warning
  // lines would change the line count the collapse threshold depends on.
  async function launch(testName: string): Promise<AcpTestCase> {
    cwd = mkdtempSync(join(tmpdir(), 'kiro-shell-collapse-'));
    tc = new AcpTestCase({
      testName,
      cwd,
      terminalSize: { width: 80, height: 24 },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    return tc;
  }

  it('collapses long shell output to last 5 lines', async () => {
    const t = await launch('shell-output-collapse');

    // Generate output exceeding HEAD_LINES(5) + tailLines(max(5, height-10)=14) = 19 lines
    await t.sendKeys('!seq 1 30');
    await t.sleepMs(100);
    await t.pressEnter();

    // Collapse hint appears while output is still active
    await t.waitForVisibleText('lines hidden', 5000);

    await t.waitForVisibleText('ask a question', 5000);
  }, 30000);

  it('does not collapse short shell output', async () => {
    const t = await launch('shell-output-no-collapse');

    // 3 lines of output, below the collapse threshold
    await t.sendKeys('!seq 1 3');
    await t.sleepMs(100);
    await t.pressEnter();

    await t.waitForStore((s) => s.isShellEscape === false, 10000);
    await t.sleepMs(500);

    const snapshotText = t.getSnapshot().join('\n');
    expect(snapshotText).not.toContain('lines hidden');
    expect(snapshotText).not.toContain('ctrl+o');

    await t.waitForVisibleText('ask a question', 5000);
  }, 30000);
});
