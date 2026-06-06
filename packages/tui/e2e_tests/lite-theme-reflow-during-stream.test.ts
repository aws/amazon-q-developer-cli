/**
 * E2E test: /theme reflow during a lite-mode session.
 *
 * 1. WHAT user-observable behavior does this assert?
 *    Per docs/design/lite-tui-action-items.md ("Hardcoded brand color
 *    bypasses /theme"): when the user runs `/theme bundled:light`
 *    (or any theme swap), the lite live region must repaint subsequent
 *    content using the NEW theme's brand color. Already-flushed scrollback
 *    rows are frozen at flush time and intentionally keep their old color
 *    (action-items doc: "text frozen at flush time, by design").
 *
 *    The action-items entry calls out 5+ hardcoded `chalk.hex('#C19AFF')`
 *    sites in the lite tree (LiteLayout welcome banner, LiteLiveRegion
 *    spinner/toolSpinner/agentTagFn fallback, ApprovalPrompt reasoning,
 *    lite/render.ts default colorFn). The fix is to route every site
 *    through the theme accessor (theme.brand).
 *
 * 2. WHAT class of regression would this catch?
 *    A future change that re-introduces a hardcoded brand color (or that
 *    drops the theme accessor from one of the live-region paint paths)
 *    would make this test fail: the live region's `Kiro:` agent tag would
 *    keep its old RGB value after the theme swap. Symmetrically, a refactor
 *    that "fixes" the freeze by re-rendering scrollback after theme swap
 *    would also fail — that's a bug, not a feature, because it would
 *    invalidate every line of paint already on the user's terminal.
 *
 * 3. Could the test pass even if the feature is broken?
 *    No. The assertion compares fgColor of `Kiro:` cells against the
 *    well-known RGB values from src/theme/kiroDark.ts (#C19AFF) and
 *    src/theme/kiroLight.ts (#8700FF). If theme.brand stops reaching
 *    the streaming agent tag, the live-region tag stays #C19AFF after the
 *    swap and the assertion fails. If the implementation accidentally
 *    re-paints already-flushed rows, the older row's fgColor changes and
 *    the freeze assertion fails.
 *
 * Anchor: docs/design/lite-tui-action-items.md → "Hardcoded brand color
 *          bypasses /theme".
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import {
  applyTheme,
  BRAND_DARK_RGB,
  BRAND_LIGHT_RGB,
  dominantRgb,
} from './lite/helpers/theme';

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

    await applyTheme(testCase, 'light');
    // showThemeMenu's "Theme set to ..." alert auto-hides in ~3s. Don't pin
    // the assertion to that ephemeral string — wait on something durable
    // instead. The footer's per-agent color repaints from kiroDark.brand
    // (#C19AFF) to kiroLight.brand (#8700FF) on swap; that paint stays put.
    await testCase.sleepMs(800);

    const liveContent = 'THEME_LIVE_AFTER_SWAP_MARKER';
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: liveContent } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('reflow probe');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText(liveContent, 15000);
    await testCase.waitForIdle(15000);

    // The default agent tag in lite renders as `kiro_default:` (the agent's
    // raw name verbatim, see renderAgentMessage in src/lite/render.ts).
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

    // Turn 1: streaming response under the dark (default) theme. Wait until
    // the stream completes so the agent message commits to <Static>.
    const turn1Content = 'THEME_FROZEN_ROW_DARK_TURN_ONE';
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: turn1Content } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('first turn');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText(turn1Content, 15000);
    await testCase.waitForIdle(15000);

    // Capture how many agent-tag rows existed before the theme swap. The
    // flushed row must remain at index 0 (top of scrollback) after turn 2.
    const beforeSwap = testCase.findAllTextCells('kiro_default:');
    expect(beforeSwap.length).toBeGreaterThan(0);
    const frozenFgBefore = dominantRgb(beforeSwap[0]!);
    expect(frozenFgBefore).toBe(BRAND_DARK_RGB);

    // Swap to light, then run a second turn so a new agent-tag row paints
    // under the new theme. See test 1 above for why we don't wait on the
    // "Theme set" alert string (auto-hides in ~3s).
    await applyTheme(testCase, 'light');
    await testCase.sleepMs(800);

    const turn2Content = 'THEME_FROZEN_ROW_LIGHT_TURN_TWO';
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: turn2Content } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
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
