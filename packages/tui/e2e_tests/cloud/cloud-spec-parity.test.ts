/**
 * /spec CLOUD PARITY — the SAME flow script runs in a LOCAL session and in
 * a CLOUD session (gate lifted via KIRO_TEST_SPEC_CLOUD_PARITY=1, backend
 * mocked), and the user-observable behavior is diffed: cloud must behave
 * exactly as /spec behaves locally.
 *
 * SCOPE — what "parity" means here, precisely. /spec's discovery and
 * artifact reads use `process.cwd()` in BOTH legs (runSpec), so those are
 * the same local filesystem by construction; what this suite genuinely
 * proves about the CLOUD leg is (a) the whole command surface runs
 * end-to-end with the gate lifted without any cloud-specific breakage,
 * (b) the SESSION-SCOPED side effects really cross the relay — the mode
 * switch as a forwarded session/set_config_option with a verified
 * read-back, the resume/kickoff prompts as StreamSendMessage bodies — and
 * (c) `/spec run` stays KAS-local (resolveSpecSession is classified
 * localOnly), pinned by asserting `_kiro/spec` does NOT cross the wire. A
 * sandbox-side .kiro/specs read is future work that flips assertion (c);
 * this suite documents today's split rather than proving a full remote
 * spec workflow.
 *
 * cloud-spec.test.ts pins today's user-facing contract (/spec refuses in
 * cloud). Both sessions run the identical script over identically seeded
 * workspaces: picker → artifact view → resume (C) → new+description →
 * run; a shared driver extracts NORMALIZED observations and the test
 * asserts deep equality between the two sessions.
 *
 * Model turns are OUT of the comparison: neither session has a real model
 * (the mock BFF never streams a reply; the local KAS has a fake API key),
 * so each prompt-bearing step cancels its turn immediately and only the
 * pre-turn behavior — what /spec itself does — is compared. The seam is a
 * test-only env override in runSpec; user-visible behavior is unchanged
 * without the env var (cloud-spec.test.ts proves the gate still fires).
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

type TC = NonNullable<CloudHarness['testCase']>;

/** Type a line one key at a time (autocomplete-safe) and submit. */
async function typeLine(tc: TC, text: string): Promise<void> {
  for (const ch of text) {
    await tc.sendKeys(ch);
    await tc.sleepMs(50);
  }
  await tc.sleepMs(300);
  await tc.pressEnter();
}

/** Same seeded spec as cloud-spec.test.ts — one complete feature. */
function seedSpecWorkspace(): string {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-spec-parity-'));
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
  return ws;
}

/**
 * The normalized, session-independent observations the A/B compares.
 * Everything here is what a USER would see /spec do; anything that
 * legitimately differs between session kinds (footer path vs ☁ chip,
 * timestamps, spinner rows) is deliberately not captured.
 */
interface SpecObservations {
  /** The cloud gate message appeared somewhere in the flow. */
  gateSeen: boolean;
  /** Bare /spec listed the seeded feature with its document set. */
  pickerFeatureRow: string;
  /** Artifact view: title row + the parsed task rows, exact text. */
  artifactTitleSeen: boolean;
  artifactTaskRows: string[];
  /** Resume (C from the artifact view): the continue prompt as rendered. */
  resumePromptRow: string;
  /** Footer reflects spec mode after the (verified) switch. */
  footerSpecMode: boolean;
  /** The mode switch failed (refusal / read-back mismatch). */
  failedSwitch: boolean;
  /** /spec new armed the description step (placeholder text). */
  newPlaceholderRow: string;
  /** /spec run acked with the working-autonomously toast. */
  runAckRow: string;
  /** Raw-error hygiene within the /spec interactions. */
  internalErrorSeen: boolean;
}

/** First screen row containing `needle`, whitespace-normalized. */
function rowWith(tc: TC, needle: string): string {
  const row = tc
    .getSnapshotFormatted()
    .split('\n')
    .find((r) => r.includes(needle));
  return (row ?? '').trim().replace(/\s+/g, ' ');
}

/**
 * Drive the identical /spec script and collect observations. Assumes the
 * session is booted and idle at the prompt. Every prompt-bearing step
 * cancels its turn (no model behind either session).
 */
