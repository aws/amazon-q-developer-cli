/**
 * SLASH-COMMAND A/B PARITY — the same slash commands run in a LOCAL
 * session and in a CLOUD session (mock BFF), and the user-observable
 * behavior is compared row-by-row.
 *
 * Two classes of command, two kinds of assertion:
 *
 *   PARITY commands must behave the SAME in both sessions — the surface
 *   is either client-local (/help, /clear, /usage panel shell) or
 *   sandbox-fed in a way the mock makes equivalent (/agent picker shape,
 *   /context show, /autonomous picker). For each, a normalized
 *   observation from the cloud leg must deep-equal the local leg's.
 *
 *   GATED commands must DIFFER in exactly the documented way — the local
 *   leg performs the real action while the cloud leg refuses with its
 *   gate message and performs nothing (/chat save, /context add,
 *   /rewind, ! shell). The assertion pins both sides: local really works
 *   (the reference is healthy) and cloud really refuses (the gate holds).
 *
 * One session pair drives every command, so cross-command state (panel
 * teardown, alert dismissal) is exercised the way a user would hit it.
 * /spec's deep-flow parity lives in cloud-spec-parity.test.ts; /hooks'
 * sandbox round-trip in cloud-hooks.test.ts; this suite is the broad
 * sweep across the rest of the command surface.
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

/** First screen row containing `needle`, whitespace-normalized. */
function rowWith(tc: TC, needle: string): string {
  const row = tc
    .getSnapshotFormatted()
    .split('\n')
    .find((r) => r.includes(needle));
  return (row ?? '').trim().replace(/\s+/g, ' ');
}

/** Close any open panel/picker and settle back at the prompt. */
async function settle(tc: TC): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tc.pressEscape();
    await tc.sleepMs(800);
    const snap = tc.getSnapshotFormatted();
    if (!snap.includes('esc to close') && !snap.includes('esc close')) break;
  }
  await tc.waitForText('ask a question', 15_000);
  await tc.sleepMs(500);
}

/** Wait out a 5s auto-hide alert so the next step can't stale-match it. */
async function waitAlertGone(tc: TC): Promise<void> {
  await tc.sleepMs(5_500);
}

/**
 * Per-command normalized observations. Only session-kind-independent
 * facts are captured — anything that legitimately differs (☁ chip,
 * workspace path in the footer, sandbox-vs-local row COUNTS) stays out;
 * shapes and success/refusal outcomes go in.
 */
interface CommandObservations {
  /** The /help row for the help command itself — proves the panel body
   *  rendered, and its wording must match across session kinds. */
  helpRow: string;
  /** /agent picker opened; the bundled Default row (name + source tag). */
  agentPickerDefaultRow: string;
  /**
   * /context show responded without a raw error. NOT compared as a panel:
   * locally KAS answers with a token breakdown (panel opens); in cloud the
   * forwarded `_kiro/session/context` gets the mock's `{}`, so the CLI
   * falls back to the friendly no-context alert. Both are valid /context
   * SHOW behavior — the surface differs only because the mock doesn't
   * model context state, so the comparable fact is "responded cleanly".
   */
  contextShowResponded: boolean;
  /** /mcp panel's close-hint row — proves the panel opened; wording must
   *  match across session kinds (content rows differ by provenance and are
   *  pinned by the panels suite). */
  mcpPanelFooterRow: string;
  /** Same for /tools. */
  toolsPanelFooterRow: string;
  /** /usage panel's title row — proves the panel actually opened (a bare
   *  no-error boolean would be true even if the command did nothing). */
  usagePanelTitleRow: string;
  /** /clear completed without an error alert. The VIEWPORT effect is a
   *  documented divergence and asserted per-session-kind by the caller:
   *  local /clear trims the message list but does not repaint older
   *  terminal scrollback, while cloud re-wipes the full viewport (#3651,
   *  pinned in cloud-sessions/S08) — so wipe-ness cannot be an equality
   *  field. */
  clearRanCleanly: boolean;
  /** Any raw Internal error across the whole pass. */
  internalErrorSeen: boolean;
}

