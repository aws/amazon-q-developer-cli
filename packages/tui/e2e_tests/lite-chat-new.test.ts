/**
 * /chat new in lite mode -- session isolation and scrollback safety. Bug-mine:
 *   2.3 -- liteScrollbackClearToken resets bookkeeping; BELOW the new banner no
 *          old session markers appear (old markers ABOVE the banner are expected).
 *   2.4 -- clear-token reset runs in the render body, not useEffect; old markers
 *          appear at most ONCE (never duplicated by a stale re-render).
 *   2.5 -- /chat new emits NO CSI 3J wipe; terminal scrollback above kiro survives.
 *
 * All three are non-conflicting observations of the same post-/chat-new state,
 * so they share one drive: seed 3 turns, /chat new once, then assert each.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_CHAT_NEW,
  launchLiteE2E,
  typeSlashCommand,
  sendUserMessage,
} from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

/**
 * Find the row index of the SECOND KIRO banner (the new session's banner).
 * After /chat new, a fresh KIRO banner is emitted. Everything below this
 * row belongs to the new session and must not contain old session content.
 */
function findSecondBannerRow(lines: string[]): number {
  let bannerCount = 0;
  for (let i = 0; i < lines.length; i++) {
    // The KIRO ASCII banner's distinctive first line
    if (lines[i]!.includes('_  _____ ____   ___')) {
      bannerCount++;
      if (bannerCount === 2) return i;
    }
  }
  return -1;
}

describe('lite /chat new session isolation [bug-mine 2.3, 2.4, 2.5]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('resets session, preserves scrollback, no bleed/duplicate below banner', async () => {
    testCase = await launchLiteE2E('lite-chat-new', {
      terminal: { width: 120, height: 50 },
    });

    const markers = [
      'SESSION1_TURN1_MARKER',
      'SESSION1_TURN2_MARKER',
      'SESSION1_TURN3_MARKER',
    ];
    for (let i = 0; i < markers.length; i++) {
      await streamReply(testCase, markers[i]!);
      await sendUserMessage(testCase, `turn ${i + 1}`);
      await testCase.waitForText(markers[i]!, 15000);
      await testCase.waitForIdle(10000);
    }

    // Before /chat new each marker is on screen exactly once.
    const snapBefore = testCase.getSnapshot();
    for (const m of markers) {
      expect(snapBefore.filter((l) => l.includes(m)).length).toBe(1);
    }

    const tokenBefore = (await testCase.getStore()).liteScrollbackClearToken;
    await typeSlashCommand(testCase, CMD_CHAT_NEW);

    // Wait for session reset: messages cleared + token bumped.
    await testCase.waitForStoreCondition(
      (s) =>
        s.messages.length === 0 && s.liteScrollbackClearToken > tokenBefore,
      15000
    );
    await testCase.sleepMs(1000); // Let the re-render settle.

    const snapAfter = testCase.getSnapshot();
    const fullText = snapAfter.join('\n');
    const secondBannerRow = findSecondBannerRow(snapAfter);
    expect(secondBannerRow).toBeGreaterThan(-1); // New session banner must exist.
    const newSessionText = snapAfter.slice(secondBannerRow).join('\n');

    for (const m of markers) {
      // 2.3: below the new banner, old markers must NOT appear.
      expect(newSessionText).not.toContain(m);
      // 2.4: clear-token reset runs in the render body, so the old marker is
      // never duplicated (appears at most once, in the scrollback above).
      expect(snapAfter.filter((l) => l.includes(m)).length).toBeLessThanOrEqual(
        1
      );
      // 2.5: CSI 3J is deliberately not sent, so old content survives above.
      expect(fullText).toContain(m);
    }

    // Store confirms full reset.
    const storeAfter = await testCase.getStore();
    expect(storeAfter.messages.length).toBe(0);
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.liteStaticSkipBefore).toBe(0);
    expect(snapAfter.some((l) => l.includes('>'))).toBe(true);

    // 2.5: a new turn works after /chat new, and old content stays preserved.
    await streamReply(testCase, 'AFTER_CHAT_NEW_WORKS');
    await sendUserMessage(testCase, 'new session msg');
    await testCase.waitForText('AFTER_CHAT_NEW_WORKS', 15000);
    await testCase.waitForIdle(10000);

    const snapFinal = testCase.getSnapshot().join('\n');
    expect(snapFinal).toContain('AFTER_CHAT_NEW_WORKS');
    expect(snapFinal).toContain(markers[0]!);
  }, 120000);
});