async function driveSpecFlows(tc: TC): Promise<SpecObservations> {
  const obs: SpecObservations = {
    gateSeen: false,
    pickerFeatureRow: '',
    artifactTitleSeen: false,
    artifactTaskRows: [],
    resumePromptRow: '',
    footerSpecMode: false,
    failedSwitch: false,
    newPlaceholderRow: '',
    runAckRow: '',
    internalErrorSeen: false,
  };
  const sawGate = () =>
    tc.getSnapshotFormatted().includes('not available for a cloud session');

  // ── Picker.
  await typeLine(tc, '/spec');
  await tc.waitForText('checkout-flow', 15_000);
  obs.gateSeen ||= sawGate();
  obs.pickerFeatureRow = rowWith(tc, 'checkout-flow');
  await tc.pressEscape();
  await tc.sleepMs(1_000);

  // ── Artifact view (explicit artifact).
  await typeLine(tc, '/spec view checkout-flow tasks');
  await tc.waitForText('Wire the cart API', 15_000);
  obs.artifactTitleSeen = tc
    .getSnapshotFormatted()
    .includes('/spec view checkout-flow');
  obs.artifactTaskRows = ['Wire the cart API', 'Render order totals'].map(
    (t) => rowWith(tc, t)
  );
  // Close robustly: an Esc landing while the panel is mounting is dropped,
  // and the panel owns single-key bindings (C) that would swallow input.
  for (let i = 0; i < 5; i++) {
    await tc.pressEscape();
    await tc.sleepMs(1_000);
    if (!tc.getSnapshotFormatted().includes('esc close')) break;
  }
  await tc.waitForText('ask a question', 15_000);
  await tc.sleepMs(500);

  // ── Resume via the artifact view's C keybind (resumeSpecFeature): mode
  // switch + continue prompt.
  await typeLine(tc, '/spec checkout-flow');
  await tc.waitForText('esc close', 15_000);
  await tc.sendKeys('c');
  await tc.waitForText('Continue working on the "checkout-flow" spec', 20_000);
  obs.resumePromptRow = rowWith(tc, 'Continue working on the');
  obs.failedSwitch = tc
    .getSnapshotFormatted()
    .includes('Failed to switch to spec mode');
  // Footer row leads with the current agent name once the switch verifies.
  // Footer row leads with the current agent name; tolerate leading
  // whitespace/box glyphs (the raw ^-anchored regex missed the local
  // footer's indentation).
  obs.footerSpecMode = tc
    .getSnapshotFormatted()
    .split('\n')
    .some((row) => row.replace(/^[│\s]+/, '').startsWith('Spec'));
  // Cancel the (model-less) turn before the next command.
  await tc.sleepMs(2_000);
  await tc.pressEscape();
  await tc.waitForText('ask a question', 15_000);
  await tc.sleepMs(2_000);

  // ── New: arm the description step, then submit the description.
  await typeLine(tc, '/spec new my-widget');
  await tc.waitForText('describe what "my-widget" should do', 15_000);
  obs.newPlaceholderRow = rowWith(tc, 'describe what "my-widget"');
  await typeLine(tc, 'Track widget usage per session');
  await tc.waitForText('Track widget usage per session', 15_000);
  await tc.sleepMs(2_000);
  await tc.pressEscape();
  await tc.waitForText('ask a question', 15_000);
  await tc.sleepMs(2_000);

  // ── Run: resolveSpecSession + runAllTasks ack.
  await typeLine(tc, '/spec run checkout-flow');
  await tc.waitForText('the agent is working autonomously', 20_000);
  obs.runAckRow = rowWith(tc, 'the agent is working autonomously');

  await tc.sleepMs(2_000);
  obs.gateSeen ||= sawGate();
  obs.internalErrorSeen = tc
    .getSnapshotFormatted()
    .includes('Internal error');
  return obs;
}

