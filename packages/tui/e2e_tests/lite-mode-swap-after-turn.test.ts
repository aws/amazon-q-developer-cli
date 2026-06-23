/**
 * Mode-swapping (/tui and /lite) while the agent is active. Bug-mine:
 * - 2.1: cursor realignment via useLayoutEffect (first lite message must paint).
 * - 2.2: useLayoutEffect (not useEffect) for cursor reset (no missing first batch).
 * - 2.6: tui→lite sets liteStaticSkipBefore=messages.length (no duplicate scrollback).
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

describe('lite mode swap after turn [bug-mine 2.1, 2.2, 2.6]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite→tui swap: content rendered in lite is preserved in store after swap', async () => {
    testCase = await launchLiteE2E('swap-lite-to-tui', {
      terminal: { width: 120, height: 50 },
    });

    await streamReply(testCase, 'LITE_RESPONSE_MARKER');

    await sendUserMessage(testCase, 'hello');
    await testCase.waitForText('LITE_RESPONSE_MARKER', 15000);
    await testCase.waitForIdle(10000);

    await typeSlashCommand(testCase, CMD_TUI);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'tui', 10000);
    await testCase.sleepMs(500);

    // Bug 2.1: lite-era messages are preserved in store after the swap.
    const store = await testCase.getStore();
    expect(store.uiMode).toBe('tui');
    const hasLiteContent = store.messages.some((m) =>
      JSON.stringify(m).includes('LITE_RESPONSE_MARKER')
    );
    expect(hasLiteContent).toBe(true);

    await streamReply(testCase, 'TUI_AFTER_SWAP');

    await sendUserMessage(testCase, 'tui msg');
    await testCase.waitForText('TUI_AFTER_SWAP', 15000);
    await testCase.waitForIdle(10000);

    const snap = testCase.getSnapshot();
    expect(snap.join('\n')).toContain('TUI_AFTER_SWAP');
  }, 60000);

  it('tui→lite swap: first lite message appears (cursor realignment, bug 2.1/2.2)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('swap-tui-to-lite-cursor')
      .withTerminal({ width: 120, height: 50 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // Complete a turn in TUI mode (advances the static cursor)
    await streamReply(testCase, 'TUI_CONTENT_BEFORE_SWAP');

    await sendUserMessage(testCase, 'hello tui');
    await testCase.waitForText('TUI_CONTENT_BEFORE_SWAP', 15000);
    await testCase.waitForIdle(10000);

    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);
    await testCase.sleepMs(500);

    // Bug 2.1/2.2: The first message sent in lite mode must actually render.
    // If the cursor wasn't realigned via useLayoutEffect, the first lite
    // batch would silently never paint because twinki's bridge still holds
    // the old totalStaticWritten from TUI's renders.
    await streamReply(testCase, 'LITE_AFTER_SWAP_MARKER');

    await sendUserMessage(testCase, 'first lite');
    await testCase.waitForText('LITE_AFTER_SWAP_MARKER', 15000);
    await testCase.waitForIdle(10000);

    // The new lite message must be visible on screen (bug 2.1 fix).
    const snap = testCase.getSnapshot();
    const allText = snap.join('\n');
    expect(allText).toContain('LITE_AFTER_SWAP_MARKER');

    const store = await testCase.getStore();
    expect(store.uiMode).toBe('lite');
  }, 60000);

  it('tui→lite cold swap: liteScrollbackClearToken bumped, new messages render (bug 2.6)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('swap-tui-to-lite-cold')
      .withTerminal({ width: 120, height: 50 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    await streamReply(testCase, 'TUI_TURN_ONE_REPLY');

    await sendUserMessage(testCase, 'turn one');
    await testCase.waitForText('TUI_TURN_ONE_REPLY', 15000);
    await testCase.waitForIdle(10000);

    await streamReply(testCase, 'TUI_TURN_TWO_REPLY');

    await sendUserMessage(testCase, 'turn two');
    await testCase.waitForText('TUI_TURN_TWO_REPLY', 15000);
    await testCase.waitForIdle(10000);

    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);
    await testCase.sleepMs(1000);

    // Bug 2.6: setUiMode clears scrollback and re-renders from scratch.
    // liteScrollbackClearToken is bumped so LiteLayout/ConversationView wipe
    // their singletons and the terminal is cleared.
    const storeAfterSwap = await testCase.getStore();
    expect(storeAfterSwap.uiMode).toBe('lite');
    expect(storeAfterSwap.liteScrollbackClearToken).toBeGreaterThan(0);

    // All TUI messages survive the swap (not lost).
    const msgCount = storeAfterSwap.messages.length;
    expect(msgCount).toBeGreaterThanOrEqual(4); // 2 user + 2 assistant at minimum

    await streamReply(testCase, 'LITE_NEW_REPLY');

    await sendUserMessage(testCase, 'new lite msg');
    await testCase.waitForText('LITE_NEW_REPLY', 15000);
    await testCase.waitForIdle(10000);

    const snap = testCase.getSnapshot();
    const allText = snap.join('\n');
    expect(allText).toContain('LITE_NEW_REPLY');
  }, 60000);
});
