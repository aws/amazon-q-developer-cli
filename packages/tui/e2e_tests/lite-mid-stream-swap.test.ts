/**
 * E2E test: real mid-stream mode swap [bug-mine 2.1, 2.2].
 *
 * DIVERGENCE from bug-mine 2.2: bug 2.2 is a race where useEffect (async)
 * misses the first batch of content after a mode swap; the fix is
 * useLayoutEffect (synchronous). This test can only assert the observable
 * outcome (content survival + correct rendering), not the hook timing — under
 * the buggy useEffect the first lite batch after swap would be lost/duplicated.
 *
 * RTS lookahead: the RTS ResponseParser uses 1-lookahead — after consuming an
 * AssistantResponseEvent it peeks the NEXT event for a CodeReferenceEvent, so
 * event N only becomes visible after event N+1 arrives. Tests push N+1 events
 * to ensure N is rendered before the swap.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_LITE,
  CMD_TUI,
  launchLiteE2E,
  typeSlashCommand,
  sendUserMessage,
} from './lite/helpers/commands';
import { assistantEvent } from './lite/helpers/responses';

describe('lite mid-stream mode swap [bug-mine 2.1, 2.2]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite->tui: command queued during streaming, fires at turn-end', async () => {
    testCase = await launchLiteE2E('mid-stream-lite-to-tui', {
      terminal: { width: 120, height: 50 },
    });

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
      assistantEvent(chunk1Content),
      assistantEvent(' ' + chunk2Content),
      assistantEvent(' ' + chunk3Content),
      assistantEvent(' ' + chunk4Content),
    ]);

    await sendUserMessage(testCase, 'start streaming');

    // chunk 3 rendered confirms events 1-3 emitted
    await testCase.waitForText(chunk3Content, 15000);

    const midStreamStore = await testCase.getStore();
    expect(midStreamStore.isProcessing).toBe(true);

    // In lite mode, /tui during processing is QUEUED (fires at turn-end).
    await typeSlashCommand(testCase, CMD_TUI);

    await testCase.waitForText('queued', 5000);

    // Mode must NOT have changed yet (still processing).
    const storeAfterQueue = await testCase.getStore();
    expect(storeAfterQueue.uiMode).toBe('lite');
    expect(storeAfterQueue.isProcessing).toBe(true);

    // Push remaining events + null: unblocks the peek for event 4 and
    // completes the stream; the queue then drains, firing /tui.
    await testCase.pushSendMessageResponse([
      assistantEvent(' ' + postQueueContent),
      assistantEvent(' ' + finalContent),
    ]);
    await testCase.pushSendMessageResponse(null);

    // Mode swaps once the queue drains after the stream ends.
    await testCase.waitForStoreCondition((s) => s.uiMode === 'tui', 15000);
    await testCase.waitForIdle(15000);

    // waitForIdle only checks isProcessing — it returns the moment the queue
    // drains (the swap fires), not when the new mode finished painting.
    // Without this, getSnapshot() races the TUI rerender and the screen can
    // come back blank (~50% on this machine). waitForText polls the live
    // xterm parse so it returns as soon as the content appears.
    await testCase.waitForText(finalContent, 15000);

    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('tui');

    const allMessageText = finalStore.messages
      .map((m) => JSON.stringify(m))
      .join(' ');

    expect(allMessageText).toContain(chunk1Content);
    expect(allMessageText).toContain(chunk2Content);
    expect(allMessageText).toContain(chunk3Content);
    expect(allMessageText).toContain(chunk4Content);
    expect(allMessageText).toContain(postQueueContent);
    expect(allMessageText).toContain(finalContent);

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
      assistantEvent(chunk1Content),
      assistantEvent(' ' + chunk2Content),
      assistantEvent(' ' + chunk3Content),
      assistantEvent(' ' + chunk4Content),
      assistantEvent(' ' + chunk5Content),
      assistantEvent(' ' + finalContent),
    ]);
    await testCase.pushSendMessageResponse(null);

    await sendUserMessage(testCase, 'begin stream');

    await testCase.waitForText(finalContent, 15000);
    await testCase.waitForIdle(15000);

    const storeBeforeSwap = await testCase.getStore();
    expect(storeBeforeSwap.uiMode).toBe('tui');
    expect(storeBeforeSwap.isProcessing).toBe(false);

    // Swap to lite right after the stream completes. Bug 2.1/2.2: TUI's
    // static cursor has advanced; lite must realign.
    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);
    await testCase.sleepMs(500);

    // Bug 2.1: TUI-era messages must survive the swap into the store.
    const storeAfterSwap = await testCase.getStore();
    expect(storeAfterSwap.uiMode).toBe('lite');
    const allMessageText = storeAfterSwap.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(allMessageText).toContain(chunk1Content);
    expect(allMessageText).toContain(finalContent);

    // Bug 2.2: a new lite message must render correctly (no missing first
    // batch due to stale cursor).
    const liteNewContent = 'LITE_NEW_AFTER_SWAP_MARKER';
    await testCase.pushSendMessageResponse([assistantEvent(liteNewContent)]);
    await testCase.pushSendMessageResponse(null);

    await sendUserMessage(testCase, 'new lite msg');
    await testCase.waitForText(liteNewContent, 15000);
    await testCase.waitForIdle(10000);

    const snapshot = testCase.getSnapshot();
    const allScreenText = snapshot.join('\n');
    expect(allScreenText).toContain(liteNewContent);

    // Store must have both the old TUI content and new lite content.
    const finalStore = await testCase.getStore();
    const finalMessageText = finalStore.messages
      .map((m) => JSON.stringify(m))
      .join(' ');
    expect(finalMessageText).toContain(chunk1Content);
    expect(finalMessageText).toContain(finalContent);
    expect(finalMessageText).toContain(liteNewContent);
  }, 60000);
});
