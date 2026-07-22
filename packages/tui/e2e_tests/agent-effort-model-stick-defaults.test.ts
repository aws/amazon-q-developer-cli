/**
 * E2E tests for model/effort defaults on the V2 (Rust) engine, observed
 * through the real TUI. The V2 engine resolves the static model list + effort
 * ladders locally (no network — see effort-status-bar.test.ts), so seeding
 * cli.json and reading the store back is fully deterministic.
 *
 * Coverage, using the real Rust-known model ids (`claude-opus-4.7` → native
 * xhigh, `claude-sonnet-4.6` → native high):
 *   - apply a saved default model / per-model effort on a new session
 *   - `/model` and `/effort` are session-only: switching writes nothing
 *   - `/effort set-current-as-default` persists the current effort for the
 *     current model
 *
 * The agent default (`chat.defaultAgent`) and resume reconciliation are covered
 * on the KAS side; the agent apply path is engine-agnostic TUI code (index.tsx)
 * and resume needs a seeded on-disk session — both deferred here.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

const OPUS = 'claude-opus-4.7'; // default model; native effort xhigh
const SONNET = 'claude-sonnet-4.6'; // native effort high

function readCli(tc: E2ETestCase): Record<string, unknown> {
  const p = path.join(tc.sandboxDir, '.kiro', 'settings', 'cli.json');
  return fs.existsSync(p)
    ? (JSON.parse(fs.readFileSync(p, 'utf-8')) as Record<string, unknown>)
    : {};
}

function savedEffort(cli: Record<string, unknown>, model: string): unknown {
  const defaults = cli['chat.modelDefaults'] as
    | Record<string, { output_config?: { effort?: unknown } }>
    | undefined;
  return defaults?.[model]?.output_config?.effort;
}

async function waitForCli(
  tc: E2ETestCase,
  predicate: (cli: Record<string, unknown>) => boolean,
  timeoutMs = 6000
): Promise<Record<string, unknown>> {
  const start = Date.now();
  let last: Record<string, unknown> = {};
  while (Date.now() - start < timeoutMs) {
    last = readCli(tc);
    if (predicate(last)) return last;
    await tc.sleepMs(100);
  }
  throw new Error(
    `cli.json predicate unmet in ${timeoutMs}ms; last=${JSON.stringify(last)}`
  );
}

// TUI-driven slash-command flows are unreliable under the Windows CI PTY
// (see effort-status-bar / chat-command, similarly skipped). The feature is not
// Windows-specific; coverage runs on macOS + Linux.
describe.skipIf(process.platform === 'win32')('saved model/effort defaults (V2)', () => {
  let tc: E2ETestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    await new Promise((r) => setTimeout(r, 500));
  });

  // ── Apply on a new session ────────────────────────────────────────────────

  it('applies a saved chat.defaultModel on a new session', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('v2-apply-model-saved')
      .withTerminal({ width: 120, height: 40 })
      .withGlobalSettings({ 'chat.defaultModel': SONNET })
      .launch();

    await tc.waitForText('ask a question', 15000);
    const store = await tc.waitForStoreCondition(
      (s) => s.currentModel?.id === SONNET,
      15000
    );
    expect(store.currentModel?.id).toBe(SONNET);
  }, 30000);

  it('applies a saved per-model effort once the model resolves', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('v2-apply-effort-saved')
      .withTerminal({ width: 120, height: 40 })
      // Default model is opus-4.7 (native xhigh); a saved 'low' is observable.
      .withGlobalSettings({
        'chat.modelDefaults': { [OPUS]: { output_config: { effort: 'low' } } },
      })
      .launch();

    await tc.waitForText('ask a question', 15000);
    const store = await tc.waitForStoreCondition(
      (s) => s.currentEffort === 'low',
      15000
    );
    expect(store.currentEffort).toBe('low');
  }, 30000);

  // ── Session-only switching ────────────────────────────────────────────────

  it('/model and /effort switch the session without writing settings', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('v2-session-only-switch')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.waitForSlashCommands();
    await tc.waitForStoreCondition((s) => s.currentEffort !== null, 10000);

    await tc.sendKeys(`/model ${SONNET}`);
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.waitForStoreCondition((s) => s.currentModel?.id === SONNET, 10000);

    await tc.sendKeys('/effort low');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.waitForStoreCondition((s) => s.currentEffort === 'low', 10000);

    await tc.sleepMs(500);
    const cli = readCli(tc);
    expect(cli['chat.defaultModel']).toBeUndefined();
    expect(cli['chat.modelDefaults']).toBeUndefined();
  }, 45000);

  // ── Explicit persistence ──────────────────────────────────────────────────

  it('/effort set-current-as-default persists the current effort for the current model', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('v2-effort-set-default')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.waitForSlashCommands();
    await tc.waitForStoreCondition((s) => s.currentEffort !== null, 10000);

    await tc.sendKeys('/effort low');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.waitForStoreCondition((s) => s.currentEffort === 'low', 10000);

    await tc.sendKeys('/effort set-current-as-default');
    await tc.sleepMs(200);
    await tc.pressEnter();

    const cli = await waitForCli(tc, (c) => savedEffort(c, OPUS) === 'low');
    expect(savedEffort(cli, OPUS)).toBe('low');
  }, 45000);
});
