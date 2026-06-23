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

  // Cross-mode swap, both directions: bumps liteScrollbackClearToken, resets
  // liteStaticSkipBefore to 0, and preserves messages[]. The two legs catch
  // distinct rejected refactors:
  //  - tui→lite (bug-mine 2.6): pinning skipBefore to messages.length would
  //    silently drop the user's scrollback; the contract resets it to 0.
  //  - lite→tui: conditioning the token bump on `uiMode === 'lite'` would skip
  //    it here, leaving stale lite singletons for the next remount.
  it.each([
    {
      label: 'tui→lite via /lite',
      start: 'tui' as const,
      target: 'lite' as const,
      switchMode: switchToLite,
      marker: 'TUI_PRE_SWAP_REPLY',
      contentId: 'tui-pre-swap-content',
      prompt: 'hello tui',
    },
    {
      label: 'lite→tui via /tui',
      start: 'lite' as const,
      target: 'tui' as const,
      switchMode: switchToTui,
      marker: 'LITE_PRE_SWAP_REPLY',
      contentId: 'lite-pre-swap-content',
      prompt: 'hello lite',
    },
  ])(
    '$label bumps liteScrollbackClearToken and preserves messages',
    async ({ start, target, switchMode, marker, contentId, prompt }) => {
      testCase =
        start === 'tui'
          ? await TestCase.builder()
              .withTestName('swap-tui-to-lite')
              .withGlobalSettings({ 'chat.ui.mode': 'tui' })
              .withTimeout(15000)
              .launch()
          : await launchLiteInteg('swap-lite-to-tui');
      if (start === 'tui') {
        await testCase.waitForVisibleText('ask a question', 10000);
      }

      await testCase.mockSessionUpdate({
        type: AgentEventType.Content,
        id: contentId,
        content: { type: 'text' as any, text: marker },
      });
      await testCase.typeAndSubmit(prompt);
      await testCase.completeTurn();
      await testCase.sleepMs(400);

      const storeBefore = await testCase.getStore();
      expect(storeBefore.uiMode).toBe(start);
      expect(storeBefore.messages.length).toBeGreaterThan(0);
      const tokenBefore = storeBefore.liteScrollbackClearToken;

      await switchMode(testCase);

      const storeAfter = await testCase.getStore();
      expect(storeAfter.uiMode).toBe(target);
      expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
      expect(storeAfter.liteStaticSkipBefore).toBe(0);
      const allMessageText = storeAfter.messages
        .map((m) => JSON.stringify(m))
        .join(' ');
      expect(allMessageText).toContain(marker);

      await exitLiteInteg(testCase);
    },
    30000
  );

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
