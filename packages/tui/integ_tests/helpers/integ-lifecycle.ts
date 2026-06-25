import { afterEach } from 'bun:test';
import { TestCase } from '../../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../../src/types/agent-events';

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
 * Clean lite-mode exit.
 *
 * Two subtle hazards, both fixed here:
 *
 * 1. Ctrl+C routing. The quit-key dispatch (app-keypress-dispatch.ts) sends
 *    Ctrl+C to cancelMessage() while isProcessing is true, and only to
 *    incrementExitSequence() when idle. So we first wait for the turn to settle
 *    — otherwise a Ctrl+C meant to exit is eaten as a cancel and the exit
 *    sequence never reaches its threshold.
 * 2. Exit-event race. The app calls process.exit() on the second Ctrl+C, which
 *    can fire bun-pty's (one-shot) onExit *before* expectExit() registers its
 *    listener — previously hanging that call for its whole timeout. PtyManager
 *    now latches the exit, so expectExit() resolves even if the process already
 *    left. We send the two presses spaced (so both land inside the 2s exit
 *    window) and then await.
 */
export async function exitLiteInteg(tc: TestCase): Promise<void> {
  try {
    await tc.waitForStore((s) => !s.isProcessing, 15000);
  } catch {
    /* settle best-effort; the presses below still drive the exit sequence */
  }

  await tc.sendKeys([0x03]);
  await tc.sleepMs(200);
  await tc.sendKeys([0x03]);

  await tc.expectExit(30000);
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
