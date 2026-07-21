/**
 * Cloud-session interactive flows (batch 1) — E2E against the real TUI +
 * real published KAS server + mock BFF (see CloudTestCase.ts).
 *
 * Coverage map:
 *   --cloud boot → connect checklist + ☁ Cloud footer            (test 1)
 *   no source provider → gate with Kiro Web handoff              (test 2)
 *   /repo interactive picker lists provider repos                (test 3)
 *   --repo <name> skips the picker, binds the footer,
 *            and does NOT clone before the first message         (test 4)
 *   /disconnect → "work continues" + reattach hint               (test 5)
 *   /quit → keep-running prompt with both options                (test 6)
 *   Opt-in: same launch WITHOUT --cloud stays a local session    (test 7)
 *
 * The headless surfaces (--list-sessions / --delete-session) and the
 * released-build rollout gate are covered by
 * `crates/chat-cli/tests/cloud_sessions_gating.rs`. Resume trajectory and
 * concurrent sessions need a mock BFF that replays a live turn stream —
 * batch 2.
 *
 * Skipped on Windows like the other PTY e2e suites. Also skipped (loudly)
 * when the published @kiro/agent server is not installed, so the rest of the
 * e2e suite still runs for contributors without CodeArtifact access.
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

// Cloud boots do a BFF round-trip per milestone; generous but bounded.
const BOOT_TIMEOUT = 60_000;

describe('cloud sessions — interactive flows (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    '`chat --cloud` boots a cloud session (checklist + Cloud footer)',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-boot' });
      const tc = harness.testCase!;

      // Connect checklist milestones, in order of appearance.
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      // Provider catalog drained: repo count row (mock serves 3 repos).
      await tc.waitForText('repositories found', 20_000);
      // The persistent cloud location chip (icon-prefixed, so this cannot be
      // satisfied by the "Cloud session created" checklist line above).
      await tc.waitForText('☁', 10_000);
      // Prompt bar is usable.
      await tc.waitForText('ask a question', 20_000);

      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('Cloud session created');
      // The local cwd/git-branch pair must NOT render for a cloud session —
      // the sandbox is the session's location.
      expect(snapshot).not.toContain(process.cwd());
    },
    120_000
  );

  it.skipIf(skip)(
    'no source provider connected → gate with Kiro Web setup handoff',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-provider-gate',
        bffEnv: { MOCK_BFF_NO_PROVIDER: '1' },
      });
      const tc = harness.testCase!;

      // The gate replaces the chat: explains the requirement and offers the
      // browser handoff (mock returns kiro.dev/settings/source-providers).
      await tc.waitForText('source provider', BOOT_TIMEOUT);
      const snapshot = tc.getSnapshotFormatted();
      // Setup URL (or an open-browser affordance) must be presented.
      expect(
        snapshot.includes('kiro.dev') ||
          snapshot.toLowerCase().includes('browser')
      ).toBe(true);
      // No chat prompt while gated.
      expect(snapshot).not.toContain('ask a question');
    },
    120_000
  );

  it.skipIf(skip)(
    '/repo opens the interactive picker listing provider repos',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-repo-picker' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/repo') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.pressEnter();

      // Picker rows come from the mock catalog.
      await tc.waitForText('banana-service', 30_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('apple-service');
      expect(snapshot).toContain('cherry-service');
    },
    120_000
  );

  it.skipIf(skip)(
    '--repo binds the footer, skips the picker, defers the clone',
    async () => {
      harness = await CloudHarness.launch({
        testName: 'cloud-repo-flag',
        cliArgs: ['--cloud', '--repo', 'kiro-team/banana-service'],
      });
      const tc = harness.testCase!;

      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      const snapshot = tc.getSnapshotFormatted();
      // Footer location shows the bound repo (basename path form).
      expect(snapshot).toContain('banana-service');
      // No interactive picker appeared.
      expect(snapshot).not.toContain('space to toggle');
      // Deferred clone: nothing clones before the first message.
      expect(snapshot.toLowerCase()).not.toContain('cloning');
    },
    120_000
  );

  it.skipIf(skip)(
    '/disconnect detaches, prints "work continues" + reattach hint',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-disconnect' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/disconnect') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.pressEnter();

      await tc.waitForText("Your work continues while you're away", 30_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('Quit session');
      // The reattach hint names the resume flag.
      expect(snapshot).toContain('--resume-id');
    },
    120_000
  );

  it.skipIf(skip)(
    '/quit prompts keep-running vs turn-off; keep-running detaches',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-quit-prompt' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/quit') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.pressEnter();

      // The cloud quit prompt with both choices.
      await tc.waitForText('continue working', 30_000);
      const prompt = tc.getSnapshotFormatted();
      expect(prompt).toContain('Yes (agent continues)');
      expect(prompt).toContain('No (agent stops)');

      // Select keep-running (first option).
      await tc.pressEnter();
      await tc.waitForText("Your work continues while you're away", 30_000);
    },
    120_000
  );

  it.skipIf(skip)(
    'opt-in: same env WITHOUT --cloud boots a plain local session',
    async () => {
      // Same KAS engine + endpoint env — only the flag differs. NOTE: the
      // harness runs with KIRO_TEST_MODE=1 (rollout force-ON), so this proves
      // the OPT-IN semantic (feature on + no flag + stray endpoint env =
      // local session), NOT the released rollout gate. The gate itself is
      // proven by the release-profile tests in
      // crates/chat-cli/tests/cloud_sessions_gating.rs.
      harness = await CloudHarness.launch({
        testName: 'cloud-dark-local',
        cliArgs: [],
      });
      const tc = harness.testCase!;

      await tc.waitForText('ask a question', BOOT_TIMEOUT);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('Cloud session created');
      expect(snapshot).not.toContain('repositories found');
      expect(snapshot).not.toContain('☁');
    },
    120_000
  );
});
