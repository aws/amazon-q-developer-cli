/**
 * Queued follow-up messages survive a mode swap and drain correctly in the new
 * mode. In lite, known slash commands typed while processing are queued (not
 * rejected); processQueue drains FIFO on turn completion, so a queued mode-swap
 * command changes the mode mid-drain and later queued messages fire in the new mode.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_LITE, CMD_TUI, typeSlashCommand } from './lite/helpers/commands';
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
    testCase = await E2ETestCase.builder()
      .withTestName('queue-survival-lite-to-tui')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // --- Turn 1: complete a full turn to warm the session ---
    await streamReply(testCase, 'Turn one done.');

    await testCase.sendKeys('warm up');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Turn one done', 15000);
    await testCase.waitForIdle(10000);

    // --- Turn 2: start processing (keepOpen = stream stays open, no null) ---
    await streamReply(testCase, 'Still thinking.', { keepOpen: true });

    await testCase.sendKeys('turn two');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Confirm isProcessing is true
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // --- Queue /tui while processing (lite mode queues known slash commands) ---
    // trailingSpace so the slash command menu doesn't intercept Enter; without
    // it the menu handles Enter and doesn't clear PromptInput's segments buffer.
    await typeSlashCommand(testCase, CMD_TUI, { trailingSpace: true });

    // Wait for the "queued" transient alert (confirms submit + clear)
    await testCase.waitForText('queued', 5000);

    // Confirm /tui is in the queue
    store = await testCase.getStore();
    expect(store.queuedMessages).toContain(CMD_TUI);
    await testCase.sleepMs(300);

    // --- Queue a follow-up message while still processing ---
    await testCase.sendKeys('QUEUED_FOLLOWUP');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Wait for the follow-up to appear in the queue
    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 2,
      5000
    );

    // Confirm both are queued in correct order
    store = await testCase.getStore();
    expect(store.queuedMessages[0]).toBe(CMD_TUI);
    expect(store.queuedMessages[1]).toBe('QUEUED_FOLLOWUP');
    expect(store.uiMode).toBe('lite');

    // --- Complete turn 2 (push null) → processQueue drains ---
    // First: /tui fires → mode swaps to TUI
    // Then: QUEUED_FOLLOWUP fires as sendMessage in TUI mode
    // Prepare the response for QUEUED_FOLLOWUP:
    await testCase.pushSendMessageResponse(null); // end turn 2
    await streamReply(testCase, 'RESPONSE_IN_TUI_MODE'); // end queued message turn

    // Wait for the queued message response to appear
    await testCase.waitForText('RESPONSE_IN_TUI_MODE', 20000);
    await testCase.waitForIdle(15000);

    // --- Final assertions ---
    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('tui');
    expect(finalStore.queuedMessages).toEqual([]);
    expect(finalStore.isProcessing).toBe(false);

    // The queued message's response exists in messages
    const hasResponse = finalStore.messages.some((m) =>
      JSON.stringify(m).includes('RESPONSE_IN_TUI_MODE')
    );
    expect(hasResponse).toBe(true);
  }, 60000);

  it('tui -> lite: queued /tui + /lite + message drains in lite mode', async () => {
    // Start in lite, then queue: /tui (swap to tui), /lite (swap back), message.
    // The message should fire in lite mode after both swaps execute.
    testCase = await E2ETestCase.builder()
      .withTestName('queue-survival-tui-to-lite')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // --- Turn 1: warm up ---
    await streamReply(testCase, 'Warm up done.');

    await testCase.sendKeys('warm up');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Warm up done', 15000);
    await testCase.waitForIdle(10000);

    // --- Turn 2: start processing (keepOpen = stream stays open) ---
    await streamReply(testCase, 'Processing.', { keepOpen: true });

    await testCase.sendKeys('turn two');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Confirm isProcessing
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // --- Queue /tui (lite→tui swap) ---
    // trailingSpace prevents the slash menu from intercepting Enter.
    await typeSlashCommand(testCase, CMD_TUI, { trailingSpace: true });

    // Wait for the "queued" alert (confirms submit + input cleared)
    await testCase.waitForText('queued', 5000);
    await testCase.sleepMs(300);

    // --- Queue /lite (tui→lite swap back) ---
    await typeSlashCommand(testCase, CMD_LITE, { trailingSpace: true });

    // Wait for /lite to appear in the queue
    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 2 && s.queuedMessages[1] === CMD_LITE,
      5000
    );
    await testCase.sleepMs(300);

    // --- Queue a follow-up message ---
    await testCase.sendKeys('QUEUED_MSG_LITE');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Wait for the message to appear in the queue
    await testCase.waitForStoreCondition(
      (s) => s.queuedMessages.length >= 3,
      5000
    );

    // Confirm all three are queued in order
    store = await testCase.getStore();
    expect(store.queuedMessages[0]).toBe(CMD_TUI);
    expect(store.queuedMessages[1]).toBe(CMD_LITE);
    expect(store.queuedMessages[2]).toBe('QUEUED_MSG_LITE');
    expect(store.uiMode).toBe('lite');

    // --- Complete turn 2 → processQueue drains ---
    // /tui fires (mode→tui), /lite fires (mode→lite), QUEUED_MSG_LITE fires
    await testCase.pushSendMessageResponse(null); // end turn 2
    await streamReply(testCase, 'RESPONSE_BACK_IN_LITE'); // end queued message turn

    // Wait for the queued message response
    await testCase.waitForText('RESPONSE_BACK_IN_LITE', 20000);
    await testCase.waitForIdle(15000);

    // --- Final assertions ---
    const finalStore = await testCase.getStore();
    expect(finalStore.uiMode).toBe('lite');
    expect(finalStore.queuedMessages).toEqual([]);
    expect(finalStore.isProcessing).toBe(false);

    // Response exists in messages
    const hasResponse = finalStore.messages.some((m) =>
      JSON.stringify(m).includes('RESPONSE_BACK_IN_LITE')
    );
    expect(hasResponse).toBe(true);

    // Verify on-screen rendering in lite mode
    const snap = testCase.getSnapshot();
    expect(snap.join('\n')).toContain('RESPONSE_BACK_IN_LITE');
  }, 60000);
});
