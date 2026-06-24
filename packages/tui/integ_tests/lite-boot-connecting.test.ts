import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';
import { switchToLite } from '../e2e_tests/lite/helpers/mode-swap';
import {
  exitLiteInteg,
  finishAndExitLite,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Lite boot UX (bug-mine 7.x):
 *   7.1: already-initialized remount (/tui -> /lite) shows no boot indicator.
 *   7.2: after init, the boot indicator auto-hides (nothing 'loading').
 *   7.3: MCP failure shows a transient alert, not a duplicate scrollback line.
 */
function expectNoBootIndicator(snapshot: string): void {
  expect(snapshot).not.toContain('Connecting to agent');
  expect(snapshot).not.toContain('Initializing workspace');
  expect(snapshot).not.toMatch(/Loading \d+\/\d+ MCP server/);
  expect(snapshot.toLowerCase()).toContain('ask a question');
}

describe('lite boot connecting panel [bug-mine 7.1, 7.2, 7.3]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  it('no boot indicator on remount after /tui -> /lite swap [bug-mine 7.1]', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-boot-no-panel-remount')
      .withGlobalSettings({ 'chat.ui.mode': 'tui' })
      // Boot in TUI then swap to lite via /lite — but /lite is gated on the
      // rollout flag (effects.ts switchToLite), which the preload only sets
      // when an argv token matches `lite-`. A full-directory run (CI's
      // `bun test ./integ_tests/`, now uncapped) has no such token, so the
      // flag is unset and /lite silently no-ops. Set it per-test (as
      // withLite() does) — safe here because chat.ui.mode='tui' is an explicit
      // mode, so the first-launch UI-mode picker (gated on an unresolved mode
      // + rollout) never triggers.
      .withEnv({ KIRO_LITE_ROLLOUT_ENABLED: '1' })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('tui');
    expect(storeBefore.isInitialized).toBe(true);

    await switchToLite(testCase);

    // The /lite dispatch is async; poll for lite mode instead of asserting on
    // a stale snapshot. Generous timeout: the end-to-end dispatch can exceed
    // 10s under contended CI (integ runs uncapped).
    const storeAfter = await testCase.waitForStore(
      (s) => s.uiMode === 'lite',
      20000
    );
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.isInitialized).toBe(true);

    expectNoBootIndicator(testCase.getSnapshot().join('\n'));

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('boot indicator hidden after init in mock mode [bug-mine 7.2]', async () => {
    // WHY auto-hide: showBootIndicator gates on any 'loading' status. In mock
    // mode init completes synchronously, so by mount everything is 'ready' and
    // the single-row indicator is never rendered. (Replaced an earlier
    // multi-line connecting panel + 5s grace timer that could show a green ✓
    // stage while a slow MCP was still spinning below.)
    testCase = await launchLiteInteg('lite-boot-indicator-hidden-after-init');

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
    expect(store.isInitialized).toBe(true);

    expectNoBootIndicator(testCase.getSnapshot().join('\n'));
    expect(store.initErrors).toEqual([]);

    await exitLiteInteg(testCase);
  }, 30000);

  it('MCP init failure triggers transient alert (not duplicate scrollback) [bug-mine 7.3]', async () => {
    testCase = await launchLiteInteg('lite-boot-mcp-failure-alert');

    const store = await testCase.getStore();
    expect(store.isInitialized).toBe(true);
    expect(store.transientAlert).toBeNull();

    // In mock mode injectEvent only broadcasts while a prompt() is active, so
    // start a turn before injecting the MCP failure.
    await testCase.typeAndSubmit('trigger turn');
    await testCase.sleepMs(300);

    await testCase.mockSessionUpdate({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'test-failing-mcp',
      error: 'Connection refused',
    });
    await testCase.sleepMs(300);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.transientAlert).not.toBeNull();
    expect(storeAfter.transientAlert!.message).toContain('MCP failure');

    // Bug 7.3: the failure surfaces ONLY as a transient alert — no
    // scrollback-style model message naming the failed server.
    const mcpFailureInScrollback = storeAfter.messages.some(
      (m) =>
        m.role === MessageRole.Model &&
        typeof (m as any).content === 'string' &&
        (m as any).content.includes('test-failing-mcp')
    );
    expect(mcpFailureInScrollback).toBe(false);

    await finishAndExitLite(testCase);
  }, 30000);
});
