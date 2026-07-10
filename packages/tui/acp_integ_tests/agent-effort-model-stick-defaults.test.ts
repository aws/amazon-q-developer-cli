/**
 * Wire-level tests for the KAS sticky agent/model/effort defaults, covering the
 * three dimensions the TUI reconciles from cli.json:
 *
 *   - model  — `chat.defaultModel` (global), opt-out `chat.disableAutoDefaultModel`
 *   - effort — `chat.modelDefaults[<model>]` (per-model, at the model's effort
 *              schema path), opt-out `chat.disableAutoDefaultEffort`
 *   - agent  — `chat.defaultAgent` (global); applied at startup only, never
 *              auto-persisted (there is no agent write path)
 *
 * The stateful mock KAS ({@link installStatefulKas}) models the v3 wire contract
 * faithfully so each test exercises the TUI's OWN apply/reconcile/persist logic
 * — its outgoing `set_config_option` requests, its store state, and the cli.json
 * it writes — rather than any KAS-internal behavior.
 *
 * Behavior groups: A apply-on-new, B reconcile-on-load, C mid-session switch,
 * D persistence, E opt-out.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AcpTestCase } from './shared/AcpTestCase';
import {
  installStatefulKas,
  makeKiroHome,
  readCliJson,
  savedEffort,
  waitForCliJson,
  effortSetValues,
  configSetValues,
  waitForConfigSet,
  writeResumeStub,
  v3WinnerListing,
  registerCleanup,
  cleanupStickyDefaults,
  type SelectEntry,
} from './shared/sticky-defaults-harness';

const A = 'claude-opus-4.8';
const B = 'claude-opus-4.7';
const C = 'claude-sonnet-4';
const N = 'amazon-nova-pro'; // no effort schema

const MODELS: SelectEntry[] = [
  { value: A, name: 'Claude Opus 4.8' },
  { value: B, name: 'Claude Opus 4.7' },
  { value: C, name: 'Claude Sonnet 4' },
];
const MODELS_WITH_N: SelectEntry[] = [
  ...MODELS,
  { value: N, name: 'Amazon Nova Pro' },
];
const AGENTS: SelectEntry[] = [
  { value: 'vibe', name: 'Default' },
  { value: 'spec', name: 'Spec' },
];

type Kas = ReturnType<typeof installStatefulKas>;

/** Re-sync the store from the mock's CURRENT state, defeating the documented
 * session-start broadcast race. Carries only values the product produced. */
function resync(tc: AcpTestCase, sessionId: string, kas: Kas): void {
  tc.mock.notify('session/update', {
    sessionId,
    update: {
      sessionUpdate: 'config_option_update',
      configOptions: kas.configOptions(),
    },
  });
}

async function bootReady(tc: AcpTestCase): Promise<void> {
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);
}

