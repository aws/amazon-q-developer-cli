import { TestCase } from '../../../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../../../src/types/agent-events';

/**
 * Launch the integ TestCase in lite mode and wait for the input prompt.
 * Captures the launch/wait ceremony repeated across every lite integ test.
 */
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

/** Clean lite-mode exit: three Ctrl+C then assert the process exits. */
export async function exitLiteInteg(tc: TestCase): Promise<void> {
  await tc.sendKeys([0x03, 0x03, 0x03]);
  await tc.expectExit();
}

/** completeTurn + settle + clean exit — the repeated tail of most lite integ tests. */
export async function finishAndExitLite(tc: TestCase): Promise<void> {
  await tc.completeTurn();
  await tc.sleepMs(100);
  await exitLiteInteg(tc);
}

/**
 * Inject a Content event, submit a prompt, settle, and assert the turn is
 * processing. Captures the busy-turn preamble shared by the cancel/interrupt
 * cases. Returns once isProcessing===true.
 */
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
