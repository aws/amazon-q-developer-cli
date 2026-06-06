/**
 * E2E test: Real mid-stream mode swap.
 *
 * Validates bug-mine entries:
 *   2.1 — Mode-swap cursor realignment via useLayoutEffect
 *   2.2 — useLayoutEffect (not useEffect) for cursor reset
 *
 * Test 1 (lite->tui): Typing /tui DURING active streaming queues the command.
 * When the stream completes, the queue drains and fires the mode swap. All
 * pre-swap and post-swap content must survive.
 *
 * Test 2 (tui->lite): TUI mode rejects slash commands during processing, so
 * the swap fires immediately after the stream completes. The test verifies
 * that the TUI-rendered content's static cursor state does not prevent the
 * lite renderer from picking up (bug-mine 2.1: cursor realignment). This is
 * the same scenario as 2.2 — messages accumulated during TUI streaming must
 * appear in lite mode without "missing first batch" artifacts.
 *
 * DIVERGENCE from bug-mine 2.2 description:
 *   Bug 2.2 describes a race where useEffect (async) would miss the first
 *   batch of content after a mode swap. The fix (useLayoutEffect) is
 *   synchronous. This test validates the observable outcome — content survival
 *   and correct rendering — but cannot directly assert the React lifecycle
 *   hook timing. If useEffect were incorrectly used, the first lite batch
 *   after swap would be lost or duplicated.
 *
 * NOTE on RTS lookahead:
 *   The RTS ResponseParser uses a 1-lookahead pattern: after consuming an
 *   AssistantResponseEvent, it peeks the NEXT event to check for
 *   CodeReferenceEvent. This means event N's content only becomes visible to
 *   the TUI after event N+1 arrives. Tests push N+1 events to ensure N are
 *   rendered before the swap.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_LITE, CMD_TUI } from './lite/helpers/commands';

/** Helper: type a slash command char-by-char to avoid autocomplete race. */
async function typeSlashCommand(tc: E2ETestCase, command: string): Promise<void> {
  for (const char of command) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.pressEnter();
}