// TUI-driven slash-command flows are unreliable under the Windows CI PTY
// (see effort-status-bar / chat-command, similarly skipped). The feature is not
// Windows-specific; coverage runs on macOS + Linux.
describe.skipIf(process.platform === 'win32')(
  'sticky agent/model/effort defaults (KAS)',
  () => {
    let tc: AcpTestCase | null = null;

    afterEach(async () => {
      if (tc) {
        await tc.cleanup();
        tc = null;
      }
      cleanupStickyDefaults();
    });

    // ── Group A — apply on a NEW session ──────────────────────────────────────

    it('A: applies a saved chat.defaultModel on a new session', async () => {
      const home = makeKiroHome({ 'chat.defaultModel': B });
      tc = new AcpTestCase({
        testName: 'apply-model-saved',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      // The product read the saved default and switched the session to it.
      await waitForConfigSet(tc, 'model', B);
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);
    });

    it('A: an explicit --model flag wins over a saved chat.defaultModel', async () => {
      const home = makeKiroHome({ 'chat.defaultModel': B });
      tc = new AcpTestCase({
        testName: 'apply-model-flag',
        args: ['--model', A],
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: C, // engine default differs from both flag and saved
        initialEffort: 'high',
      });
      await bootReady(tc);

      await waitForConfigSet(tc, 'model', A);
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentModel?.id === A, 6000);
      // The saved default B was never applied — the flag won.
      expect(configSetValues(tc, 'model')).not.toContain(B);
    });

    it('A: applies a saved per-model effort once the model resolves', async () => {
      const home = makeKiroHome({
        'chat.modelDefaults': { [A]: { output_config: { effort: 'low' } } },
      });
      tc = new AcpTestCase({
        testName: 'apply-effort-saved',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high', // native != saved, so the apply is observable
      });
      await bootReady(tc);

      await waitForConfigSet(tc, 'effortLevel', 'low');
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentEffort === 'low', 6000);
    });

    it('A: an explicit --effort flag suppresses the saved per-model effort apply', async () => {
      const home = makeKiroHome({
        'chat.modelDefaults': { [A]: { output_config: { effort: 'low' } } },
      });
      tc = new AcpTestCase({
        testName: 'apply-effort-flag',
        args: ['--effort', 'high'],
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high', // KAS applied the flag
      });
      await bootReady(tc);

      // Give any (erroneous) auto-apply a chance to fire, then assert absence:
      // the saved 'low' must NOT be auto-applied when --effort is explicit.
      await tc.sleepMs(600);
      expect(effortSetValues(tc)).not.toContain('low');
      await tc.waitForStore((s) => s.currentEffort === 'high', 6000);
    });

    it('A: a saved effort not valid for the model is skipped (S4)', async () => {
      const home = makeKiroHome({
        'chat.modelDefaults': { [A]: { output_config: { effort: 'max' } } }, // not in the ladder
      });
      tc = new AcpTestCase({
        testName: 'apply-effort-invalid',
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      await tc.sleepMs(600);
      expect(effortSetValues(tc)).toEqual([]);
      await tc.waitForStore((s) => s.currentEffort === 'high', 6000);
    });

    it('A: effort apply is skipped for a model with no effort schema (S3)', async () => {
      const home = makeKiroHome({
        'chat.defaultModel': N,
        'chat.modelDefaults': { [N]: { output_config: { effort: 'low' } } },
      });
      tc = new AcpTestCase({
        testName: 'apply-effort-no-schema',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS_WITH_N,
        initialModel: A,
        initialEffort: 'high',
        noEffortSchema: new Set([N]),
      });
      await bootReady(tc);

      // The saved model N is applied, but its empty ladder means no effort write.
      await waitForConfigSet(tc, 'model', N);
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentModel?.id === N, 6000);
      await tc.sleepMs(400);
      expect(effortSetValues(tc)).toEqual([]);
    });

    it('A: applies a saved chat.defaultAgent on a new session', async () => {
      const home = makeKiroHome({ 'chat.defaultAgent': 'spec' });
      tc = new AcpTestCase({
        testName: 'apply-agent-saved',
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
        agents: AGENTS,
        initialMode: 'vibe',
      });
      await bootReady(tc);

      // The product forwarded the saved agent as the initial session mode.
      const req = tc.mock.receivedRequests('session/new')[0]!;
      expect(
        (req.params as { _meta?: { kiro?: { modeId?: string } } })._meta?.kiro
          ?.modeId
      ).toBe('spec');
      await tc.waitForStore((s) => s.currentAgent?.name === 'spec', 6000);
    });

    it('A: an explicit --agent flag wins over a saved chat.defaultAgent', async () => {
      const home = makeKiroHome({ 'chat.defaultAgent': 'default' });
      tc = new AcpTestCase({
        testName: 'apply-agent-flag',
        args: ['--agent', 'spec'],
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
        agents: AGENTS,
        initialMode: 'vibe',
      });
      await bootReady(tc);

      const req = tc.mock.receivedRequests('session/new')[0]!;
      expect(
        (req.params as { _meta?: { kiro?: { modeId?: string } } })._meta?.kiro
          ?.modeId
      ).toBe('spec');
      await tc.waitForStore((s) => s.currentAgent?.name === 'spec', 6000);
    });

    it('A: applies saved agent + model + per-model effort together on a new session', async () => {
      const home = makeKiroHome({
        'chat.defaultAgent': 'spec',
        'chat.defaultModel': B,
        'chat.modelDefaults': { [B]: { output_config: { effort: 'low' } } },
      });
      tc = new AcpTestCase({
        testName: 'apply-combo',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
        agents: AGENTS,
        initialMode: 'vibe',
      });
      await bootReady(tc);

      const req = tc.mock.receivedRequests('session/new')[0]!;
      expect(
        (req.params as { _meta?: { kiro?: { modeId?: string } } })._meta?.kiro
          ?.modeId
      ).toBe('spec');
      await waitForConfigSet(tc, 'model', B);
      await waitForConfigSet(tc, 'effortLevel', 'low');
      resync(tc, 's1', kas);
      await tc.waitForStore(
        (s) =>
          s.currentAgent?.name === 'spec' &&
          s.currentModel?.id === B &&
          s.currentEffort === 'low',
        6000
      );
    });

    // ── Group B — reconcile on LOAD (resume) ──────────────────────────────────

    it('B: a resumed session keeps its own agent/model/effort; global defaults are not imposed', async () => {
      const winnerId = 'resume-keep';
      // Globals all differ from the resumed session's own state.
      const home = makeKiroHome({
        'chat.defaultAgent': 'spec',
        'chat.defaultModel': B,
        'chat.modelDefaults': { [A]: { output_config: { effort: 'low' } } },
      });
      const cwd = realpathSync(
        mkdtempSync(join(tmpdir(), 'kiro-sticky-resume-'))
      );
      registerCleanup(() => rmSync(cwd, { recursive: true, force: true }));
      const stub = writeResumeStub(v3WinnerListing(cwd, winnerId));
      registerCleanup(stub.cleanup);

      tc = new AcpTestCase({
        testName: 'resume-keep-own',
        args: ['--resume'],
        cwd,
        extraEnv: { KIRO_HOME: home, KIRO_CHAT_CLI_BIN: stub.binPath },
      });
      installStatefulKas(tc, {
        sessionId: winnerId,
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
        agents: AGENTS,
        initialMode: 'vibe',
      });
      await bootReady(tc);

      // Loaded via session/load (NOT session/new): the session's own selections win.
      await tc.waitForStore(
        (s) =>
          s.sessionId === winnerId &&
          s.currentModel?.id === A &&
          s.currentEffort === 'high' &&
          s.currentAgent?.name === 'default',
        15000
      );
      expect(tc.mock.receivedRequests('session/load')).toHaveLength(1);
      expect(tc.mock.receivedRequests('session/new')).toHaveLength(0);

      // No default was imposed on load — no config writes at all.
      await tc.sleepMs(400);
      expect(configSetValues(tc, 'model')).toEqual([]);
      expect(configSetValues(tc, 'effortLevel')).toEqual([]);
      expect(configSetValues(tc, 'mode')).toEqual([]);
    }, 30000);

    it("B: an autonomous model change in a resumed session does not apply the new model's saved effort", async () => {
      const winnerId = 'resume-autonomous';
      const home = makeKiroHome({
        'chat.modelDefaults': {
          [A]: { output_config: { effort: 'low' } },
          [B]: { output_config: { effort: 'medium' } },
        },
      });
      const cwd = realpathSync(
        mkdtempSync(join(tmpdir(), 'kiro-sticky-resume-'))
      );
      registerCleanup(() => rmSync(cwd, { recursive: true, force: true }));
      const stub = writeResumeStub(v3WinnerListing(cwd, winnerId));
      registerCleanup(stub.cleanup);

      tc = new AcpTestCase({
        testName: 'resume-autonomous',
        args: ['--resume'],
        cwd,
        extraEnv: { KIRO_HOME: home, KIRO_CHAT_CLI_BIN: stub.binPath },
      });
      const kas = installStatefulKas(tc, {
        sessionId: winnerId,
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      await tc.waitForStore(
        (s) => s.sessionId === winnerId && s.currentModel?.id === A,
        15000
      );
      expect(effortSetValues(tc)).toEqual([]);

      // Autonomous rate-limit fallback to B (serverPush). B has a saved default.
      kas.state.model = B;
      resync(tc, winnerId, kas);
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);

      // Non-divergence: resumed + serverPush must NOT apply B's saved default.
      await tc.sleepMs(400);
      expect(effortSetValues(tc)).toEqual([]);
      expect(kas.state.effort).toBe('high');
    }, 30000);

    // ── Group C — mid-session switch ──────────────────────────────────────────

    it("C: restores a model's saved effort on switch-back, and forces nothing for a model with no saved default", async () => {
      const home = makeKiroHome({
        'chat.modelDefaults': {
          [A]: { output_config: { effort: 'xhigh' } },
          [B]: { output_config: { effort: 'medium' } },
        },
      });
      tc = new AcpTestCase({
        testName: 'switch-back',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'xhigh',
      });
      await bootReady(tc);
      resync(tc, 's1', kas);
      await tc.waitForStore(
        (s) => s.currentModel?.id === A && s.currentEffort === 'xhigh',
        5000
      );
      expect(effortSetValues(tc)).toEqual([]);

      // A -> B: KAS carries xhigh; the swap applies B's saved 'medium'.
      await tc.sendKeys(`/model ${B}`);
      await tc.pressEnter();
      await tc.waitForStore(
        (s) => s.currentModel?.id === B && s.currentEffort === 'medium',
        6000
      );
      await waitForConfigSet(tc, 'effortLevel', 'medium');
      expect(effortSetValues(tc)).toEqual(['medium']);

      // B -> A: KAS carries 'medium'; the swap restores A's saved 'xhigh'.
      await tc.sendKeys(`/model ${A}`);
      await tc.pressEnter();
      await tc.waitForStore(
        (s) => s.currentModel?.id === A && s.currentEffort === 'xhigh',
        6000
      );
      await waitForConfigSet(tc, 'effortLevel', 'xhigh');
      expect(effortSetValues(tc)).toEqual(['medium', 'xhigh']);

      // A -> C (no saved default): force nothing, keep the carried value.
      const before = effortSetValues(tc).length;
      await tc.sendKeys(`/model ${C}`);
      await tc.pressEnter();
      await tc.waitForStore(
        (s) => s.currentModel?.id === C && s.currentEffort === 'xhigh',
        6000
      );
      await tc.sleepMs(400);
      expect(effortSetValues(tc).length).toBe(before);
      expect(kas.state.effort).toBe('xhigh');
    });

    it('C: switching to a model whose saved effort equals the current level writes nothing (S1)', async () => {
      const home = makeKiroHome({
        'chat.modelDefaults': { [B]: { output_config: { effort: 'medium' } } },
      });
      tc = new AcpTestCase({
        testName: 'switch-idempotent',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'medium', // == B's saved default
      });
      await bootReady(tc);
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentEffort === 'medium', 5000);

      // A -> B: KAS carries 'medium'; B's saved is 'medium' == current -> no-op.
      await tc.sendKeys(`/model ${B}`);
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);
      await tc.sleepMs(400);
      expect(effortSetValues(tc)).toEqual([]);
    });

    it("C: an explicit /model switch inside a resumed session applies the new model's saved effort", async () => {
      const winnerId = 'resume-switch';
      const home = makeKiroHome({
        'chat.modelDefaults': { [B]: { output_config: { effort: 'medium' } } },
      });
      const cwd = realpathSync(
        mkdtempSync(join(tmpdir(), 'kiro-sticky-resume-'))
      );
      registerCleanup(() => rmSync(cwd, { recursive: true, force: true }));
      const stub = writeResumeStub(v3WinnerListing(cwd, winnerId));
      registerCleanup(stub.cleanup);

      tc = new AcpTestCase({
        testName: 'resume-explicit-switch',
        args: ['--resume'],
        cwd,
        extraEnv: { KIRO_HOME: home, KIRO_CHAT_CLI_BIN: stub.binPath },
      });
      installStatefulKas(tc, {
        sessionId: winnerId,
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);
      await tc.waitForStore(
        (s) =>
          s.sessionId === winnerId &&
          s.currentModel?.id === A &&
          s.currentEffort === 'high',
        15000
      );
      expect(effortSetValues(tc)).toEqual([]);

      // Explicit /model B (clientInitiated) applies even in a resumed session.
      await tc.sendKeys(`/model ${B}`);
      await tc.pressEnter();
      await tc.waitForStore(
        (s) => s.currentModel?.id === B && s.currentEffort === 'medium',
        6000
      );
      await waitForConfigSet(tc, 'effortLevel', 'medium');
      expect(effortSetValues(tc)).toEqual(['medium']);
    }, 30000);

    // ── Group D — persistence ─────────────────────────────────────────────────

    it('D: persists /model + /effort to cli.json and reapplies them on a fresh session', async () => {
      const home = makeKiroHome();

      // Session 1: set a model + effort, assert they land in cli.json.
      tc = new AcpTestCase({
        testName: 'persist-write',
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      await tc.sendKeys(`/model ${B}`);
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);
      await tc.waitForVisibleText('saved as default', 3000);

      await tc.sendKeys('/effort low');
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentEffort === 'low', 6000);
      await tc.waitForVisibleText('saved for', 3000);

      const cli1 = await waitForCliJson(
        home,
        (c) => c['chat.defaultModel'] === B && savedEffort(c, B) === 'low'
      );
      expect(cli1['chat.defaultModel']).toBe(B);
      expect(savedEffort(cli1, B)).toBe('low');

      await tc.cleanup();
      tc = null;

      // Session 2: fresh process, SAME KIRO_HOME. Startup applies the saved
      // model (B) and B's saved per-model effort (low).
      tc = new AcpTestCase({
        testName: 'persist-reapply',
        extraEnv: { KIRO_HOME: home },
      });
      const kas2 = installStatefulKas(tc, {
        sessionId: 's2',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      await waitForConfigSet(tc, 'model', B);
      await waitForConfigSet(tc, 'effortLevel', 'low');
      resync(tc, 's2', kas2);
      await tc.waitForStore(
        (s) => s.currentModel?.id === B && s.currentEffort === 'low',
        6000
      );
    });

    it('D: /model set-current-as-default persists even when auto-default-model is disabled (P4)', async () => {
      const home = makeKiroHome({ 'chat.disableAutoDefaultModel': true });
      tc = new AcpTestCase({
        testName: 'persist-set-default',
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      // Auto-write is off: switching does NOT persist a default.
      await tc.sendKeys(`/model ${B}`);
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);
      await tc.sleepMs(400);
      expect(readCliJson(home)['chat.defaultModel']).toBeUndefined();

      // Explicit set-current-as-default writes regardless of the opt-out.
      await tc.sendKeys('/model set-current-as-default');
      await tc.pressEnter();
      const cli = await waitForCliJson(
        home,
        (c) => c['chat.defaultModel'] === B
      );
      expect(cli['chat.defaultModel']).toBe(B);
    });

    // ── Group E — opt-out ─────────────────────────────────────────────────────

    it('E: disableAutoDefaultEffort suppresses the /effort write but still applies a pre-existing default', async () => {
      const home = makeKiroHome({
        'chat.disableAutoDefaultEffort': true,
        'chat.modelDefaults': { [A]: { output_config: { effort: 'low' } } },
      });
      tc = new AcpTestCase({
        testName: 'optout-effort',
        extraEnv: { KIRO_HOME: home },
      });
      const kas = installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      // apply-NOT-suppressed: the pre-existing default is applied at startup.
      await waitForConfigSet(tc, 'effortLevel', 'low');
      resync(tc, 's1', kas);
      await tc.waitForStore((s) => s.currentEffort === 'low', 6000);

      // write-SUPPRESSED: changing effort takes effect but must not persist.
      await tc.sendKeys('/effort high');
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentEffort === 'high', 6000);
      await tc.waitForVisibleText('Effort set to High', 3000);

      const snap = tc.getSnapshotFormatted();
      expect(snap).toContain('Effort set to High');
      expect(snap).not.toContain('saved for');

      await tc.sleepMs(500);
      expect(savedEffort(readCliJson(home), A)).toBe('low');
    });

    it('E: disableAutoDefaultModel suppresses the /model write but still applies the saved default', async () => {
      const home = makeKiroHome({
        'chat.disableAutoDefaultModel': true,
        'chat.defaultModel': B,
      });
      tc = new AcpTestCase({
        testName: 'optout-model',
        extraEnv: { KIRO_HOME: home },
      });
      installStatefulKas(tc, {
        sessionId: 's1',
        models: MODELS,
        initialModel: A,
        initialEffort: 'high',
      });
      await bootReady(tc);

      // apply-NOT-suppressed: saved default model B is applied at startup.
      await waitForConfigSet(tc, 'model', B);
      await tc.waitForStore((s) => s.currentModel?.id === B, 6000);

      // write-SUPPRESSED: switching to A must not overwrite the saved default.
      await tc.sendKeys(`/model ${A}`);
      await tc.pressEnter();
      await tc.waitForStore((s) => s.currentModel?.id === A, 6000);
      await tc.waitForVisibleText('Switched to Claude Opus 4.8', 3000);

      const snap = tc.getSnapshotFormatted();
      expect(snap).toContain('Switched to Claude Opus 4.8');
      expect(snap).not.toContain('saved as default');

      await tc.sleepMs(500);
      expect(readCliJson(home)['chat.defaultModel']).toBe(B);
    });
  }
);
