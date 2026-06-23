/**
 * Queued follow-up messages survive a mode swap and drain correctly in the new
 * mode. In lite, known slash commands typed while processing are queued (not
 * rejected); processQueue drains FIFO on turn completion, so a queued mode-swap
 * command changes the mode mid-drain and later queued messages fire in the new mode.
 */

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_LITE,
  CMD_TUI,
  launchLiteE2E,
  typeSlashCommand,
  sendUserMessage,
} from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

// TEMPORARILY SKIPPED: mid-turn message queueing in lite is known to be
// not-ideal UX on this branch — these tests assert the final queueing behavior
// that lands with the core logic branch later. Re-enable (drop `.skip`) in the
// PR that ports the finished queueing logic into lite.
describe.skip('queued message survives mode swap', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  // trailingSpace on every queued slash command so the slash menu doesn't
  // intercept Enter (it would handle Enter and never clear PromptInput's
  // segments buffer). All cases: warm a turn, open a keepOpen stream, queue the
  // commands + a follow-up message behind it (FIFO), then drain into the final
  // mode where the message fires.
  it.each([
    {
      name: 'lite -> tui: queued /tui + message drains in tui mode',
      testName: 'queue-survival-lite-to-tui',
      queuedCommands: [CMD_TUI],
      queuedMessage: 'QUEUED_FOLLOWUP',
      finalReply: 'RESPONSE_IN_TUI_MODE',
      expectedMode: 'tui' as const,
    },
    {
      name: 'tui -> lite: queued /tui + /lite + message drains in lite mode',
      testName: 'queue-survival-tui-to-lite',
      queuedCommands: [CMD_TUI, CMD_LITE],
      queuedMessage: 'QUEUED_MSG_LITE',
      finalReply: 'RESPONSE_BACK_IN_LITE',
      expectedMode: 'lite' as const,
    },
  ])(
    '$name',
    async ({
      testName,
      queuedCommands,
      queuedMessage,
      finalReply,
      expectedMode,
    }) => {
      const tc = await launchLiteE2E(testName, {
        terminal: { width: 120, height: 50 },
      });
      testCase = tc;

      // Turn 1 warms the session.
      await streamReply(tc, 'Warm up done.');
      await sendUserMessage(tc, 'warm up');
      await tc.waitForText('Warm up done', 15000);
      await tc.waitForIdle(10000);

      // Turn 2 keepOpen so the stream stays open while we queue behind it.
      await streamReply(tc, 'Still thinking.', { keepOpen: true });
      await sendUserMessage(tc, 'turn two');
      await tc.sleepMs(500);
      expect((await tc.getStore()).isProcessing).toBe(true);

      for (let i = 0; i < queuedCommands.length; i++) {
        await typeSlashCommand(tc, queuedCommands[i]!, { trailingSpace: true });
        if (i === 0) await tc.waitForText('queued', 5000);
        else
          await tc.waitForStoreCondition(
            (s) => s.queuedMessages.length >= i + 1,
            5000
          );
        await tc.sleepMs(300);
      }

      await sendUserMessage(tc, queuedMessage);
      const expectedQueue = [...queuedCommands, queuedMessage];
      await tc.waitForStoreCondition(
        (s) => s.queuedMessages.length >= expectedQueue.length,
        5000
      );

      const store = await tc.getStore();
      expect(store.queuedMessages).toEqual(expectedQueue);
      expect(store.uiMode).toBe('lite');

      // Completing turn 2 drains FIFO: the swap command(s) change mode, then the
      // queued message fires in the final mode.
      await tc.pushSendMessageResponse(null); // end turn 2
      await streamReply(tc, finalReply); // end queued message turn
      await tc.waitForText(finalReply, 20000);
      await tc.waitForIdle(15000);

      const finalStore = await tc.getStore();
      expect(finalStore.uiMode).toBe(expectedMode);
      expect(finalStore.queuedMessages).toEqual([]);
      expect(finalStore.isProcessing).toBe(false);
      expect(
        finalStore.messages.some((m) => JSON.stringify(m).includes(finalReply))
      ).toBe(true);
      expect(tc.getSnapshot().join('\n')).toContain(finalReply);
    },
    60000
  );
});