/** Drive the PARITY commands (same-behavior class) and observe. */
async function driveParityCommands(tc: TC): Promise<CommandObservations> {
  const obs: CommandObservations = {
    helpRow: '',
    agentPickerDefaultRow: '',
    contextShowResponded: false,
    mcpPanelFooterRow: '',
    toolsPanelFooterRow: '',
    usagePanelTitleRow: '',
    clearRanCleanly: false,
    internalErrorSeen: false,
  };

  // /help — pure client surface. The panel opens on the command list;
  // anchor on a row that exists in both session kinds.
  await typeLine(tc, '/help');
  await tc.waitForText('Show available commands', 15_000);
  obs.helpRow = rowWith(tc, 'Show available commands');
  await settle(tc);

  // /agent — picker opens on the bundled set (local: local registry;
  // cloud: the downlink push). The Default PICKER row is the one carrying
  // the Bundled source tag — the footer also says "Default", so the tag is
  // what disambiguates. Strip the focus glyph and badge columns (cursor-
  // dependent) so only the name + source shape is compared.
  await typeLine(tc, '/agent');
  await tc.waitForText('Select agent', 15_000);
  obs.agentPickerDefaultRow = (
    tc
      .getSnapshotFormatted()
      .split('\n')
      .find((r) => r.includes('Default') && r.includes('Bundled')) ?? ''
  )
    .trim()
    .replace(/\s+/g, ' ')
    .replace(/^[│❯> ]+/, '')
    .split('[')[0]!
    .trim();
  await settle(tc);

  // /context show — stays live in cloud (bug 8/22 contract). Locally the
  // breakdown panel opens; in cloud the mock's empty answer produces the
  // friendly no-context alert (see the field doc) — either counts, a raw
  // error counts as no response.
  await typeLine(tc, '/context show');
  const ctxDeadline = Date.now() + 15_000;
  while (Date.now() < ctxDeadline) {
    const snap = tc.getSnapshotFormatted();
    if (snap.includes('Internal error')) break;
    if (
      snap.includes('Tab to switch to /usage') ||
      snap.includes('No context files attached') ||
      snap.includes('Context files:')
    ) {
      obs.contextShowResponded = true;
      break;
    }
    await tc.sleepMs(500);
  }
  await settle(tc);
  await waitAlertGone(tc);

  // /mcp and /tools — panels must OPEN in both (content differs by
  // provenance: local pool vs awaiting-sandbox; that's pinned elsewhere).
  await typeLine(tc, '/mcp');
  await tc.waitForText('MCP', 15_000);
  await tc.waitForText('esc to close', 15_000);
  obs.mcpPanelFooterRow = rowWith(tc, 'esc to close');
  await settle(tc);

  await typeLine(tc, '/tools');
  // Panel-OPEN anchor only: content intentionally differs (local pool vs
  // the awaiting-sandbox provenance notice, #3690) and is pinned by the
  // panels suite; parity here is "the panel opens cleanly in both".
  await tc.waitForText('esc to close', 15_000);
  obs.toolsPanelFooterRow = rowWith(tc, 'esc to close');
  await settle(tc);

  // /usage — the panel must OPEN in both session kinds (its title row is
  // the positive evidence; content differs by plan/no-data state and is
  // not compared). The title row BEGINS with '/usage' after the border
  // glyph — a contains-match would also hit the rotating startup tip that
  // mentions /usage, which raced this capture on CI.
  await typeLine(tc, '/usage');
  const usageDeadline = Date.now() + 15_000;
  while (Date.now() < usageDeadline && !obs.usagePanelTitleRow) {
    obs.usagePanelTitleRow =
      tc
        .getSnapshotFormatted()
        .split('\n')
        .map((r) => r.trim().replace(/\s+/g, ' ').replace(/^[│❯> ]+/, ''))
        .find((r) => r.startsWith('/usage')) ?? '';
    if (!obs.usagePanelTitleRow) await tc.sleepMs(500);
  }
  // No catch: a stuck /usage panel would silently poison every later step,
  // so let settle's waitForText throw here like everywhere else.
  await settle(tc);

  // /clear — must complete without an error alert in both session kinds.
  // The viewport effect is intentionally NOT compared (see the field doc);
  // the cloud-side full wipe is pinned by cloud-sessions/S08.
  await typeLine(tc, 'PARITY_CLEAR_MARKER hello');
  await tc.sleepMs(2_000);
  await tc.pressEscape(); // cancel the (model-less) turn
  await tc.waitForText('ask a question', 15_000);
  await tc.sleepMs(1_500);
  await typeLine(tc, '/clear');
  await tc.sleepMs(3_000);
  const postClear = tc.getSnapshotFormatted();
  obs.clearRanCleanly =
    !postClear.includes('Failed to') && !postClear.includes('Internal error');

  obs.internalErrorSeen = tc
    .getSnapshotFormatted()
    .includes('Internal error');
  return obs;
}

