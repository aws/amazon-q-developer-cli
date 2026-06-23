/**
 * E2E test: lite history cap on session resume [bug-mine 2.7]
 *
 * Validates that LITE_HISTORY_RENDER_CAP (70) clamps the number of messages
 * rendered into <Static> when resuming a long session. The store still holds
 * the full history for context-window accounting — only the painted slice is
 * bounded.
 */

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import { LITE_HISTORY_RENDER_CAP } from '../src/components/layout/lite/static-flush';
import { CMD_CHAT, sendUserMessage } from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

describe('lite history cap [bug-mine 2.7]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it('caps rendered history at LITE_HISTORY_RENDER_CAP on session resume', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('lite-history-cap')
      .withLite()
      .launch();

    // 40 turns * (user + assistant) = ~80 messages, exceeding the cap of 70.
    const acp = await testCase.launchAcpHelper();
    const sessionId = await acp.newSession();

    const totalTurns = 40;
    for (let i = 0; i < totalTurns; i++) {
      const marker = `HIST_MSG_${String(i).padStart(3, '0')}`;
      await acp.pushResponse(sessionId, [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: `Response ${marker}` },
          },
        },
      ]);
      await acp.pushResponse(sessionId, null);
      await acp.prompt(sessionId, `Turn ${marker}`);
    }

    await acp.terminateSession(sessionId);
    await acp.close();

    // Wait for TUI to be ready, then load the pre-created session via /chat.
    await testCase.waitForText('>', 20000);
    await testCase.waitForSlashCommands(15000);

    for (const char of CMD_CHAT) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(50);
    }
    await testCase.pressEnter();

    // Wait for the session to appear in the picker (title is the first prompt).
    await testCase.waitForText('Turn HIST_MSG_000', 15000);
    await testCase.pressEnter();

    await testCase.sleepMs(2000);

    const lastMarker = `HIST_MSG_${String(totalTurns - 1).padStart(3, '0')}`;
    await testCase.waitForText(`Response ${lastMarker}`, 90000);

    // Store holds the full history; only the painted slice is bounded.
    const store = await testCase.getStore();
    expect(store.messages.length).toBeGreaterThan(LITE_HISTORY_RENDER_CAP);

    const expectedSkip = Math.max(
      0,
      store.messages.length - LITE_HISTORY_RENDER_CAP
    );
    expect(store.liteStaticSkipBefore).toBe(expectedSkip);
    expect(store.liteStaticSkipBefore).toBeGreaterThan(0);

    const snapshot = testCase.getSnapshot();
    const allText = snapshot.join('\n');

    // Early messages below the cap boundary are skipped; the latest is painted.
    expect(allText).not.toContain('HIST_MSG_000');
    expect(allText).not.toContain('HIST_MSG_001');
    expect(allText).not.toContain('HIST_MSG_002');
    expect(allText).toContain(`Response ${lastMarker}`);

    // liteStaticSkipBefore is a static lower bound; live turns past it render
    // normally. Confirm the resumed sessionId matches before pushing mock
    // responses — pushSendMessageResponse routes via store.sessionId.
    const storeAfterResume = await testCase.getStore();
    expect(storeAfterResume.sessionId).toBe(sessionId);

    await testCase.waitForIdle(15000);
    await testCase.sleepMs(500);

    await streamReply(testCase, 'POST_RESUME_LIVE_MSG');

    await sendUserMessage(testCase, 'post resume check');
    await testCase.waitForText('POST_RESUME_LIVE_MSG', 30000);

    const snap2 = testCase.getSnapshot();
    expect(snap2.some((line) => line.includes('POST_RESUME_LIVE_MSG'))).toBe(
      true
    );
  }, 240000);
});
