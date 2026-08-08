/**
 * /spec in cloud sessions — E2E against the real TUI + published KAS +
 * mock BFF.
 *
 * /spec is a LOCAL-WORKSPACE surface: every form of it reads `.kiro/specs/`
 * under the CLI's cwd (spec-workspace.ts), which is NOT the sandbox clone a
 * cloud session runs on. The command effect therefore refuses up front in
 * cloud sessions (effects.ts runSpec) instead of showing local specs as if
 * they were sandbox state — the same scope-mismatch class as the fixed
 * /mcp//tools panel leak (#3690) and the /chat save/load gate (bug #19).
 *
 * This suite pre-feeds a COMPLETE spec into the workspace and pins both
 * halves of that contract in one file:
 *
 *   cloud: every subcommand form refuses with the gate message and the
 *          pre-fed feature name never reaches the screen        (test 1)
 *   local control: the SAME seeded workspace without --cloud drives the
 *          real flows (picker → view → tasks)                   (test 2)
 *
 * The local control is what makes test 1 meaningful: it proves the gate is
 * scoped to cloud sessions, not a broken /spec.
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
 * Pre-feed a workspace with one complete spec feature. The document set
 * matches what the KAS spec workflow tracks (spec-workspace.ts), so in a
 * local session the feature lists as "requirements, design, tasks" and is
 * runnable — and in a cloud session there is real local state to leak if
 * the gate ever regresses.
 */
function seedSpecWorkspace(): string {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-spec-ws-'));
  const feature = path.join(ws, '.kiro', 'specs', 'checkout-flow');
  fs.mkdirSync(feature, { recursive: true });
  fs.writeFileSync(
    path.join(feature, 'requirements.md'),
    '# Requirements\n\n' +
      '## 1. Cart totals\n\n' +
      'WHEN the cart changes THE checkout page SHALL recompute totals.\n'
  );
  fs.writeFileSync(
    path.join(feature, 'design.md'),
    '# Design\n\nThe checkout flow calls the cart service over REST.\n'
  );
  fs.writeFileSync(
    path.join(feature, 'tasks.md'),
    '# Tasks\n\n' +
      '- [ ] 1. Wire the cart API\n' +
      '  - [ ] 1.1 Define the cart client\n' +
      '- [ ] 2. Render order totals\n'
  );
  // pickMostRecentArtifact breaks ties with strict `>`, so three writes in
  // the same millisecond could open requirements.md instead of tasks.md —
  // pin tasks.md as unambiguously newest.
  const future = new Date(Date.now() + 2_000);
  fs.utimesSync(path.join(feature, 'tasks.md'), future, future);
  return ws;
}

const GATE_MESSAGE = '/spec is not available for a cloud session yet.';

describe('cloud sessions — /spec gate + local control (mock BFF)', () => {
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
    'cloud: every /spec form refuses; the pre-fed local spec never leaks',
    async () => {
      const ws = seedSpecWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-spec-gate',
        cwd: ws,
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      // Every subcommand routes through the same runSpec effect, so each
      // form must hit the identical gate — none may fall through to the
      // local .kiro/specs read behind it. Same session throughout: a leak
      // caused by state from an earlier form would surface on a later one.
      const forms = [
        '/spec',
        '/spec checkout-flow',
        '/spec view checkout-flow tasks',
        '/spec new my-widget',
        '/spec run checkout-flow',
        '/spec analyze_requirements',
      ];
      for (const form of forms) {
        await typeLine(tc, form);
        await tc.waitForText(GATE_MESSAGE, 15_000);
        const snapshot = tc.getSnapshotFormatted();
        // The seeded feature name is the leak probe: with the gate in
        // place the local .kiro/specs tree is never read, so the name
        // can only appear in the text WE typed (the command line itself).
        // The echo filter matches `/spec` as a command token (followed by
        // whitespace or EOL) — a bare `/spec` substring would also filter
        // real leak rows like `.kiro/specs/checkout-flow/`.
        const leaked = snapshot
          .split('\n')
          .filter(
            (row) =>
              row.includes('checkout-flow') && !/\/spec(\s|$)/.test(row)
          );
        expect(leaked).toEqual([]);
        // Gate must not arm follow-on state: /spec new must NOT leave the
        // description-collection step armed behind the refusal.
        expect(snapshot).not.toContain('describe what "my-widget"');
        // Wait out the alert's auto-hide so the next form's waitForText
        // can't match this iteration's message.
        await tc.sleepMs(5_500);
      }

      // Nothing may have crossed the wire for any of it: no spec-mode
      // switch, no spec ext-method, no kickoff prompt.
      expect(harness.bffOutput()).not.toContain('"value":"spec"');
      expect(harness.bffOutput()).not.toContain('_kiro/spec');
      expect(harness.bffOutput()).not.toContain('Start a new spec called');

      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('Internal error');
      expect(snapshot).not.toContain('rejected by sandbox');
    },
    300_000
  );

  it.skipIf(skip)(
    'local control: same seeded workspace without --cloud drives the real /spec flows',
    async () => {
      const ws = seedSpecWorkspace();
      tempDirs.push(ws);
      // Same env (mock BFF endpoint exported, KAS engine) but NO --cloud:
      // the session stays local, so the gate must not fire and the picker
      // must list the pre-fed feature. This is the control that proves
      // test 1 pinned a cloud-scoped gate, not a globally broken /spec.
      harness = await CloudHarness.launch({
        testName: 'cloud-spec-local-control',
        cwd: ws,
        cliArgs: [],
      });
      const tc = harness.testCase!;
      await tc.waitForText('ask a question', BOOT_TIMEOUT);

      // Bare /spec: picker lists the seeded feature with its document set.
      await typeLine(tc, '/spec');
      await tc.waitForText('checkout-flow', 15_000);
      await tc.waitForText('requirements, design, tasks', 15_000);
      let snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain(GATE_MESSAGE);

      // Select the highlighted feature: routes through `/spec view
      // checkout-flow` into the artifact panel, which opens on the most
      // recent artifact (tasks.md, seeded last) and renders the parsed
      // task list from the pre-fed file.
      await tc.pressEnter();
      await tc.waitForText('Wire the cart API', 15_000);
      snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toContain('/spec view checkout-flow');

      // Close the artifact panel and give the input a beat to settle —
      // keys typed while a panel is still tearing down land in its filter.
      await tc.pressEscape();
      await tc.sleepMs(1_000);

      // Unknown feature still errors honestly against the local tree.
      await typeLine(tc, '/spec view no-such-feature');
      await tc.waitForText('No spec found at .kiro/specs/no-such-feature/', 15_000);

      snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('Internal error');
    },
    180_000
  );
});
