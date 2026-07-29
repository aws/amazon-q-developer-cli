/**
 * Repo-picker interaction E2E — regression coverage for bug #28 ("tab to
 * switch panels" + interactive Selected section, merged in #3656) and the
 * pink/white checkmark accent nit. See COVERAGE.md.
 *
 *   Tab moves focus to the Selected panel; arrows + space act there  (test 1)
 *   Space in Selected panel unchecks the repo (returns to unselected) (test 1)
 *   Selecting repos marks them with the accent checkmark              (test 2)
 *
 * Real TUI + published KAS + mock BFF (3 provider repos served).
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
const TAB = '\t';
const SPACE = ' ';

describe('cloud sessions — repo picker interactions (mock BFF)', () => {
  let harness: CloudHarness | null = null;

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
  });

  it.skipIf(skip)(
    'Tab focuses the Selected panel where space unchecks a repo (bug #28)',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-picker-tab' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/repo') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.sleepMs(300);
      await tc.pressEnter();
      await tc.waitForText('banana-service', 30_000);

      // Select the first repo (cursor starts on it). The panel headers carry
      // live counts — Selected(N) / All(3) — which make unambiguous pins.
      await tc.sendKeys(SPACE);
      await tc.sleepMs(300);
      let snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('Selected(1)');
      // The checked row shows its mark in the main list.
      expect(snapshot).toMatch(/\[✓\].*banana-service/);

      // Tab moves focus to the Selected panel; space there UNCHECKS (bug #28:
      // pre-fix, the Selected panel was inert and Tab did nothing).
      await tc.sendKeys(TAB);
      await tc.sleepMs(300);
      await tc.sendKeys(SPACE);
      await tc.sleepMs(300);

      snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('Selected(0)');
      // The main-list row reads unchecked again.
      expect(snapshot).toMatch(/\[ \].*banana-service/);
    },
    120_000
  );

  it.skipIf(skip)(
    'selecting repos marks them checked in the main list',
    async () => {
      harness = await CloudHarness.launch({ testName: 'cloud-picker-check' });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      for (const ch of '/repo') {
        await tc.sendKeys(ch);
        await tc.sleepMs(50);
      }
      await tc.sleepMs(300);
      await tc.pressEnter();
      await tc.waitForText('banana-service', 30_000);

      await tc.sendKeys(SPACE);
      await tc.sleepMs(300);

      const snapshot = tc.getSnapshotFormatted();
      // A checked row renders its mark inside the bracket group on the same
      // line as the repo (the ACCENT is color, which the text snapshot can't
      // see — the mark's presence in-bracket is the regression pin), and the
      // other repos stay unchecked.
      expect(snapshot).toMatch(/\[✓\].*banana-service/);
      expect(snapshot).toMatch(/\[ \].*apple-service/);
    },
    120_000
  );
});
