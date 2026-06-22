import { TestCase } from '../../../src/test-utils/TestCase';

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
