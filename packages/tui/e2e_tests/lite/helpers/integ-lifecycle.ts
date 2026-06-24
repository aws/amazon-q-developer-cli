import { afterEach } from 'bun:test';
import { TestCase } from '../../../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../../../src/types/agent-events';

interface Cleanable {
  cleanup(): Promise<void>;
}

/**
 * Register a per-test cleanup that reads the current testCase via `getter` and
 * cleans it up after each test. Replaces the identical nullable-testCase
 * afterEach block previously copied into every lite test file — the test keeps
 * its plain `let testCase` and reassigns it normally.
 */
export function trackCleanup(getter: () => Cleanable | null): void {
  afterEach(async () => {
    const tc = getter();
    if (tc) await tc.cleanup();
  });
}

/** Launch the integ TestCase in lite mode and wait for the input prompt. */
export async function launchLiteInteg(
  testName: string,
  opts: {
    timeout?: number;
    terminal?: { width: number; height: number };
    settings?: Record<string, unknown>;
    env?: Record<string, string>;
  } = {}
): Promise<TestCase> {
  let builder = TestCase.builder()
    .withTestName(testName)
    .withLite()
    .withTimeout(opts.timeout ?? 15000);
  if (opts.terminal) builder = builder.withTerminal(opts.terminal);
  if (opts.settings) builder = builder.withGlobalSettings(opts.settings);
  if (opts.env) builder = builder.withEnv(opts.env);
  const tc = await builder.launch();
  await tc.waitForVisibleText('ask a question', 10000);
  return tc;
}

/**
 * Clean lite-mode exit: three Ctrl+C then assert the process exits.
 *
 * Uses a generous exit timeout (60s vs expectExit's 10s default): under load a
 * CPU/IO-starved chat_cli can take well over 10s to actually terminate after
 * the interrupt — the teardown timing, not the product, is what's slow. This
 * is the shared exit path for the lite integ suite, so the headroom covers all
 * of them.
 */
export async function exitLiteInteg(tc: TestCase): Promise<void> {
  await tc.sendKeys([0x03, 0x03, 0x03]);
  await tc.expectExit(60000);
}

/** completeTurn + settle + clean exit — the repeated tail of most lite integ tests. */
export async function finishAndExitLite(tc: TestCase): Promise<void> {
  await tc.completeTurn();
  await tc.sleepMs(100);
  await exitLiteInteg(tc);
}

/** Inject a Content event, submit a prompt, and settle — the busy-turn preamble shared by the cancel/interrupt cases. */
export async function startBusyTurn(
  tc: TestCase,
  opts: { marker: string; id?: string; prompt?: string; settleMs?: number }
): Promise<void> {
  await tc.mockSessionUpdate({
    type: AgentEventType.Content,
    id: opts.id ?? 'content-1',
    content: { type: ContentType.Text, text: opts.marker },
  });
  await tc.typeAndSubmit(opts.prompt ?? 'test prompt');
  await tc.sleepMs(opts.settleMs ?? 300);
}
