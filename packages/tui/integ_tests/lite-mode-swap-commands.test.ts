import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { switchToLite, switchToTui } from '../e2e_tests/lite/helpers/mode-swap';

/**
 * Bug-mine 2.9: /lite and /tui mode-swap commands.
 *
 * 2.9 anchor — setUiMode same-mode noop: dispatching setUiMode('lite')
 *  while already in lite must NOT bump liteScrollbackClearToken (that
 *  would wipe scrollback for a no-op call).
 *
 * The cross-mode tests below also lock in the wipe-and-replay contract
 * setUiMode owns: bump the clear token, reset liteStaticSkipBefore to 0,
 * preserve the messages array (so the destination renderer paints the
 * full session in its own style). bug-mine 2.6's earlier framing —
 * "tui→lite pins skipBefore to messages.length" — was rejected: the
 * explicit contract documented in setUiMode's body is to RESET skipBefore
 * to 0 in both directions.
 *
 * Implementation note: Slash commands typed into the PromptInput trigger the
 * CommandMenu when the input matches a known command prefix (no trailing
 * space). The menu intercepts Enter and routes through handleUserInput.
 * We type the command with a trailing space so the menu closes, then Enter
 * submits directly through PromptInput's normal path.
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
    // Anchor: app-store.ts setUiMode (the cross-mode path). The swap must:
    //   (a) flip uiMode to 'lite' (proves the slash command actually
    //       reached setUiMode through the dispatcher, not just edited
    //       the input box);
    //   (b) bump liteScrollbackClearToken so the lite renderer wipes
    //       its singletons + repaints from messages[];
    //   (c) reset liteStaticSkipBefore to 0 so every prior message
    //       repaints under the new mode (the explicit contract in
    //       setUiMode's body comment — both directions wipe and replay).
    //
    // Class of regression: a refactor that changes setUiMode to NOT bump
    // the clear token would let the prior session's TUI rows hang around
    // in scrollback while the new lite mode painted a fresh banner above
    // them. A refactor that pins liteStaticSkipBefore to messages.length
    // (a previously-considered but rejected design) would silently drop
    // the user's scrollback the moment they swapped modes.
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
    // The clear token bumped on the cross-mode swap (lite renderer's
    // wipe-and-repaint signal).
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    // setUiMode resets skipBefore to 0 in both directions (per the
    // explicit contract — see app-store.ts setUiMode body comment).
    expect(storeAfter.liteStaticSkipBefore).toBe(0);
    // Messages persist across the swap; only the rendering surface changed.
    const allMessageText = storeAfter.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMessageText).toContain('TUI_PRE_SWAP_REPLY');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('lite→tui via /tui slash command bumps clear token and preserves messages', async () => {
    // Mirror of the above for the reverse direction. The TUI layout
    // doesn't consume liteScrollbackClearToken at runtime (it's a
    // lite-side wipe signal), but the cross-mode contract still bumps
    // it so a subsequent lite remount picks up cleanly. Catches a
    // refactor that conditions the bump on `uiMode === 'lite'` and
    // skips it on the lite→tui leg, leaving stale lite singletons
    // behind for the next remount.
    testCase = await TestCase.builder()
      .withTestName('swap-lite-to-tui')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

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

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('same-mode dispatch is a noop (bug 2.9)', async () => {
    testCase = await TestCase.builder()
      .withTestName('swap-noop-same-mode')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Note the current scrollback clear token
    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('lite');
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // Type /lite while already in lite mode — should be a noop
    await switchToLite(testCase);

    // Verify liteScrollbackClearToken did NOT change (bug 2.9)
    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.liteScrollbackClearToken).toBe(tokenBefore);

    // No crash — clean exit
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
