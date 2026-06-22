/**
 * /theme reflow during a lite session: post-swap content repaints with the new
 * brand color, but already-flushed scrollback rows stay frozen at their old
 * color by design (flushed paint is immutable). RGB constants come from
 * kiroDark.ts (#C19AFF) / kiroLight.ts (#8700FF).
 *
 * Anchor: docs/design/lite-tui-action-items.md → "Hardcoded brand color bypasses /theme".
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import {
  applyTheme,
  BRAND_DARK_RGB,
  BRAND_LIGHT_RGB,
  dominantRgb,
} from './lite/helpers/theme';
import { streamReply } from './lite/helpers/responses';

describe('lite /theme reflow during stream', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('live region picks up the new brand color after /theme bundled:light', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-theme-reflow-live')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    await applyTheme(testCase, 'dark');
    await testCase.sleepMs(800);
    await applyTheme(testCase, 'light');
    // The "Theme set to ..." alert auto-hides in ~3s; don't pin assertions on it.
    await testCase.sleepMs(800);

    const liveContent = 'THEME_LIVE_AFTER_SWAP_MARKER';
    await streamReply(testCase, liveContent);

    await testCase.sendKeys('reflow probe');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText(liveContent, 15000);
    await testCase.waitForIdle(15000);

    // Lite renders the default agent tag as `kiro_default:` (raw name verbatim).
    const allTagCells = testCase.findAllTextCells('kiro_default:');
    expect(allTagCells.length).toBeGreaterThan(0);
    const lastTag = allTagCells[allTagCells.length - 1]!;
    const fg = dominantRgb(lastTag);
    expect(fg).toBe(BRAND_LIGHT_RGB);
    expect(fg).not.toBe(BRAND_DARK_RGB);
  }, 60000);

  it('already-flushed scrollback rows stay frozen at the old brand color', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('lite-theme-reflow-frozen')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.waitForSlashCommands();
    await testCase.getSessionId();

    // Force a dark baseline: the default theme varies by user settings, but the
    // first committed row must be dark so the later light row proves freezing.
    await applyTheme(testCase, 'dark');
    await testCase.sleepMs(800);

    // Turn 1 under dark; wait for stream completion so it commits to <Static>.
    const turn1Content = 'THEME_FROZEN_ROW_DARK_TURN_ONE';
    await streamReply(testCase, turn1Content);
    await testCase.sendKeys('first turn');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText(turn1Content, 15000);
    await testCase.waitForIdle(15000);

    // The flushed row must remain at index 0 (top of scrollback) after turn 2.
    const beforeSwap = testCase.findAllTextCells('kiro_default:');
    expect(beforeSwap.length).toBeGreaterThan(0);
    const frozenFgBefore = dominantRgb(beforeSwap[0]!);
    expect(frozenFgBefore).toBe(BRAND_DARK_RGB);

    // Swap to light, then run a second turn so a new agent-tag row paints
    // under the new theme.
    await applyTheme(testCase, 'light');
    await testCase.sleepMs(800);

    const turn2Content = 'THEME_FROZEN_ROW_LIGHT_TURN_TWO';
    await streamReply(testCase, turn2Content);
    await testCase.sendKeys('second turn');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText(turn2Content, 15000);
    await testCase.waitForIdle(15000);

    const afterSwap = testCase.findAllTextCells('kiro_default:');
    // At least the original row plus one new row from turn 2.
    expect(afterSwap.length).toBeGreaterThanOrEqual(beforeSwap.length + 1);
    // The first (oldest) agent-tag row keeps its old dark-theme RGB —
    // already-flushed scrollback is frozen at flush time, by design.
    const frozenFgAfter = dominantRgb(afterSwap[0]!);
    expect(frozenFgAfter).toBe(BRAND_DARK_RGB);
    // The newest agent-tag row painted under the light theme uses the new RGB.
    const liveFgAfter = dominantRgb(afterSwap[afterSwap.length - 1]!);
    expect(liveFgAfter).toBe(BRAND_LIGHT_RGB);
  }, 90000);
});
