import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';
import { switchToLite } from '../e2e_tests/lite/helpers/mode-swap';

/**
 * Bug-mine 7.1, 7.3 + boot-indicator behavior: lite boot UX.
 *
 * 7.1: If already initialized at mount time (e.g. /tui -> /lite swap),
 *      the boot indicator stays hidden so we don't re-flash boot UI for
 *      a session that's been running for a while.
 * 7.2: After init completes in mock mode, the dim boot indicator stays
 *      hidden because nothing is in 'loading' state. (Replaces the
 *      previous grace-period test — the multi-line connecting panel and
 *      its 5s grace were dropped in favor of a single-row indicator that
 *      auto-hides when its phase settles.)
 * 7.3: MCP failure shows transient alert, not duplicate scrollback line.
 */
describe('lite boot connecting panel [bug-mine 7.1, 7.2, 7.3]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('no boot indicator on remount after /tui -> /lite swap [bug-mine 7.1]', async () => {
    // Start in TUI mode (already initialized by the time LiteLayout mounts)
    testCase = await TestCase.builder()
      .withTestName('lite-boot-no-panel-remount')
      .withGlobalSettings({ 'chat.ui.mode': 'tui' })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Verify TUI mode and that we are already initialized
    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('tui');
    expect(storeBefore.isInitialized).toBe(true);

    // Switch to lite mode — LiteLayout mounts with isInitialized already
    // true and bootProgress entries already 'ready', so showBootIndicator
    // is false from the first paint.
    await switchToLite(testCase);

    // Verify mode switched
    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.isInitialized).toBe(true);

    // Take a snapshot immediately — should NOT show any boot-indicator text.
    // None of the indicator's phase labels should be visible because every
    // bootProgress entry is in 'ready' state by mount time.
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('Connecting to agent');
    expect(snapshot).not.toContain('Initializing workspace');
    expect(snapshot).not.toMatch(/Loading \d+\/\d+ MCP server/);

    // The "ask a question" prompt should be visible (input is ready)
    expect(snapshot.toLowerCase()).toContain('ask a question');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('boot indicator hidden after init in mock mode [bug-mine 7.2]', async () => {
    // Validates the new single-row indicator's auto-hide behavior.
    //
    // In mock mode, init completes synchronously (MockSessionClient methods
    // are no-ops), so by the time LiteLayout mounts, every bootProgress
    // entry is 'ready' and mcpInitStatus is empty. The boot indicator's
    // showBootIndicator memo gates on any 'loading' status — with nothing
    // loading, the row is never rendered.
    //
    // (The previous test in this slot exercised a 5s grace-period timer
    // for the multi-line connecting panel. That panel and its grace timer
    // were removed: the panel persisted past the grace window when slow
    // MCPs were still loading, and its per-stage timers could show a green
    // ✓ "initializing workspace (10.7s)" while a 13s MCP was still spinning
    // below — visually contradictory. The single-row replacement avoids
    // both bugs by having no per-stage timer and auto-hiding the moment
    // its phase settles.)
    testCase = await TestCase.builder()
      .withTestName('lite-boot-indicator-hidden-after-init')
      .withLite()
      .withTimeout(15000)
      .launch();

    // Wait for the app to finish initializing
    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
    expect(store.isInitialized).toBe(true);

    // No grace timer to wait on — the indicator hides immediately when its
    // phase settles. Snapshot right away.
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('Connecting to agent');
    expect(snapshot).not.toContain('Initializing workspace');
    expect(snapshot).not.toMatch(/Loading \d+\/\d+ MCP server/);

    // Input area remains functional
    expect(snapshot.toLowerCase()).toContain('ask a question');

    // initErrors should be empty (successful boot, no MCP failures)
    expect(store.initErrors).toEqual([]);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('MCP init failure triggers transient alert (not duplicate scrollback) [bug-mine 7.3]', async () => {
    // Launch in lite mode, wait for init. The McpServerInitFailure event
    // is delivered through the global onUpdate handler (forwarded via
    // initNotificationHandler). In mock mode, injectEvent only broadcasts
    // when a prompt() is active. So we start a turn first, then inject.
    testCase = await TestCase.builder()
      .withTestName('lite-boot-mcp-failure-alert')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.isInitialized).toBe(true);
    expect(store.transientAlert).toBeNull();

    // Submit a message to activate the prompt — this makes MockSessionClient
    // broadcast injected events immediately (prompt is active).
    await testCase.typeAndSubmit('trigger turn');
    await testCase.sleepMs(300);

    // Now inject the MCP server init failure event while prompt is active.
    // The event goes through the global onUpdate handler which forwards
    // McpServerInitFailure to createStreamEventHandler, setting initErrors
    // and calling showTransientAlert.
    await testCase.mockSessionUpdate({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'test-failing-mcp',
      error: 'Connection refused',
    });
    await testCase.sleepMs(300);

    // After the failure event, a transient alert should be shown.
    // summarizeInitErrors() formats as "N MCP failure(s) — see /mcp"
    const storeAfter = await testCase.getStore();
    expect(storeAfter.transientAlert).not.toBeNull();
    expect(storeAfter.transientAlert!.message).toContain('MCP failure');

    // Verify that the messages array does NOT contain a scrollback-style
    // MCP failure message. The fix for bug 7.3 removed the scrollback line
    // in favor of the transient alert alone. Messages should only contain
    // the standard user/model messages, not an MCP failure line.
    const messages = storeAfter.messages;
    const mcpFailureInScrollback = messages.some(
      (m) =>
        m.role === MessageRole.Model &&
        typeof (m as any).content === 'string' &&
        (m as any).content.includes('test-failing-mcp')
    );
    expect(mcpFailureInScrollback).toBe(false);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
