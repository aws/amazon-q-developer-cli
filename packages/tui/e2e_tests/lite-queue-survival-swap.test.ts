/**
 * Queued follow-up messages survive a mode swap and drain correctly in the new
 * mode. In lite, known slash commands typed while processing are queued (not
 * rejected); processQueue drains FIFO on turn completion, so a queued mode-swap
 * command changes the mode mid-drain and later queued messages fire in the new mode.
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
import { streamReply } from './lite/helpers/responses';

describe('queued message survives mode swap', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite -> tui: queued /tui + message drains in tui mode', async () => {
    testCase = await launchLiteE2E('queue-survival-lite-to-tui', {
      terminal: { width: 120, height: 50 },
    });

    // Turn 1 warms the session.
    await streamReply(testCase, 'Turn one done.');

    await sendUserMessage(testCase, 'warm up');
    await testCase.waitForText('Turn one done', 15000);
    await testCase.waitForIdle(10000);

    // Turn 2 keepOpen so the stream stays open while we queue behind it.
    await streamReply(testCase, 'Still thinking.', { keepOpen: true });

    await sendUserMessage(testCase, 'turn two');
    await testCase.sleepMs(500);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // trailingSpace so the slash menu doesn't intercept Enter; without it the
    // menu handles Enter and never clears PromptInput's segments buffer.
    await typeSlashCommand(testCase, CMD_TUI, { trailingSpace: true });

    await testCase.waitForText('queued', 5000);

    store = await testCase.getStore();
    expect(store.queuedMessages).toContain(CMD_TUI);
    await testCase.sleepMs(300);

    await sendUserMessage(testCase, 'QUEUED_FOLLOWUP');
    await testCase.sleepMs(500);

    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 2,
      5000
    );

    store = await testCase.getStore();
    expect(store.queuedMessages[0]).toBe(CMD_TUI);
    expect(store.queuedMessages[1]).toBe('QUEUED_FOLLOWUP');
    expect(store.uiMode).toBe('lite');

    // Completing turn 2 drains FIFO: /tui swaps to TUI, then QUEUED_FOLLOWUP
    // fires as sendMessage in the new (TUI) mode.
    await testCase.pushSendMessageResponse(null); // end turn 2
    await streamReply(testCase, 'RESPONSE_IN_TUI_MODE'); // end queued message turn

    await testCase.waitForText('RESPONSE_IN_TUI_MODE', 20000);
    await testCase.waitForIdle(15000);

    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('tui');
    expect(finalStore.queuedMessages).toEqual([]);
    expect(finalStore.isProcessing).toBe(false);

    const hasResponse = finalStore.messages.some((m) =>
      JSON.stringify(m).includes('RESPONSE_IN_TUI_MODE')
    );
    expect(hasResponse).toBe(true);
  }, 60000);

  it('tui -> lite: queued /tui + /lite + message drains in lite mode', async () => {
    // Start in lite, then queue: /tui (swap to tui), /lite (swap back), message.
    // The message should fire in lite mode after both swaps execute.
    testCase = await launchLiteE2E('queue-survival-tui-to-lite', {
      terminal: { width: 120, height: 50 },
    });

    await streamReply(testCase, 'Warm up done.');

    await sendUserMessage(testCase, 'warm up');
    await testCase.waitForText('Warm up done', 15000);
    await testCase.waitForIdle(10000);

    await streamReply(testCase, 'Processing.', { keepOpen: true });

    await sendUserMessage(testCase, 'turn two');
    await testCase.sleepMs(500);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // trailingSpace prevents the slash menu from intercepting Enter.
    await typeSlashCommand(testCase, CMD_TUI, { trailingSpace: true });

    await testCase.waitForText('queued', 5000);
    await testCase.sleepMs(300);

    await typeSlashCommand(testCase, CMD_LITE, { trailingSpace: true });

    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 2 && s.queuedMessages[1] === CMD_LITE,
      5000
    );
    await testCase.sleepMs(300);

    await sendUserMessage(testCase, 'QUEUED_MSG_LITE');
    await testCase.sleepMs(500);

    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 3,
      5000
    );

    store = await testCase.getStore();
    expect(store.queuedMessages[0]).toBe(CMD_TUI);
    expect(store.queuedMessages[1]).toBe(CMD_LITE);
    expect(store.queuedMessages[2]).toBe('QUEUED_MSG_LITE');
    expect(store.uiMode).toBe('lite');

    // Drain order: /tui (mode→tui), /lite (mode→lite), then QUEUED_MSG_LITE
    // fires in lite mode.
    await testCase.pushSendMessageResponse(null); // end turn 2
    await streamReply(testCase, 'RESPONSE_BACK_IN_LITE'); // end queued message turn

    await testCase.waitForText('RESPONSE_BACK_IN_LITE', 20000);
    await testCase.waitForIdle(15000);

    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('lite');
    expect(finalStore.queuedMessages).toEqual([]);
    expect(finalStore.isProcessing).toBe(false);

    const hasResponse = finalStore.messages.some((m) =>
      JSON.stringify(m).includes('RESPONSE_BACK_IN_LITE')
    );
    expect(hasResponse).toBe(true);

    const snap = testCase.getSnapshot();
    expect(snap.join('\n')).toContain('RESPONSE_BACK_IN_LITE');
  }, 60000);
});
