/**
 * Cloud file-attach E2E — regression for bug #34 ("Attempting to attach local
 * file to cloud session does not work", fixed by #3652's send-time scan).
 * See COVERAGE.md.
 *
 * The strict-parity design: the input layer is byte-identical to local; the
 * cloud gap closes at SEND time (utils/cloud-attach.ts) — outgoing text is
 * scanned for local paths and the bytes ship in-band (images as image
 * blocks, UTF-8 text as resource blocks). Unit coverage lives in
 * cloud-attach.test.ts; this E2E proves the wire: the mock BFF records the
 * StreamSendMessage payload and the test asserts the file BYTES (not the
 * dead local path alone) reached it.
 *
 *   text file path in prompt → resource bytes reach the BFF     (test 1)
 *   local control: same flow without --cloud sends no resource  (covered by
 *     the unit parity suite; E2E control omitted to keep runtime bounded)
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

describe('cloud sessions — local file attach ships bytes in-band (mock BFF)', () => {
  let harness: CloudHarness | null = null;
  let tmpFile: string | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
    if (tmpFile) {
      fs.rmSync(tmpFile, { force: true });
      tmpFile = null;
    }
  });

  it.skipIf(skip)(
    'a local text-file path in the prompt ships its bytes to the BFF (bug #34)',
    async () => {
      // A file whose CONTENT is a unique marker: seeing the marker in the
      // BFF's request log proves the bytes crossed the wire in-band (the
      // sandbox could never read the local path itself).
      tmpFile = path.join(os.tmpdir(), `cloud-attach-e2e-${Date.now()}.txt`);
      fs.writeFileSync(tmpFile, 'SECRET-MARKER-XYLOPHONE-42\n');

      harness = await CloudHarness.launch({ testName: 'cloud-attach-bytes' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of `read ${tmpFile}`) {
        await tc.sendKeys(ch);
        await tc.sleepMs(30);
      }
      await tc.pressEnter();
      // Give the send-time scan + StreamSendMessage round trip time to land.
      await tc.sleepMs(8_000);

      // The mock BFF logs every op's decoded input to stdout, which the
      // harness captured. The marker must appear in what the BFF received —
      // bytes in-band, not just the dead local path.
      const bffLog = harness.bffOutput();
      expect(bffLog).toContain('SECRET-MARKER-XYLOPHONE-42');
    },
    120_000
  );
});