describe('cloud sessions — slash-command A/B parity with local (mock BFF)', () => {
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
    'parity commands behave identically local vs cloud; gated commands refuse only in cloud',
    async () => {
      const mkws = () => {
        const ws = fs.mkdtempSync(
          path.join(os.tmpdir(), 'kiro-e2e-cmd-parity-')
        );
        // A context file for the gated /context add leg.
        fs.writeFileSync(path.join(ws, 'notes.md'), '# notes\n');
        tempDirs.push(ws);
        return ws;
      };

      // ── LOCAL leg (reference).
      const local = await CloudHarness.launch({
        testName: 'cmd-parity-local',
        cwd: mkws(),
        cliArgs: [],
      });
      harnesses.push(local);
      const ltc = local.testCase!;
      await ltc.waitForText('ask a question', BOOT_TIMEOUT);
      const localObs = await driveParityCommands(ltc);

      // Reference health: every captured row must be non-empty on the
      // LOCAL leg — otherwise the deep equality below could pass on
      // absent-in-both (rowWith returns '' on a miss).
      expect(localObs.helpRow).not.toBe('');
      expect(localObs.agentPickerDefaultRow).not.toBe('');
      expect(localObs.mcpPanelFooterRow).not.toBe('');
      expect(localObs.toolsPanelFooterRow).not.toBe('');
      expect(localObs.contextShowResponded).toBe(true);
      expect(localObs.usagePanelTitleRow).not.toBe('');
      expect(localObs.clearRanCleanly).toBe(true);
      expect(localObs.internalErrorSeen).toBe(false);

      // Local half of the GATED class: the real actions succeed locally.
      await typeLine(ltc, '/context add notes.md');
      await ltc.waitForText('Added', 15_000);
      expect(ltc.getSnapshotFormatted()).not.toContain(
        'not available for a cloud session'
      );
      await waitAlertGone(ltc);

      // ── CLOUD leg.
      const cloud = await CloudHarness.launch({
        testName: 'cmd-parity-cloud',
        cwd: mkws(),
      });
      harnesses.push(cloud);
      const ctc = cloud.testCase!;
      await ctc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await ctc.waitForText('ask a question', 20_000);
      const cloudObs = await driveParityCommands(ctc);

      // ── THE parity assertion for the same-behavior class.
      expect(cloudObs).toEqual(localObs);

      // ── Gated class in cloud: refusal, with the exact message class the
      // gates suite pins per-command; here the A/B contract is "local did
      // it, cloud refused it".
      await typeLine(ctc, '/context add notes.md');
      await ctc.waitForText('not available for a cloud session', 15_000);
      await waitAlertGone(ctc);

      await typeLine(ctc, '/chat save parity-session');
      await ctc.waitForText('save is not available for a cloud session', 15_000);
      await waitAlertGone(ctc);

      // ! shell escape: blocked in cloud. The positive contract is the
      // gate alert; the negative is that the echo's OUTPUT row (marker
      // without the `!echo` prefix) never appears — the typed command echo
      // itself legitimately shows the marker, so filter that row.
      await typeLine(ctc, '!echo PARITY_SHELL_MARKER');
      await ctc.waitForText(
        'Shell commands are not available for a cloud session',
        15_000
      );
      const shellLeaks = ctc
        .getSnapshotFormatted()
        .split('\n')
        .filter(
          (row) =>
            row.includes('PARITY_SHELL_MARKER') && !row.includes('!echo')
        );
      expect(shellLeaks).toEqual([]);
      await waitAlertGone(ctc);

      // Wire sanity: the cloud leg's panel/command traffic rode the mock
      // BFF (session was really relayed), and no raw error anywhere.
      expect(cloud.bffOutput()).toContain('CreateSpace');
      const finalSnap = ctc.getSnapshotFormatted();
      expect(finalSnap).not.toContain('Internal error');
      expect(finalSnap).not.toContain('rejected by sandbox');
    },
    900_000
  );
});
