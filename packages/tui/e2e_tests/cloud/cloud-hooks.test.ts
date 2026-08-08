/**
 * /hooks in cloud sessions — E2E against the real TUI + published KAS +
 * mock BFF.
 *
 * KAS classifies `_kiro/hooks/list` as sessionLive, so for a relayed
 * (cloud) session the CLI's /hooks fetch is FORWARDED to the sandbox over
 * the BFF SendAcpMessage op — the sandbox's hooks, not this machine's.
 * The existing cloud-panels test pins the negative bar on an unseeded
 * sandbox (no raw error, no local leak). This suite adds the two halves
 * it can't see:
 *
 *   positive round-trip: a sandbox that HAS hooks (mock answers the
 *     forwarded _kiro/hooks/list) renders them in the panel, and the
 *     request actually crossed the BFF wire                     (test 1)
 *   pre-fed local hooks: with real workspace .kiro/hooks seeded, the
 *     cloud panel still shows only sandbox rows — the pre-fed-config
 *     treatment applied to hooks (the #3690 leak class)         (test 1)
 *   local control: the same seeded workspace without --cloud lists the
 *     local hook, proving the seed is real and the scoping is what
 *     separates the two sessions                                (test 2)
 */
import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { CloudHarness } from './CloudTestCase';

const KAS_SERVER = path.join(
  __dirname,
  '../../node_modules/@kiro/agent/dist/server/acp-server.js'
);
const kasServerMissing =
  process.platform !== 'win32' && !fs.existsSync(KAS_SERVER);
if (kasServerMissing) {
  console.warn(
    `SKIPPING cloud E2E: published KAS server not found at ${KAS_SERVER}. ` +
      'Run ./scripts/codeartifact-login.sh && bun install to enable these tests.'
  );
}
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;

/** Type a line one key at a time (autocomplete-safe) and submit. */
async function typeLine(
  tc: NonNullable<CloudHarness['testCase']>,
  text: string
): Promise<void> {
  for (const ch of text) {
    await tc.sendKeys(ch);
    await tc.sleepMs(50);
  }
  await tc.sleepMs(300);
  await tc.pressEnter();
}

/**
 * Seed a workspace with one local hook. The KAS V2 hooks loader reads
 * `<root>/.kiro/hooks/*.json` in the kasHookFileSchema shape (version +
 * hooks[] with trigger/action); the probe name can only reach the screen
 * via a real local-hooks read.
 */
function seedHooksWorkspace(): string {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-hooks-ws-'));
  const hooksDir = path.join(ws, '.kiro', 'hooks');
  fs.mkdirSync(hooksDir, { recursive: true });
  fs.writeFileSync(
    path.join(hooksDir, 'local-probe.json'),
    JSON.stringify({
      version: 'v1',
      hooks: [
        {
          // Manual trigger: listed by /hooks but never fires on its own,
          // so the seed cannot inject a turn into either session.
          name: 'LOCALHOOK_PROBE',
          trigger: 'Manual',
          action: { type: 'agent', prompt: 'Say hi from the local hook.' },
        },
      ],
    })
  );
  return ws;
}

describe('cloud sessions — /hooks sandbox round-trip (mock BFF)', () => {
  let harness: CloudHarness | null = null;
  const tempDirs: string[] = [];

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
    for (const dir of tempDirs.splice(0)) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it.skipIf(skip)(
    'cloud /hooks lists the SANDBOX hooks over the relay; seeded local hooks never leak',
    async () => {
      const ws = seedHooksWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-hooks-roundtrip',
        cwd: ws,
        bffEnv: { MOCK_BFF_HOOKS: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      await typeLine(tc, '/hooks');
      // The sandbox (mock) answers the forwarded _kiro/hooks/list with its
      // hook — the panel must render it.
      await tc.waitForText('SANDBOX_HOOK_PROBE', 15_000);

      const snapshot = tc.getSnapshotFormatted();
      // The seeded LOCAL hook must not appear: cloud /hooks bypasses the
      // local-fed cache (kas-handlers/hooks.ts) and the local .kiro/hooks
      // tree belongs to a workspace the session is not running on.
      expect(snapshot).not.toContain('LOCALHOOK_PROBE');
      expect(snapshot).not.toContain('Internal error');
      expect(snapshot).not.toContain('Unable to fetch hooks');

      // The fetch actually crossed the BFF as a forwarded ext method —
      // asserted from the decoded envelope in the mock's op log.
      expect(harness.bffOutput()).toContain('_kiro/hooks/list');
      await tc.pressEscape();
    },
    120_000
  );

  it.skipIf(skip)(
    'local control: same seeded workspace without --cloud lists the local hook',
    async () => {
      const ws = seedHooksWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-hooks-local-control',
        cwd: ws,
        cliArgs: [],
        bffEnv: { MOCK_BFF_HOOKS: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('ask a question', BOOT_TIMEOUT);

      await typeLine(tc, '/hooks');
      // Local session: the KAS process loads <cwd>/.kiro/hooks itself, so
      // the seeded probe renders. This is what makes test 1's no-leak
      // assertion meaningful — same seed, opposite scoping.
      await tc.waitForText('LOCALHOOK_PROBE', 15_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('SANDBOX_HOOK_PROBE');
      expect(snapshot).not.toContain('Internal error');
      await tc.pressEscape();
    },
    120_000
  );
});
