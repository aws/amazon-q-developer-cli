/**
 * /autonomous on|off in a cloud session — E2E against the real TUI +
 * published KAS + mock BFF.
 *
 * KAS 0.27.8 forwards `session/set_mode` (and the CLI's read-back via
 * `session/set_config_option`) for relayed sessions to the sandbox over the
 * BFF's SendAcpMessage op; the mock BFF plays the sandbox's core, applying
 * the mode and answering the read-back with matching configOptions. So this
 * suite pins the FULL verified-switch path end-to-end:
 *
 *   bare /autonomous opens the on/off picker with [current] tagged   (test 1)
 *   /autonomous on verifies + confirms; footer shows Autonomous chip (test 2)
 *   idempotence: on-when-on / off-when-off say "already"             (test 2)
 *   /autonomous off switches back and clears the chip                (test 2)
 *   the switch actually crossed the wire (SendAcpMessage in BFF log) (test 2)
 *
 * The pre-relay failure shape (KAS silently no-ops set_mode, the CLI's
 * read-back catches it and surfaces the honest "not supported on this
 * session yet" message) is covered by the kas.ts unit suite; with 0.27.8+
 * pinned in package.json the E2E path is the applied-switch path.
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

/** Type a slash command one key at a time (autocomplete-safe) and submit. */
async function typeCommand(
  tc: NonNullable<CloudHarness['testCase']>,
  command: string
): Promise<void> {
  for (const ch of command) {
    await tc.sendKeys(ch);
    await tc.sleepMs(50);
  }
  // Let the autocomplete settle so Enter submits the typed text rather than
  // accepting a different completion.
  await tc.sleepMs(300);
  await tc.pressEnter();
}

describe('cloud sessions — /autonomous mode switch (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    'bare /autonomous opens the on/off picker with the current state tagged',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-autonomous-picker' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      await typeCommand(tc, '/autonomous');

      // The picker offers both options; the fresh session is off, so the
      // [current] tag must sit on the `off` row. Pin the row shape (option
      // label + tag on ONE row — `[ \t]` not `\s`, which would span newlines
      // and let an unrelated line ending in "on" match against a following
      // line starting with "[current]") so a tag on the wrong row fails.
      await tc.waitForText('[current]', 15_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toMatch(/off[ \t]+\[current\]/);
      expect(snapshot).not.toMatch(/on[ \t]+\[current\]/);
      await tc.pressEscape();
    },
    120_000
  );

  it.skipIf(skip)(
    '/autonomous on verifies the applied switch; off reverts; both are idempotent',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-autonomous-onoff' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      // ON: the success line only prints AFTER the read-back verification
      // (kas.ts setSessionMode) confirms the sandbox applied the mode — so
      // its presence is evidence of a real round-trip, not a blind echo.
      await typeCommand(tc, '/autonomous on');
      await tc.waitForText('Autonomous mode on', 15_000);
      let snapshot = tc.getSnapshotFormatted();
      // A failed verification surfaces the honest not-supported message —
      // with the 0.27.8 relay + mock sandbox that would be a regression.
      expect(snapshot).not.toContain('not supported on this session yet');
      expect(snapshot).not.toContain('Failed to switch autonomous mode');
      // (The Autonomous footer chip is not asserted from the snapshot: the
      // word appears in the success message itself, so a substring check
      // would be vacuous. Chip rendering from the store's current agent is
      // pinned by the status-surface unit tests.)

      // The switch must have crossed the BFF wire as a forwarded core verb
      // WITH the autonomous payload. The modeId is asserted as the JSON pair
      // from the decoded envelope log — a bare 'autonomous' substring would be
      // vacuous (the test's own workspace path contains the word).
      expect(harness.bffOutput()).toContain('session/set_mode');
      expect(harness.bffOutput()).toContain('"modeId":"autonomous"');

      // Idempotent ON.
      await typeCommand(tc, '/autonomous on');
      await tc.waitForText('Autonomous mode is already on', 15_000);

      // OFF: same verified path back to the default agent.
      await typeCommand(tc, '/autonomous off');
      await tc.waitForText('Autonomous mode off', 15_000);
      snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('not supported on this session yet');

      // Idempotent OFF.
      await typeCommand(tc, '/autonomous off');
      await tc.waitForText('Autonomous mode is already off', 15_000);

      // No-error hygiene across the whole flow (mirrors the KR invariant).
      snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('Internal error');
      expect(snapshot).not.toContain('rejected by sandbox');
    },
    180_000
  );

  it.skipIf(skip)(
    'toggle notice renders below an idle turn body, in wall-clock order',
    async () => {
      // Regression (#3821): a system notice emitted while the previous turn
      // was idle-but-uncommitted used to commit straight to scrollback ABOVE
      // the turn body still rendering in the dynamic region — "Autonomous
      // mode on" appeared folded into past conversation. Pin the true order:
      // turn body first, then the later notice, surviving the next turn's
      // commit, with the notice on screen exactly once.
      harness = await CloudHarness.launch({ testName: 'cloud-autonomous-order' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      // A prompt whose turn goes idle without committing: the mock BFF acks
      // the submit but never streams a done frame down the tail, so cancel
      // (Esc) leaves the turn idle-but-uncommitted — the bug's precondition.
      await typeCommand(tc, 'MARKER_TURN_BODY please respond');
      await tc.sleepMs(3_000);
      await tc.pressEscape();
      await tc.waitForText('ask a question', 15_000);

      await typeCommand(tc, '/autonomous on');
      await tc.waitForText('Autonomous mode on', 15_000);

      // Screen-row order: the notice must sit BELOW the turn body.
      const rowOf = (rows: string[], text: string): number =>
        rows.findIndex((row) => row.includes(text));
      let rows = tc.getSnapshot();
      let bodyRow = rowOf(rows, 'MARKER_TURN_BODY');
      let noticeRow = rowOf(rows, 'Autonomous mode on');
      expect(bodyRow).toBeGreaterThanOrEqual(0);
      expect(noticeRow).toBeGreaterThan(bodyRow);

      // Order must hold after the next turn commits everything to scrollback.
      await typeCommand(tc, 'MARKER_SECOND_TURN follow-up');
      await tc.sleepMs(3_000);
      await tc.pressEscape();
      await tc.waitForText('MARKER_SECOND_TURN', 15_000);

      rows = tc.getSnapshot();
      bodyRow = rowOf(rows, 'MARKER_TURN_BODY');
      noticeRow = rowOf(rows, 'Autonomous mode on');
      const secondRow = rowOf(rows, 'MARKER_SECOND_TURN');
      expect(bodyRow).toBeGreaterThanOrEqual(0);
      expect(noticeRow).toBeGreaterThan(bodyRow);
      expect(secondRow).toBeGreaterThan(noticeRow);
      // Exactly once on the real screen grid (xterm rendering, so unlike the
      // unit harness there is no dynamic/static double paint to discount).
      const noticeCount = rows.filter((row) =>
        row.includes('Autonomous mode on')
      ).length;
      expect(noticeCount).toBe(1);
    },
    180_000
  );
});