describe('lite mid-stream mode swap [bug-mine 2.1, 2.2]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite->tui: command queued during streaming, fires at turn-end', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('mid-stream-lite-to-tui')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    const chunk1Content = 'CHUNK_ONE_ALPHA_CONTENT';
    const chunk2Content = 'CHUNK_TWO_BETA_CONTENT';
    const chunk3Content = 'CHUNK_THREE_GAMMA_CONTENT';
    const chunk4Content = 'CHUNK_FOUR_DELTA_BUFFER';
    const postQueueContent = 'CHUNK_POST_QUEUE_EPSILON';
    const finalContent = 'CHUNK_FINAL_ZETA_DONE';

    // Push 4 events. Due to the RTS lookahead, events 1-3 render immediately
    // and event 4 is consumed but held in the peek buffer, keeping the stream
    // open (isProcessing = true).
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: chunk1Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk2Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk3Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk4Content } } },
    ]);

    // Send user message to trigger the response
    await testCase.sendKeys('start streaming');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for chunk 3 to render (confirming events 1-3 emitted)
    await testCase.waitForText(chunk3Content, 15000);

    // Verify we are still streaming
    const midStreamStore = await testCase.getStore();
    expect(midStreamStore.isProcessing).toBe(true);

    // --- QUEUE THE SWAP COMMAND MID-STREAM ---
    // In lite mode, /tui during processing is QUEUED (fires at turn-end).
    await typeSlashCommand(testCase, CMD_TUI);

    // Confirm the command was queued (transient alert appears)
    await testCase.waitForText('queued', 5000);

    // Mode should NOT have changed yet (still processing)
    const storeAfterQueue = await testCase.getStore();
    expect(storeAfterQueue.uiMode).toBe('lite');
    expect(storeAfterQueue.isProcessing).toBe(true);

    // --- COMPLETE THE STREAM ---
    // Push remaining events + null. This unblocks the peek for event 4 and
    // completes the stream. The queue then drains, firing /tui.
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + postQueueContent } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + finalContent } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Wait for the mode to actually swap (queue drains after stream ends)
    await testCase.waitForStoreCondition(
      (s) => s.uiMode === 'tui',
      15000,
    );
    await testCase.waitForIdle(15000);

    // Wait for the post-swap TUI repaint to actually land in the terminal.
    // waitForIdle only checks isProcessing — it returns the moment the
    // queue drains, which is when the swap fires, not when the new mode
    // has finished painting. Without this, getSnapshot() races the TUI
    // rerender and the screen snapshot can come back blank (~50% on this
    // machine). waitForText polls the live xterm parse so it returns
    // as soon as the content appears, not after a fixed sleep.
    await testCase.waitForText(finalContent, 15000);

    // --- ASSERTIONS ---
    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('tui');

    // All chunk content must be in the store messages
    const allMessageText = finalStore.messages
      .map((m) => JSON.stringify(m))
      .join(' ');

    expect(allMessageText).toContain(chunk1Content);
    expect(allMessageText).toContain(chunk2Content);
    expect(allMessageText).toContain(chunk3Content);
    expect(allMessageText).toContain(chunk4Content);
    expect(allMessageText).toContain(postQueueContent);
    expect(allMessageText).toContain(finalContent);

    // The final content should be visible in TUI mode
    const snapshot = testCase.getSnapshot();
    const allScreenText = snapshot.join('\n');
    expect(allScreenText).toContain(finalContent);
  }, 60000);

  it('tui->lite: swap immediately after streaming, content preserved (bug 2.1/2.2)', async () => {
    // TUI mode rejects slash commands during processing (shows a warning).
    // This test swaps immediately after the stream completes, exercising the
    // same cursor-realignment code path: the TUI's static cursor has advanced
    // during streaming, and lite must pick up without losing content.
    testCase = await E2ETestCase.builder()
      .withTestName('mid-stream-tui-to-lite')
      .withTerminal({ width: 120, height: 50 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    const chunk1Content = 'STREAM_PART_ALPHA_BEGIN';
    const chunk2Content = 'STREAM_PART_BETA_MIDDLE';
    const chunk3Content = 'STREAM_PART_GAMMA_PROGRESS';
    const chunk4Content = 'STREAM_PART_DELTA_BUFFER';
    const chunk5Content = 'STREAM_PART_EPSILON_TAIL';
    const finalContent = 'STREAM_PART_ZETA_END';

    // Push 6 events (first 5 render, 6th held in peek) + null to complete.
    // This simulates a multi-chunk streaming response that completes.
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: chunk1Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk2Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk3Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk4Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + chunk5Content } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: ' ' + finalContent } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send user message to trigger the response
    await testCase.sendKeys('begin stream');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the stream to complete and content to render
    await testCase.waitForText(finalContent, 15000);
    await testCase.waitForIdle(15000);

    // Verify TUI rendered all content
    const storeBeforeSwap = await testCase.getStore();
    expect(storeBeforeSwap.uiMode).toBe('tui');
    expect(storeBeforeSwap.isProcessing).toBe(false);

    // --- SWAP TO LITE (immediately after stream completes) ---
    // Bug 2.1/2.2: TUI's static cursor has advanced. Lite must realign.
    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition(
      (s) => s.uiMode === 'lite',
      10000,
    );
    await testCase.sleepMs(500);

    // --- VERIFY POST-SWAP BEHAVIOR ---
    // Bug 2.1: Messages from the TUI era must be in the store after swap
    const storeAfterSwap = await testCase.getStore();
    expect(storeAfterSwap.uiMode).toBe('lite');
    const allMessageText = storeAfterSwap.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMessageText).toContain(chunk1Content);
    expect(allMessageText).toContain(finalContent);

    // Bug 2.2: A new message in lite mode must render correctly (no missing
    // first batch due to stale cursor). Push a new response and verify it
    // appears in the terminal.
    const liteNewContent = 'LITE_NEW_AFTER_SWAP_MARKER';
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: liteNewContent } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('new lite msg');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText(liteNewContent, 15000);
    await testCase.waitForIdle(10000);

    // The new lite message must be visible on screen
    const snapshot = testCase.getSnapshot();
    const allScreenText = snapshot.join('\n');
    expect(allScreenText).toContain(liteNewContent);

    // Store must have both the old TUI content and new lite content
    const finalStore = await testCase.getStore();
    const finalMessageText = finalStore.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(finalMessageText).toContain(chunk1Content);
    expect(finalMessageText).toContain(finalContent);
    expect(finalMessageText).toContain(liteNewContent);
  }, 60000);
});
