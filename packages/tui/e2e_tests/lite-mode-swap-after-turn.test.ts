/**
 * Mode-swapping (/tui and /lite) while the agent is active. Bug-mine:
 * - 2.1: cursor realignment via useLayoutEffect (first lite message must paint).
 * - 2.2: useLayoutEffect (not useEffect) for cursor reset (no missing first batch).
 * - 2.6: tui→lite sets liteScrollbackClearToken=messages.length (no duplicate scrollback).
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
} from './lite/helpers/commands';
import { driveTurn } from './lite/helpers/responses';

interface SwapCase {
  name: string;
  testName: string;
  launch: (n: string) => Promise<E2ETestCase>;
  swapCmd: string;
  target: 'lite' | 'tui';
  /** [content, prompt] pairs driven before the swap. */
  preTurns: Array<[string, string]>;
  /** Marker driven after the swap; must render on screen. */
  postMarker: string;
  /** Cold swap also bumps liteScrollbackClearToken and keeps >=4 messages. */
  assertColdClear?: boolean;
}

const CASES: SwapCase[] = [
  {
    name: 'lite→tui swap: content rendered in lite is preserved in store after swap',
    testName: 'swap-lite-to-tui',
    launch: launchLiteE2E,
    swapCmd: CMD_TUI,
    target: 'tui',
    preTurns: [['LITE_RESPONSE_MARKER', 'hello']],
    postMarker: 'TUI_AFTER_SWAP',
  },
  {
    name: 'tui→lite swap: first lite message appears (cursor realignment, bug 2.1/2.2)',
    testName: 'swap-tui-to-lite-cursor',
    launch: launchTuiE2E,
    swapCmd: CMD_LITE,
    target: 'lite',
    preTurns: [['TUI_CONTENT_BEFORE_SWAP', 'hello tui']],
    // Bug 2.1/2.2: the first message sent in lite mode must actually render —
    // if the cursor wasn't realigned via useLayoutEffect, the first lite batch
    // would silently never paint because twinki's bridge still holds the old
    // totalStaticWritten from TUI's renders.
    postMarker: 'LITE_AFTER_SWAP_MARKER',
  },
  {
    name: 'tui→lite cold swap: liteScrollbackClearToken bumped, new messages render (bug 2.6)',
    testName: 'swap-tui-to-lite-cold',
    launch: launchTuiE2E,
    swapCmd: CMD_LITE,
    target: 'lite',
    preTurns: [
      ['TUI_TURN_ONE_REPLY', 'turn one'],
      ['TUI_TURN_TWO_REPLY', 'turn two'],
    ],
    postMarker: 'LITE_NEW_REPLY',
    assertColdClear: true,
  },
];

describe('lite mode swap after turn [bug-mine 2.1, 2.2, 2.6]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it.each(CASES)(
    '$name',
    async ({
      testName,
      launch,
      swapCmd,
      target,
      preTurns,
      postMarker,
      assertColdClear,
    }) => {
      testCase = await launch(testName);

      for (const [content, prompt] of preTurns) {
        await driveTurn(testCase, content, prompt);
      }

      await typeSlashCommand(testCase, swapCmd);
      await testCase.waitForStoreCondition((s) => s.uiMode === target, 10000);
      await testCase.sleepMs(assertColdClear ? 1000 : 500);

      const storeAfterSwap = await testCase.getStore();
      expect(storeAfterSwap.uiMode).toBe(target);
      // Pre-swap messages survive into the store (bug 2.1).
      const preContent = preTurns[0]![0];
      const survived = storeAfterSwap.messages.some((m) =>
        JSON.stringify(m).includes(preContent)
      );
      expect(survived).toBe(true);

      if (assertColdClear) {
        // Bug 2.6: setUiMode clears scrollback and re-renders from scratch;
        // liteScrollbackClearToken is bumped so LiteLayout/ConversationView wipe
        // their singletons and the terminal is cleared.
        expect(storeAfterSwap.liteScrollbackClearToken).toBeGreaterThan(0);
        expect(storeAfterSwap.messages.length).toBeGreaterThanOrEqual(4);
      }

      await driveTurn(testCase, postMarker, 'after swap');

      expect(testCase.getSnapshot().join('\n')).toContain(postMarker);
    },
    60000
  );
});
