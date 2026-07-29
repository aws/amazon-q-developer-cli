/**
 * Cloud-session /mcp and /tools panels (PR #3690) — E2E against the real TUI
 * + real published KAS server + mock BFF (see CloudTestCase.ts).
 *
 * Pins the readiness model at the customer-visible level: in a cloud session
 * the panels must never show this machine's configuration. The mock BFF
 * never relays a sandbox mcp/tools snapshot, so the panels must hold the
 * awaiting notice (not local rows, not an empty-table default).
 *
 * Skipped on Windows and without the published @kiro/agent server, matching
 * cloud-sessions.test.ts.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { CloudHarness } from './CloudTestCase';

const KAS_SERVER = path.join(
  __dirname,
  '../../node_modules/@kiro/agent/dist/server/acp-server.js'
);
const kasServerMissing =
  process.platform !== 'win32' && !fs.existsSync(KAS_SERVER);
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;
const AWAITING_NOTICE = 'Cloud sandbox configuration not yet received';

async function typeCommand(
  tc: NonNullable<CloudHarness['testCase']>,
  cmd: string
): Promise<void> {
  for (const ch of cmd) {
    await tc.sendKeys(ch);
    await tc.sleepMs(50);
  }
  await tc.pressEnter();
}

describe('cloud sessions — /mcp and /tools panels (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    '/mcp in a cloud session shows the awaiting notice, never local servers',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-mcp-panel' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      await typeCommand(tc, '/mcp');
      await tc.waitForText(AWAITING_NOTICE, 15_000);

      const snapshot = tc.getSnapshotFormatted();
      // The local KAS in this harness runs with the repo cwd's MCP config;
      // none of it may leak into a cloud session's panel. McpPanel status
      // labels are running/loading/failed — any of them means a local row.
      expect(snapshot).toContain(AWAITING_NOTICE);
      expect(snapshot).not.toMatch(/\brunning\b|\bloading\b|\bauth-required\b/);
    },
    120_000
  );

  it.skipIf(skip)(
    '/tools in a cloud session shows the awaiting notice, never local tools',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-tools-panel' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      await typeCommand(tc, '/tools');
      await tc.waitForText(AWAITING_NOTICE, 15_000);

      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain(AWAITING_NOTICE);
      // Builtin tool rows (read/write/shell) are the giveaway that the local
      // process's tool registry leaked into the cloud panel.
      expect(snapshot).not.toMatch(/\bread_file\b|\bexecute_bash\b/);
    },
    120_000
  );
});
