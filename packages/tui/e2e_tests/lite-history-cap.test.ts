/**
 * E2E test: lite history cap on session resume [bug-mine 2.7]
 *
 * Validates that LITE_HISTORY_RENDER_CAP (70) clamps the number of messages
 * rendered into <Static> when resuming a long session. The store still holds
 * the full history for context-window accounting — only the painted slice is
 * bounded.
 *
 * Strategy:
 *   1. Use AcpTestHelper to pre-populate a session with ~80 messages (40 turns
 *      of user+assistant pairs).
 *   2. Launch the TUI in lite mode and load that session via /chat.
 *   3. Assert liteStaticSkipBefore is correctly set (messages.length - 70).
 *   4. Assert that early messages are NOT visible in the terminal snapshot.
 *   5. Assert that the most recent messages ARE visible.
 *   6. Send a new message after resume to verify live appends still work.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { LITE_HISTORY_RENDER_CAP } from '../src/components/layout/lite/static-flush';
import { CMD_CHAT } from './lite/helpers/commands';

describe('lite history cap [bug-mine 2.7]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('caps rendered history at LITE_HISTORY_RENDER_CAP on session resume', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('lite-history-cap')
      .withLite()
      .launch();

    // Create a session with many messages via AcpTestHelper.
    // 40 turns * (1 user prompt + 1 assistant response) = ~80 messages in the store,
    // which exceeds the cap of 70.
    const acp = await testCase.launchAcpHelper();
    const sessionId = await acp.newSession();

    const totalTurns = 40;
    for (let i = 0; i < totalTurns; i++) {
      const marker = `HIST_MSG_${String(i).padStart(3, '0')}`;
      await acp.pushResponse(sessionId, [
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: `Response ${marker}` } } },
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

    // Give the TUI time to start loading the session.
    await testCase.sleepMs(2000);

    // Wait for the session to load — the most recent response should be visible.
    const lastMarker = `HIST_MSG_${String(totalTurns - 1).padStart(3, '0')}`;
    await testCase.waitForText(`Response ${lastMarker}`, 90000);

    // Verify the store has ALL messages (full history preserved).
    const store = await testCase.getStore();
    // Each turn creates at least 2 messages (User + Model). Total should exceed the cap.
    expect(store.messages.length).toBeGreaterThan(LITE_HISTORY_RENDER_CAP);

    // Verify liteStaticSkipBefore is set correctly: messages.length - 70.
    const expectedSkip = Math.max(0, store.messages.length - LITE_HISTORY_RENDER_CAP);
    expect(store.liteStaticSkipBefore).toBe(expectedSkip);
    expect(store.liteStaticSkipBefore).toBeGreaterThan(0);

    // Verify that early messages (those below the cap boundary) are NOT in the terminal.
    const snapshot = testCase.getSnapshot();
    const allText = snapshot.join('\n');

    // The first few messages should be skipped (not rendered).
    expect(allText).not.toContain('HIST_MSG_000');
    expect(allText).not.toContain('HIST_MSG_001');
    expect(allText).not.toContain('HIST_MSG_002');

    // The last message should definitely be visible.
    expect(allText).toContain(`Response ${lastMarker}`);

    // Verify that new messages sent AFTER resume still render correctly.
    // The liteStaticSkipBefore is a static lower bound; live turns past it
    // render normally.
    //
    // Confirm the store's sessionId matches the resumed session before pushing
    // mock responses — pushSendMessageResponse routes via store.sessionId.
    const storeAfterResume = await testCase.getStore();
    expect(storeAfterResume.sessionId).toBe(sessionId);

    // Wait for the TUI to be fully idle after session resume before
    // sending a new prompt.
    await testCase.waitForIdle(15000);
    await testCase.sleepMs(500);

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'POST_RESUME_LIVE_MSG' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('post resume check');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('POST_RESUME_LIVE_MSG', 30000);

    const snap2 = testCase.getSnapshot();
    expect(snap2.some(line => line.includes('POST_RESUME_LIVE_MSG'))).toBe(true);
  }, 240000);
});
