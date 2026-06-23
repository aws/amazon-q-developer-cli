/**
 * Real mid-stream mode swap: a /tui command typed while a lite turn is streaming
 * is QUEUED and fires at turn-end (the tui<->lite content-preservation half of
 * bug-mine 2.1/2.2 is covered by lite-mode-swap-after-turn.test.ts).
 *
 * WHY N+1 events: the RTS ResponseParser uses 1-lookahead (after an
 * AssistantResponseEvent it peeks the NEXT event for a CodeReferenceEvent), so
 * event N only becomes visible once event N+1 arrives.
 */

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_TUI,
  launchLiteE2E,
  typeSlashCommand,
  sendUserMessage,
} from './lite/helpers/commands';
import { assistantEvent, messageText } from './lite/helpers/responses';

describe('lite mid-stream slash-command queueing', () => {
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
});
