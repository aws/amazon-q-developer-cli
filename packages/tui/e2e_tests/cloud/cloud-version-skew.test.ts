/**
 * Cloud-session version-skew guidance — E2E against the real TUI + real
 * published KAS server + mock BFF in MOCK_BFF_UNROUTED=1 mode.
 *
 * Replays the 07/31 production outage end-to-end: the BFF's fronting layer
 * stopped routing KiroWebBearerService, every operation came back as an empty
 * <UnknownOperationException/>, and the Smithy client flattened that to
 * "createSession: UnknownError". Until #3797 the CLI filed that under
 * cloud_error_kind=other, so the CloudSandbox-VersionSkew alarm counted
 * nothing and users got a raw "UnknownError" with no way forward.
 *
 * The unit pin for the classifier lives in cloud-error-classify.test.ts;
 * what only this test proves is the full path — real KAS parse/flatten of
 * the HTTP error shape, classification at the boot-failure handler, and the
 * user-facing guidance actually rendering — with no local-session fallback
 * masking the failure.
 *
 * Skipped on Windows like the other PTY e2e suites, and loudly when the
 * published @kiro/agent server is not installed.
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
if (kasServerMissing) {
  console.warn(
    `SKIPPING cloud E2E: published KAS server not found at ${KAS_SERVER}. ` +
      'Run ./scripts/codeartifact-login.sh && bun install to enable these tests.'
  );
}
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;

describe('cloud sessions — version-skew guidance (mock BFF, unrouted)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    'an unrouted BFF surfaces the version-skew guidance, not a bare UnknownError',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-version-skew',
        bffEnv: { MOCK_BFF_UNROUTED: '1' },
      });
      const tc = harness.testCase!;

      // The classified guidance line renders (index.tsx boot-failure handler
      // → classifyCloudError → cloudErrorGuidance('version_skew')).
      await tc.waitForText('out of sync', BOOT_TIMEOUT);

      const snapshot = tc.getSnapshotFormatted();
      // Recovery action is present.
      expect(snapshot).toContain('Update kiro');
      // The failure must not be mistaken for success: no live cloud session
      // and no silent local fallback presenting a working prompt.
      expect(snapshot).not.toContain('Cloud session created');
    },
    120_000
  );
});
