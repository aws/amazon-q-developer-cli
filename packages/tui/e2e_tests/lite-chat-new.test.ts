/**
 * E2E test: /chat new in lite mode -- session isolation and scrollback safety.
 *
 * Validates bug-mine entries:
 *   2.3 -- liteScrollbackClearToken resets all bookkeeping refs. Prior session
 *          rows don't bleed into the new session's rendered region.
 *   2.4 -- Clear-token reset runs in the render body, not useEffect. No
 *          duplicate copy of old session appears in scrollback.
 *   2.5 -- No CSI 3J terminal wipe on /chat new. Terminal scrollback ABOVE
 *          kiro is preserved (pre-kiro content remains accessible).
 *
 * Key terminal behavior:
 *   - /chat new does NOT emit CSI 3J (terminal scrollback wipe). Old content
 *     remains in the terminal buffer above the new session.
 *   - The invariant for 2.3 is: BELOW the new session's welcome banner,
 *     no old session markers appear. Old markers may still exist above the
 *     new banner in scrollback -- that's correct and expected (2.5).
 *   - The invariant for 2.4 is: old markers appear at most ONCE in the full
 *     terminal buffer -- they are NEVER duplicated by a stale re-render.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_CHAT_NEW } from './lite/helpers/commands';

/** Helper: type a slash command char-by-char to avoid autocomplete race. */
async function typeSlashCommand(tc: E2ETestCase, command: string): Promise<void> {
  for (const char of command) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.pressEnter();
}

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

  it('prior session markers do not bleed below new session banner (2.3)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-chat-new-no-bleed')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // --- Turn 1 ---
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'SESSION1_TURN1_MARKER' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('turn one');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('SESSION1_TURN1_MARKER', 15000);
    await testCase.waitForIdle(10000);

    // --- Turn 2 ---
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'SESSION1_TURN2_MARKER' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('turn two');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('SESSION1_TURN2_MARKER', 15000);
    await testCase.waitForIdle(10000);

    // --- Turn 3 ---
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'SESSION1_TURN3_MARKER' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('turn three');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('SESSION1_TURN3_MARKER', 15000);
    await testCase.waitForIdle(10000);

    // Verify all 3 markers are visible before /chat new
    const snapBefore = testCase.getSnapshot();
    const textBefore = snapBefore.join('\n');
    expect(textBefore).toContain('SESSION1_TURN1_MARKER');
    expect(textBefore).toContain('SESSION1_TURN2_MARKER');
    expect(textBefore).toContain('SESSION1_TURN3_MARKER');

    // Record the liteScrollbackClearToken before reset
    const storeBefore = await testCase.getStore();
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // --- /chat new ---
    await typeSlashCommand(testCase, CMD_CHAT_NEW);

    // Wait for session reset: messages cleared + token bumped
    await testCase.waitForStoreCondition(
      (s) => s.messages.length === 0 && s.liteScrollbackClearToken > tokenBefore,
      15000,
    );
    await testCase.sleepMs(1000); // Let the re-render settle

    // Bug 2.3: Below the new session's KIRO banner, old markers must NOT appear.
    // The new session starts with a fresh banner. Everything below it is the
    // new session's content. Old markers may still exist ABOVE (in scrollback)
    // because we don't CSI 3J -- that's correct per bug 2.5.
    const snapAfter = testCase.getSnapshot();
    const secondBannerRow = findSecondBannerRow(snapAfter);
    expect(secondBannerRow).toBeGreaterThan(-1); // New session banner must exist

    // Extract the new session region (from the second banner onwards)
    const newSessionLines = snapAfter.slice(secondBannerRow);
    const newSessionText = newSessionLines.join('\n');
    expect(newSessionText).not.toContain('SESSION1_TURN1_MARKER');
    expect(newSessionText).not.toContain('SESSION1_TURN2_MARKER');
    expect(newSessionText).not.toContain('SESSION1_TURN3_MARKER');

    // Store confirms full reset
    const storeAfter = await testCase.getStore();
    expect(storeAfter.messages.length).toBe(0);
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
    expect(storeAfter.liteStaticSkipBefore).toBe(0);
  }, 90000);

  it('no duplicate scrollback of old session on /chat new (2.4)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-chat-new-no-duplicate')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // Complete 2 turns with unique markers
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'DUPE_CHECK_ALPHA' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('alpha msg');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('DUPE_CHECK_ALPHA', 15000);
    await testCase.waitForIdle(10000);

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'DUPE_CHECK_BETA' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('beta msg');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('DUPE_CHECK_BETA', 15000);
    await testCase.waitForIdle(10000);

    // Count occurrences of each marker before /chat new -- should be exactly 1
    const snapBefore = testCase.getSnapshot();
    const countAlphaBefore = snapBefore.filter(l => l.includes('DUPE_CHECK_ALPHA')).length;
    const countBetaBefore = snapBefore.filter(l => l.includes('DUPE_CHECK_BETA')).length;
    expect(countAlphaBefore).toBe(1);
    expect(countBetaBefore).toBe(1);

    // --- /chat new ---
    const storePre = await testCase.getStore();
    const tokenPre = storePre.liteScrollbackClearToken;
    await typeSlashCommand(testCase, CMD_CHAT_NEW);

    await testCase.waitForStoreCondition(
      (s) => s.messages.length === 0 && s.liteScrollbackClearToken > tokenPre,
      15000,
    );
    await testCase.sleepMs(1000);

    // Bug 2.4: After /chat new, no marker should appear MORE THAN ONCE in
    // the terminal buffer. Because the clear-token reset runs in the render
    // body (not useEffect), the stale refs are wiped BEFORE the first render
    // with the new empty state, preventing the old items from being re-emitted
    // a second time.
    //
    // The old markers may still appear once (in the scrollback region above
    // the new session's banner) because CSI 3J is deliberately not sent. The
    // key invariant is they are NEVER duplicated.
    const snapAfter = testCase.getSnapshot();
    const countAlphaAfter = snapAfter.filter(l => l.includes('DUPE_CHECK_ALPHA')).length;
    const countBetaAfter = snapAfter.filter(l => l.includes('DUPE_CHECK_BETA')).length;

    // Each marker appears at most once -- never duplicated by a stale re-render
    expect(countAlphaAfter).toBeLessThanOrEqual(1);
    expect(countBetaAfter).toBeLessThanOrEqual(1);

    // Additionally: below the new session's banner, neither marker appears
    const secondBannerRow = findSecondBannerRow(snapAfter);
    expect(secondBannerRow).toBeGreaterThan(-1);
    const newSessionText = snapAfter.slice(secondBannerRow).join('\n');
    expect(newSessionText).not.toContain('DUPE_CHECK_ALPHA');
    expect(newSessionText).not.toContain('DUPE_CHECK_BETA');
  }, 90000);

  it('terminal scrollback above kiro preserved -- no CSI 3J wipe (2.5)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-chat-new-no-csi3j')
      .withTerminal({ width: 120, height: 50 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // Complete one turn so there is session content
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'SCROLLBACK_PRESERVE_CHECK' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('preserve test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('SCROLLBACK_PRESERVE_CHECK', 15000);
    await testCase.waitForIdle(10000);

    // --- /chat new ---
    const storePre = await testCase.getStore();
    const tokenPre = storePre.liteScrollbackClearToken;
    await typeSlashCommand(testCase, CMD_CHAT_NEW);

    await testCase.waitForStoreCondition(
      (s) => s.messages.length === 0 && s.liteScrollbackClearToken > tokenPre,
      15000,
    );
    await testCase.sleepMs(1000);

    // Bug 2.5: The comment in LiteLayout explicitly says:
    //   "We deliberately do NOT write \x1b[3J / \x1b[2J"
    //
    // If CSI 3J were sent, the terminal buffer would be completely wiped and
    // the first session's content would vanish. Without CSI 3J, old content
    // from the first session is preserved in scrollback above the new banner.
    //
    // Verification: The first session's content (SCROLLBACK_PRESERVE_CHECK)
    // must still be present in the terminal buffer. It won't be below the new
    // banner, but it should still be somewhere in the visible buffer (since
    // the terminal is tall enough to hold both sessions).
    const snapAfter = testCase.getSnapshot();
    const fullText = snapAfter.join('\n');

    // Old session content is preserved in the buffer (not wiped by CSI 3J)
    expect(fullText).toContain('SCROLLBACK_PRESERVE_CHECK');

    // The new session's prompt must also be present (app is functional)
    const hasPrompt = snapAfter.some(l => l.includes('>'));
    expect(hasPrompt).toBe(true);

    // The new session should be functional: verify by sending a message
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'AFTER_CHAT_NEW_WORKS' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('new session msg');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('AFTER_CHAT_NEW_WORKS', 15000);
    await testCase.waitForIdle(10000);

    // The response from the new session must be visible
    const snapFinal = testCase.getSnapshot();
    expect(snapFinal.join('\n')).toContain('AFTER_CHAT_NEW_WORKS');

    // And the old session content is STILL preserved (not wiped retroactively)
    expect(snapFinal.join('\n')).toContain('SCROLLBACK_PRESERVE_CHECK');
  }, 90000);
});
