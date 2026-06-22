import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { switchToLite, switchToTui } from '../e2e_tests/lite/helpers/mode-swap';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * /lite and /tui mode-swap commands (bug-mine 2.9). setUiMode's contract:
 *  cross-mode swap bumps liteScrollbackClearToken, resets liteStaticSkipBefore
 *  to 0 in BOTH directions, and preserves messages[]; same-mode dispatch is a
 *  noop (must not bump the token). The per-test comments below pin two
 *  distinct rejected/refactor-prone designs.
 */

describe('lite mode swap commands [bug-mine 2.9]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('tui→lite via /lite slash command bumps liteScrollbackClearToken and preserves messages', async () => {
    // Rejected design (bug-mine 2.6): pinning liteStaticSkipBefore to
    // messages.length on tui→lite would silently drop the user's scrollback.
    // The shipped contract resets skipBefore to 0 so prior messages repaint.
    testCase = await TestCase.builder()
      .withTestName('swap-tui-to-lite')
      .withGlobalSettings({ 'chat.ui.mode': 'tui' })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'tui-pre-swap-content',
      content: { type: 'text' as any, text: 'TUI_PRE_SWAP_REPLY' },
    });
    await testCase.typeAndSubmit('hello tui');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('tui');
    expect(storeBefore.messages.length).toBeGreaterThan(0);
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    await switchToLite(testCase);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.liteStaticSkipBefore).toBe(0);
    const allMessageText = storeAfter.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMessageText).toContain('TUI_PRE_SWAP_REPLY');

    await exitLiteInteg(testCase);
  }, 30000);

  it('lite→tui via /tui slash command bumps clear token and preserves messages', async () => {
    // Distinct from the tui→lite leg: catches a refactor that conditions the
    // token bump on `uiMode === 'lite'` and skips it on lite→tui, leaving
    // stale lite singletons for the next remount.
    testCase = await launchLiteInteg('swap-lite-to-tui');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'lite-pre-swap-content',
      content: { type: 'text' as any, text: 'LITE_PRE_SWAP_REPLY' },
    });
    await testCase.typeAndSubmit('hello lite');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('lite');
    expect(storeBefore.messages.length).toBeGreaterThan(0);
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    await switchToTui(testCase);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('tui');
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.liteStaticSkipBefore).toBe(0);
    const allMessageText = storeAfter.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMessageText).toContain('LITE_PRE_SWAP_REPLY');

    await exitLiteInteg(testCase);
  }, 30000);

  it('same-mode dispatch is a noop (bug 2.9)', async () => {
    testCase = await launchLiteInteg('swap-noop-same-mode');

    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('lite');
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // /lite while already in lite must NOT bump the clear token (bug 2.9).
    await switchToLite(testCase);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.liteScrollbackClearToken).toBe(tokenBefore);

    await exitLiteInteg(testCase);
  }, 30000);
});
