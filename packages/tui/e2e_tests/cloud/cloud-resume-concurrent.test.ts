/**
 * Cloud-session resume trajectory + concurrent sessions (batch 2) — E2E
 * against the real TUI + real published KAS server + mock BFF.
 *
 * Batch 1 (cloud-sessions.test.ts) covered boot, the provider gate, the repo
 * picker, detach, and quit. This suite covers what needed a turn-stream-replay
 * mock BFF:
 *
 *   resume with history: --resume-id <cloud-id> replays the canned two-turn
 *            transcript (user + agent messages, completed tool call) and
 *            lights the cloud surface                              (test 1)
 *   resume auto-detects cloud: no --cloud flag needed — the resume
 *            target's cloud row flips the launch to cloud mode      (test 1)
 *   resume isolation: resuming the EMPTY session while another
 *            session's history exists replays nothing (the mock keys
 *            history by session id, so a wrong-session replay fails) (test 2)
 *   concurrent sessions: /sessions lists multiple cloud rows with
 *            row-scoped environment + status columns                (test 3)
 *   switching: filter-typing in the picker selects a specific other
 *            session; the switch replays THAT session's distinct
 *            transcript and not the first session's                 (test 4)
 *
 * The mock BFF serves LoadSession as a real AWS event stream (lean-dialect
 * frames + per-turn `done` frames + the `session_loaded` sentinel), which is
 * exactly the shape a suspended-MDE resume is served in production, so the
 * KAS-side fold (reconstruction, orphan closure, dedup) runs for real.
 *
 * Skipped on Windows like the other PTY e2e suites, and loudly when the
 * published @kiro/agent server is not installed.
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

// Cloud boots and resumes do BFF round-trips per milestone; generous but bounded.
const BOOT_TIMEOUT = 60_000;

describe('cloud sessions — resume + concurrent (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    '--resume-id <cloud-id> without --cloud replays history and lights the cloud surface',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-resume-history',
        // No --cloud: the resume resolution must flip cloud mode on by itself
        // from the cloud row the merged listing returns.
        cliArgs: ['--resume-id', MOCK_SPACE_IDS.banana],
        bffEnv: { MOCK_BFF_HISTORY: '1' },
      });
      const tc = harness.testCase!;

      // Resume-flow wording (UX fix in #3656): the checklist step reads
      // "Resuming cloud session…" / "✓ Cloud session resumed" — never the
      // create-flow "Creating"/"created" wording that confused testers into
      // thinking resume had made a NEW session.
      await tc.waitForText('Cloud session resumed', BOOT_TIMEOUT);

      // History replay: every replayed string is awaited (not just asserted
      // on a later snapshot) so a slow tool-row render can't flake this.
      await tc.waitForText('clone the repo and list the files', BOOT_TIMEOUT);
      await tc.waitForText('Shell git clone banana-service', 20_000);
      await tc.waitForText('Repo cloned: 120 files at HEAD.', 20_000);
      await tc.waitForText('now add a health check endpoint', 20_000);
      await tc.waitForText('Added GET /health returning 200 OK.', 20_000);
      // Prompt is usable after the replay (the session_loaded sentinel closed
      // the fold cleanly).
      await tc.waitForText('ask a question', 20_000);

      const snapshot = tc.getSnapshotFormatted();
      // The resumed session is cloud: the persistent cloud chip renders.
      expect(snapshot).toContain('☁');
      // The completed tool replays as completed — never as Cancelled/
      // interrupted (bug #2's regression shape).
      expect(snapshot).not.toContain('Cancelled');
      expect(snapshot).not.toContain('interrupted');
      // Second turn arrived after the first (ordering survived the fold).
      // Both strings were awaited above, so neither indexOf can read -1 and
      // make this vacuous.
      expect(
        snapshot.indexOf('clone the repo and list the files')
      ).toBeLessThan(snapshot.indexOf('now add a health check endpoint'));
    },
    180_000
  );

  it.skipIf(skip)(
    '--resume-id of an empty cloud session comes up cloud without another session’s history',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-resume-empty',
        cliArgs: ['--cloud', '--resume-id', MOCK_SPACE_IDS.empty],
        // MOCK_BFF_HISTORY stays ON so the banana transcript EXISTS in the
        // mock — the empty space's LoadSession must still serve only the
        // sentinel (the mock keys history by session id). This makes the
        // no-leak assertion real: a wrong-session replay would fail it.
        bffEnv: { MOCK_BFF_HISTORY: '1' },
      });
      const tc = harness.testCase!;

      await tc.waitForText('ask a question', BOOT_TIMEOUT);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('☁');
      // Nothing replayed: another session's canned history must NOT leak
      // into this empty session's scrollback.
      expect(snapshot).not.toContain('clone the repo and list the files');
      expect(snapshot).not.toContain('refactor the payments retry logic');
    },
    120_000
  );

  it.skipIf(skip)(
    '/sessions lists concurrent cloud sessions alongside the active one',
    async () => {
      // NOTE ON STATUS: KAS's remote listing (spaceToSummary; re-verified on
      // the pinned 0.26.14 bundle) carries
      // no per-space execution status — every remote row reads `idle`; live
      // working/waiting statuses exist only for the actively-attached session.
      // So this asserts the concurrent rows, their cloud tags, and the status
      // column's presence — not per-row live states. When KAS starts stamping
      // per-space status on the listing, tighten this to assert
      // working/waiting (the mock's GetSessionStatus already serves them —
      // see sessionStatusFor in mock-bff.mjs).
      harness = await CloudHarness.launch({
        testName: 'cloud-concurrent-list',
        bffEnv: { MOCK_BFF_CONCURRENT: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/sessions') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.pressEnter();

      // All concurrent cloud sessions appear in the picker at once.
      await tc.waitForText('refactor payments', 30_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('migrate database');
      expect(snapshot).toContain('New cloud sandbox');
      // Row-scoped column assertions: the environment + status cells render
      // ON each concurrent row (a bare toContain('cloud') would be satisfied
      // by the titles alone). The picker row is a single line: title …
      // environment … status.
      expect(snapshot).toMatch(/refactor payments.*cloud.*idle/);
      expect(snapshot).toMatch(/migrate database.*cloud.*idle/);
    },
    120_000
  );

  it.skipIf(skip)(
    'picking another cloud session from /sessions loads it and replays its history',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-switch-session',
        bffEnv: { MOCK_BFF_CONCURRENT: '1', MOCK_BFF_HISTORY: '1' },
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/sessions') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.pressEnter();
      await tc.waitForText('refactor payments', 30_000);

      // Select the "refactor payments" session DETERMINISTICALLY via the
      // picker's incremental search (SessionPickerPanel filters rows on typed
      // text), rather than trusting sort order of same-timestamp rows.
      for (const ch of 'refactor') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.sleepMs(300);
      await tc.pressEnter();

      // Switching replays THAT session's distinct transcript — the mock keys
      // canned history by session id, so the banana transcript appearing here
      // instead would be a wrong-session replay and fails the assertion.
      await tc.waitForText('refactor the payments retry logic', 30_000);
      await tc.waitForText(
        'Extracted RetryPolicy from PaymentsClient.',
        20_000
      );
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('clone the repo and list the files');
      // Still a cloud surface after the switch.
      expect(snapshot).toContain('☁');
    },
    150_000
  );

  // ── SKIP-UNTIL-PR #3700: A→B→A reload replays A's history incl. user rows ──
  // Guards the "Cloud A→B→A re-load didn't re-replay" KAS bug (fixed KAS-side
  // f78f0e590) and #3700's user-row replay: after switching away and back, the
  // original session's FULL transcript — including the user's own messages —
  // re-renders.
  it.skip('SKIP-UNTIL-PR(#3700) switching A→B→A re-replays A’s history with user rows', async () => {
    harness = await CloudHarness.launch({
      testName: 'cloud-aba-reload',
      cliArgs: ['--cloud', '--resume-id', MOCK_SPACE_IDS.banana],
      bffEnv: { MOCK_BFF_HISTORY: '1', MOCK_BFF_CONCURRENT: '1' },
    });
    const tc = harness.testCase!;
    await tc.waitForText('Repo cloned: 120 files at HEAD.', BOOT_TIMEOUT);
    await tc.waitForText('ask a question', 20_000);

    // A → B (switch to refactor payments via picker search).
    for (const ch of '/sessions') {
      await tc.sendKeys(ch);
      await tc.sleepMs(50);
    }
    await tc.sleepMs(300);
    await tc.pressEnter();
    await tc.waitForText('refactor payments', 30_000);
    for (const ch of 'refactor') {
      await tc.sendKeys(ch);
      await tc.sleepMs(50);
    }
    await tc.sleepMs(300);
    await tc.pressEnter();
    await tc.waitForText('refactor the payments retry logic', 30_000);

    // B → A (switch back to banana).
    for (const ch of '/sessions') {
      await tc.sendKeys(ch);
      await tc.sleepMs(50);
    }
    await tc.sleepMs(300);
    await tc.pressEnter();
    await tc.waitForText('banana-service', 30_000);
    for (const ch of 'banana') {
      await tc.sendKeys(ch);
      await tc.sleepMs(50);
    }
    await tc.sleepMs(300);
    await tc.pressEnter();

    // A's history re-renders — the USER rows too (bug: user rows were
    // dropped on re-load), and B's transcript is gone.
    await tc.waitForText('clone the repo and list the files', 30_000);
    await tc.waitForText('Repo cloned: 120 files at HEAD.', 20_000);
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('now add a health check endpoint');
    expect(snapshot).not.toContain('refactor the payments retry logic');
  }, 240_000);
});
