/**
 * Cloud-session /config panel with cloud-sourced data — E2E against the real
 * TUI + real published KAS server + mock BFF (see CloudTestCase.ts).
 *
 * `MOCK_BFF_CLOUD_CONFIG=1` makes the mock BFF relay dummy cloud config
 * notifications (MCP servers, steering documents, powers, hooks — each
 * carrying the #2141 ConfigResource descriptor with origin 'cloud') on the
 * LoadSession downlink, so resuming a cloud session populates the /config
 * pages exactly as a real cloud replica would.
 *
 * Skipped on Windows and without the published @kiro/agent server, matching
 * the other cloud suites.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { CloudHarness, MOCK_SPACE_IDS } from './CloudTestCase';

const KAS_SERVER = path.join(
  __dirname,
  '../../node_modules/@kiro/agent/dist/server/acp-server.js'
);
const kasServerMissing =
  process.platform !== 'win32' && !fs.existsSync(KAS_SERVER);
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;

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

describe('cloud sessions — /config panel with cloud config (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    '/config table and steering page show the relayed cloud config',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-config-panel',
        cliArgs: ['--resume-id', MOCK_SPACE_IDS.empty],
        bffEnv: { MOCK_BFF_CLOUD_CONFIG: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('ask a question', BOOT_TIMEOUT);
      await tc.sleepMs(2_000);

      await typeCommand(tc, '/config');
      await tc.waitForText('Category', 15_000);
      await tc.waitForText('cloud', 15_000);
      // Row-scoped for precision, but NOT descriptor-proving: in a cloud
      // session the placement fallback also reads 'cloud' on every surface
      // (effectiveSource applies uniformly), so this pins the rendered
      // experience, not descriptor plumbing. The descriptor-only path is
      // pinned by the unit tests (config-panel-model: explicit source wins
      // over placement in a LOCAL session).
      const tableSnapshot = tc.getSnapshotFormatted();
      const steeringRow = tableSnapshot
        .split('\n')
        .find((line) => line.includes('steering'));
      expect(steeringRow).toBeDefined();
      expect(steeringRow).toContain('cloud');

      // Drill into the steering page via the typed subcommand: the two
      // relayed documents must be listed with a cloud source.
      await tc.pressEscape();
      await tc.sleepMs(500);
      await typeCommand(tc, '/config steering');
      await tc.waitForText('team-conventions', 15_000);
      await tc.waitForText('api-guidelines', 15_000);

      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('team-conventions');
      expect(snapshot).toContain('cloud');
    },
    120_000
  );

  it.skipIf(skip)(
    '/mcp shows the relayed cloud servers with a cloud Source column',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-config-mcp-source',
        cliArgs: ['--resume-id', MOCK_SPACE_IDS.empty],
        bffEnv: { MOCK_BFF_CLOUD_CONFIG: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('ask a question', BOOT_TIMEOUT);
      await tc.sleepMs(2_000);

      await typeCommand(tc, '/mcp');
      await tc.waitForText('github', 15_000);
      await tc.waitForText('aws-docs', 15_000);

      // Row-scoped, but placement fallback also yields 'cloud' here — this
      // pins the rendered Source column, not the descriptor (see above).
      const snapshot = tc.getSnapshotFormatted();
      const githubRow = snapshot
        .split('\n')
        .find((line) => line.includes('github'));
      expect(githubRow).toBeDefined();
      expect(githubRow).toContain('cloud');
    },
    120_000
  );
});
