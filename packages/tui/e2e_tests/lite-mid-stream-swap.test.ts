/**
 * Real mid-stream mode swap [bug-mine 2.1, 2.2]. Asserts the observable outcome
 * (content survives + renders), not the hook timing — bug 2.2's useEffect→
 * useLayoutEffect fix is what stops the first post-swap lite batch being lost.
 *
 * WHY N+1 events: the RTS ResponseParser uses 1-lookahead (after an
 * AssistantResponseEvent it peeks the NEXT event for a CodeReferenceEvent), so
 * event N only becomes visible once event N+1 arrives.
 */

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_LITE,
  CMD_TUI,
  launchLiteE2E,
  launchTuiE2E,
  typeSlashCommand,
  sendUserMessage,
} from './lite/helpers/commands';
import { assistantEvent, messageText } from './lite/helpers/responses';

describe('lite mid-stream mode swap [bug-mine 2.1, 2.2]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it('lite->tui: command queued during streaming, fires at turn-end', async () => {
    testCase = await launchLiteE2E('mid-stream-lite-to-tui', {
      terminal: { width: 120, height: 50 },
    });

    // Events 1-3 render immediately; event 4 is consumed but held in the RTS
    // peek buffer, keeping the stream open (isProcessing = true).
    const preChunks = ['ALPHA', 'BETA', 'GAMMA', 'DELTA_BUFFER'];
    const postChunks = ['POST_QUEUE_EPSILON', 'FINAL_ZETA_DONE'];
    const finalContent = postChunks[1]!;
    const renderedPreChunk = preChunks[2]!;
    await testCase.pushSendMessageResponse(
      preChunks.map((c, i) => assistantEvent((i ? ' ' : '') + c))
    );

    await sendUserMessage(testCase, 'start streaming');
    await testCase.waitForText(renderedPreChunk, 15000);

    const midStreamStore = await testCase.getStore();
    expect(midStreamStore.isProcessing).toBe(true);

    // In lite mode, /tui during processing is QUEUED (fires at turn-end).
    await typeSlashCommand(testCase, CMD_TUI);
    await testCase.waitForText('queued', 5000);

    const storeAfterQueue = await testCase.getStore();
    expect(storeAfterQueue.uiMode).toBe('lite');
    expect(storeAfterQueue.isProcessing).toBe(true);

    // Remaining events + null unblock event 4's peek and end the stream; the
    // queue then drains, firing /tui.
    await testCase.pushSendMessageResponse(
      postChunks.map((c) => assistantEvent(' ' + c))
    );
    await testCase.pushSendMessageResponse(null);

    await testCase.waitForStoreCondition((s) => s.uiMode === 'tui', 15000);
    await testCase.waitForIdle(15000);

    // waitForIdle returns the moment the queue drains (swap fires), not when
    // the new mode finished painting. Without this waitForText, getSnapshot()
    // races the TUI rerender and the screen can come back blank (~50% here).
    await testCase.waitForText(finalContent, 15000);

    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('tui');

    const allMessageText = await messageText(testCase);
    for (const c of [...preChunks, ...postChunks]) {
      expect(allMessageText).toContain(c);
    }

    expect(testCase.getSnapshot().join('\n')).toContain(finalContent);
  }, 60000);

  it('tui->lite: swap immediately after streaming, content preserved (bug 2.1/2.2)', async () => {
    // TUI mode rejects slash commands during processing (shows a warning).
    // This test swaps immediately after the stream completes, exercising the
    // same cursor-realignment code path: the TUI's static cursor has advanced
    // during streaming, and lite must pick up without losing content.
    testCase = await launchTuiE2E('mid-stream-tui-to-lite', {
      terminal: { width: 120, height: 50 },
    });

    // 6 events (first 5 render, 6th held in peek) + null to complete.
    const chunks = [
      'ALPHA_BEGIN',
      'BETA_MIDDLE',
      'GAMMA_PROGRESS',
      'DELTA_BUFFER',
      'EPSILON_TAIL',
      'ZETA_END',
    ];
    const finalContent = chunks[chunks.length - 1]!;
    await testCase.pushSendMessageResponse(
      chunks.map((c, i) => assistantEvent((i ? ' ' : '') + c))
    );
    await testCase.pushSendMessageResponse(null);

    await sendUserMessage(testCase, 'begin stream');

    await testCase.waitForText(finalContent, 15000);
    await testCase.waitForIdle(15000);

    const storeBeforeSwap = await testCase.getStore();
    expect(storeBeforeSwap.uiMode).toBe('tui');
    expect(storeBeforeSwap.isProcessing).toBe(false);

    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);
    await testCase.sleepMs(500);

    // Bug 2.1: TUI-era messages must survive the swap into the store.
    const storeAfterSwap = await testCase.getStore();
    expect(storeAfterSwap.uiMode).toBe('lite');
    let allMessageText = await messageText(testCase);
    for (const c of chunks) expect(allMessageText).toContain(c);

    // Bug 2.2: a new lite message must render (no missing first batch from a
    // stale cursor).
    const liteNewContent = 'LITE_NEW_AFTER_SWAP_MARKER';
    await testCase.pushSendMessageResponse([assistantEvent(liteNewContent)]);
    await testCase.pushSendMessageResponse(null);

    await sendUserMessage(testCase, 'new lite msg');
    await testCase.waitForText(liteNewContent, 15000);
    await testCase.waitForIdle(10000);

    expect(testCase.getSnapshot().join('\n')).toContain(liteNewContent);

    allMessageText = await messageText(testCase);
    for (const c of [...chunks, liteNewContent]) {
      expect(allMessageText).toContain(c);
    }
  }, 60000);
});