describe('cloud sessions — /spec A/B parity with local (mock BFF)', () => {
  const harnesses: CloudHarness[] = [];
  const tempDirs: string[] = [];

  afterEach(async () => {
    for (const h of harnesses.splice(0)) {
      await h.cleanup();
    }
    for (const dir of tempDirs.splice(0)) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it.skipIf(skip)(
    'the identical /spec script observes identical behavior in a local session and a gate-lifted cloud session; cloud relays mode + prompts',
    async () => {
      // ── LOCAL leg: the reference behavior. Same seeded workspace shape,
      // no --cloud; /spec here is the exact production local path.
      const localWs = seedSpecWorkspace();
      tempDirs.push(localWs);
      const local = await CloudHarness.launch({
        testName: 'spec-parity-local',
        cwd: localWs,
        cliArgs: [],
      });
      harnesses.push(local);
      const ltc = local.testCase!;
      await ltc.waitForText('ask a question', BOOT_TIMEOUT);
      const localObs = await driveSpecFlows(ltc);

      // The local leg must itself be healthy — a broken reference would
      // make the equality below vacuous.
      // Every observation field is guarded here — rowWith returns '' on a
      // miss and booleans default false, so an unguarded field could make
      // the toEqual below pass on absent-in-both.
      expect(localObs.gateSeen).toBe(false);
      expect(localObs.failedSwitch).toBe(false);
      expect(localObs.pickerFeatureRow).toContain('checkout-flow');
      expect(localObs.artifactTitleSeen).toBe(true);
      for (const row of localObs.artifactTaskRows) expect(row).not.toBe('');
      expect(localObs.resumePromptRow).toContain('checkout-flow');
      expect(localObs.footerSpecMode).toBe(true);
      expect(localObs.newPlaceholderRow).toContain('my-widget');
      expect(localObs.runAckRow).not.toBe('');
      expect(localObs.internalErrorSeen).toBe(false);

      // ── CLOUD leg: same seeded workspace content, gate lifted via the
      // test seam, backend mocked.
      const cloudWs = seedSpecWorkspace();
      tempDirs.push(cloudWs);
      const cloud = await CloudHarness.launch({
        testName: 'spec-parity-cloud',
        cwd: cloudWs,
        env: { KIRO_TEST_SPEC_CLOUD_PARITY: '1' },
        bffEnv: { MOCK_BFF_SPEC_MODE: '1' },
      });
      harnesses.push(cloud);
      const ctc = cloud.testCase!;
      await ctc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await ctc.waitForText('ask a question', 20_000);
      const cloudObs = await driveSpecFlows(ctc);

      // ── THE parity assertion: cloud behaves exactly as local did — same
      // picker row, same parsed artifact rows, same continue prompt, same
      // armed placeholder, same run ack, same mode-switch success, no gate
      // and no raw error in either.
      expect(cloudObs).toEqual(localObs);

      // ── Cloud-only wire evidence: the equality above is only meaningful
      // if the cloud leg actually went over the relay rather than being
      // locally short-circuited. The mode switch crossed as a forwarded
      // set_config_option (the mock sandbox applied it and answered the
      // read-back — a refusal would have set failedSwitch and broken
      // equality), and both prompts crossed as StreamSendMessage bodies.
      expect(cloud.bffOutput()).toContain('"configId":"mode"');
      expect(cloud.bffOutput()).toContain('"value":"spec"');
      expect(cloud.bffOutput()).toContain('Continue working on the');
      expect(cloud.bffOutput()).toContain('Start a new spec called');
      expect(cloud.bffOutput()).toContain('my-widget');
      // /spec run's ext methods stay KAS-LOCAL today (resolveSpecSession
      // is classified localOnly) — the run ack in the equality above came
      // from the local KAS, not the sandbox. Pin that split: when KAS
      // starts forwarding spec ops, this flips to toContain and the suite
      // upgrades to a true remote-spec workflow check (the mock already
      // answers _kiro/spec/invoke for that day).
      expect(cloud.bffOutput()).not.toContain('_kiro/spec');
      // The local leg must NOT have touched the BFF with spec traffic —
      // its wire is the local KAS (this also guards against the local leg
      // accidentally booting cloud).
      expect(local.bffOutput()).not.toContain('"value":"spec"');
      expect(local.bffOutput()).not.toContain('Start a new spec called');
    },
    600_000
  );
});
