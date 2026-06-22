/**
 * Lite smoke tests for BOTH-classified e2e scenarios.
 *
 * Verifies that selected BOTH-classified e2e tests work in lite mode with
 * an observable behavioral assertion (not just "no crash"). Each surviving
 * test pins a real user-observable invariant: signal-exit (process really
 * exits), cancel-state-recovery (turn really stops + a follow-up turn
 * actually goes through), chat-command (sessionId really changes + old
 * content is purged), paste-preserves-indentation (the multi-line body
 * survives bracketed paste without escape leakage).
 *
 * Earlier "no-crash" smokes for /compact, /clear, and !shell-escape lived
 * here. /clear and !shell-escape are covered with stronger behavioral
 * assertions in integ_tests/lite-misc.test.ts (10.8 / 10.5). /compact
 * needed a real backend round-trip the mock can't provide; the surviving
 * mock-only assertion passed even when /compact did nothing, so it was
 * removed.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_CHAT_NEW, typeSlashCommand } from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

// Bracketed paste escape sequences
const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';

describe('lite smoke: BOTH-classified e2e tests', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it.skipIf(process.platform === 'win32')(
    'signal-exit: SIGHUP exits cleanly in lite mode',
    async () => {
      testCase = await E2ETestCase.builder()
        .withTestName('lite-smoke-signal-exit')
        .withTerminal({ width: 120, height: 40 })
        .withLite()
        .launch();

      await testCase.waitForText('>', 15000);

      const launcherPid = testCase.getPid()!;
      expect(launcherPid).toBeGreaterThan(0);

      process.kill(launcherPid, 'SIGHUP');

      // Any exit (0 or signal-based) is fine — the assertion is no hang.
      const exitCode = await testCase.expectExit();
      expect(exitCode).toBeDefined();
    },
    30000
  );

  it('cancel-state-recovery: Ctrl+C clears isProcessing in lite mode', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-smoke-cancel-recovery')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.getSessionId();

    // keepOpen so the turn stays open until we cancel it.
    await streamReply(testCase, 'Thinking...', { keepOpen: true });

    await testCase.sendKeys('test prompt');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition((s) => s.isProcessing === true, 10000);

    await testCase.pressCtrlC();

    const afterCancel = await testCase.waitForStoreCondition(
      (s) => !s.isProcessing,
      5000
    );
    expect(afterCancel.isProcessing).toBe(false);

    // A second prompt must still go through (turn not stuck).
    await streamReply(testCase, 'SECOND_OK');

    await testCase.sendKeys('second');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    const afterSecond = await testCase.waitForStoreCondition(
      (s) => s.messages.length > afterCancel.messages.length,
      10000
    );
    expect(afterSecond.messages.length).toBeGreaterThan(
      afterCancel.messages.length
    );
  }, 60000);

  it('chat-command: /chat new resets messages in lite mode', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-smoke-chat-new')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();

    const initialSessionId = await testCase.getSessionId();

    await streamReply(testCase, 'First reply.');

    await testCase.sendKeys('hi');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForIdle(15000);

    await typeSlashCommand(testCase, CMD_CHAT_NEW);

    const afterNew = await testCase.waitForStoreCondition(
      (s) => s.sessionId !== initialSessionId && s.sessionId !== null,
      15000
    );
    expect(afterNew.sessionId).not.toBe(initialSessionId);

    // Old messages must be purged by /chat new.
    const hasOldContent = afterNew.messages.some((m) =>
      m.content.includes('First reply.')
    );
    expect(hasOldContent).toBe(false);
  }, 60000);

  it('paste: multi-line indented text lands correctly in lite mode', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-smoke-paste-indent')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.getSessionId();

    await streamReply(testCase, 'Got paste.');

    const pastedText = 'function foo() {\n  return 42;\n}';
    await testCase.sendKeys(`${PASTE_START}${pastedText}${PASTE_END}`);
    await testCase.sleepMs(300);

    // Bracketed-paste markers must not leak into the rendered input.
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('[200~');
    expect(snapshot).not.toContain('[201~');

    // Pasted content is visible either inline or as a chip.
    const hasFunctionText =
      snapshot.includes('function foo()') || snapshot.includes('3 lines');
    expect(hasFunctionText).toBe(true);

    await testCase.pressEnter();
    await testCase.waitForText('Got paste.', 10000);

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
  }, 30000);
});
