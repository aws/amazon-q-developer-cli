/**
 * Subagent (invoke_sub_agent) rendering in cloud sessions — E2E against the
 * real TUI + published KAS + mock BFF.
 *
 * The sandbox KAS registers `invoke_sub_agent` as its top-level delegation
 * tool. On replay, KAS's fold maps the persisted payload's `toolName` to the
 * ACP `title` with no `_meta` — so the CLI receives the meta-stripped
 * "Sub-agent: <role>" shape. The cloud-only InvokeSubagentPipelineAdapter
 * (#3657 port) claims that card and renders it through the orchestrate
 * pipeline path instead of a flat, unlabeled tool row (bug #12's flattened
 * view). This suite pins the cloud rendering end-to-end over a canned
 * replay containing a completed delegation:
 *
 *   the delegation renders identifiably (role visible, not a bare
 *     "Sub-agent:" flat row) and lands as completed, never Cancelled (test 1)
 *   surrounding turns replay intact around the subagent turn        (test 1)
 *
 * A LIVE delegation (streaming stage updates, crew monitor interaction)
 * needs an agent actually running turns — prod-smoke scope; the mock never
 * runs a model. The adapter's claim/labeling logic is unit-pinned in
 * invoke-subagent-pipeline.test.ts; this E2E pins the wiring: relay fold →
 * adapter enablement on a resumed cloud session → transcript rendering.
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
if (kasServerMissing) {
  console.warn(
    `SKIPPING cloud E2E: published KAS server not found at ${KAS_SERVER}. ` +
      'Run ./scripts/codeartifact-login.sh && bun install to enable these tests.'
  );
}
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;

describe('cloud sessions — subagent view on replay (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    'replayed invoke_sub_agent delegation renders with its role, completed, amid intact turns',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-subagent-replay',
        cliArgs: ['--cloud', '--resume-id', MOCK_SPACE_IDS.banana],
        bffEnv: { MOCK_BFF_HISTORY: '1', MOCK_BFF_SUBAGENT: '1' },
      });
      const tc = harness.testCase!;

      // The canned transcript replays in order; the subagent turn is last.
      // Every needle used in the ordering assertion below is waited on
      // first — indexOf returns -1 on a miss, which would satisfy
      // toBeLessThan and make the ordering check pass on ABSENCE.
      await tc.waitForText('clone the repo and list the files', BOOT_TIMEOUT);
      await tc.waitForText('now add a health check endpoint', 20_000);
      await tc.waitForText('audit the API layer for gaps', 20_000);
      await tc.waitForText('Audit complete: 2 handlers need auth added.', 20_000);
      await tc.waitForText('ask a question', 20_000);

      const snapshot = tc.getSnapshotFormatted();

      // The delegation is identifiable by its role — the adapter derives
      // the agent name from the "Sub-agent: <role>" title / args.name, so
      // a flat unlabeled tool row (bug #12's flattened view) fails here.
      expect(snapshot).toContain('api-auditor');

      // Completed means completed: the replayed delegation must never
      // resurface as Cancelled/interrupted (bug #2's regression shape
      // applied to the subagent card).
      expect(snapshot).not.toContain('Cancelled');
      expect(snapshot).not.toContain('interrupted');

      // The turns around the delegation replayed intact and in order. Both
      // needles were waited on above, so neither indexOf can be -1 — but
      // the snapshot is re-taken here, so guard against the earlier turn
      // having scrolled out between the wait and the read.
      const earlierTurnAt = snapshot.indexOf('now add a health check endpoint');
      const subagentTurnAt = snapshot.indexOf('audit the API layer for gaps');
      expect(earlierTurnAt).toBeGreaterThanOrEqual(0);
      expect(subagentTurnAt).toBeGreaterThanOrEqual(0);
      expect(earlierTurnAt).toBeLessThan(subagentTurnAt);

      // No-error hygiene (mirrors the KR invariant).
      expect(snapshot).not.toContain('Internal error');
      expect(snapshot).not.toContain('rejected by sandbox');
    },
    180_000
  );
});
