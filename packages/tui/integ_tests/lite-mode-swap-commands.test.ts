import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  switchToLite,
  switchToTui,
  visibleCount,
  visibleIndex,
} from '../e2e_tests/lite/helpers/mode-swap';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * /lite and /tui mode-swap commands (bug-mine 2.9). setUiMode's contract:
 *  cross-mode swap bumps liteScrollbackClearToken, resets liteStaticSkipBefore
 *  to 0 in BOTH directions, and preserves messages[]; same-mode dispatch is a
 *  noop (must not bump the token). The per-test comments below pin two
 *  distinct rejected/refactor-prone designs.
 */

describe('lite mode swap commands [bug-mine 2.9]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  // Cross-mode swap, both directions: bumps liteScrollbackClearToken, resets
  // liteStaticSkipBefore to 0, and preserves messages[]. The two legs catch
  // distinct rejected refactors:
  //  - tui→lite (bug-mine 2.6): pinning skipBefore to messages.length would
  //    silently drop the user's scrollback; the contract resets it to 0.
  //  - lite→tui: conditioning the token bump on `uiMode === 'lite'` would skip
  //    it here, leaving stale lite singletons for the next remount.
  it.each([
    {
      label: 'tui→lite via /lite',
      start: 'tui' as const,
      target: 'lite' as const,
      switchMode: switchToLite,
      marker: 'TUI_PRE_SWAP_REPLY',
      contentId: 'tui-pre-swap-content',
      prompt: 'hello tui',
    },
    {
      label: 'lite→tui via /tui',
      start: 'lite' as const,
      target: 'tui' as const,
      switchMode: switchToTui,
      marker: 'LITE_PRE_SWAP_REPLY',
      contentId: 'lite-pre-swap-content',
      prompt: 'hello lite',
    },
  ])(
    '$label bumps liteScrollbackClearToken and preserves messages',
    async ({ start, target, switchMode, marker, contentId, prompt }) => {
      testCase =
        start === 'tui'
          ? await TestCase.builder()
              .withTestName('swap-tui-to-lite')
              .withGlobalSettings({ 'chat.ui.mode': 'tui' })
              .withEnv({ KIRO_LITE_ROLLOUT_ENABLED: '1' })
              .withTimeout(15000)
              .launch()
          : await launchLiteInteg('swap-lite-to-tui');
      if (start === 'tui') {
        await testCase.waitForVisibleText('ask a question', 10000);
      }

      await testCase.mockSessionUpdate({
        type: AgentEventType.Content,
        id: contentId,
        content: { type: 'text' as any, text: marker },
      });
      await testCase.typeAndSubmit(prompt);
      await testCase.completeTurn();
      await testCase.waitForStore((s) => !s.isProcessing, 10000);
      await testCase.sleepMs(400);

      const storeBefore = await testCase.getStore();
      expect(storeBefore.uiMode).toBe(start);
      expect(storeBefore.messages.length).toBeGreaterThan(0);
      const tokenBefore = storeBefore.liteScrollbackClearToken;

      await switchMode(testCase);

      const storeAfter = await testCase.getStore();
      expect(storeAfter.uiMode).toBe(target);
      expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);
      expect(storeAfter.liteStaticSkipBefore).toBe(0);
      const allMessageText = storeAfter.messages
        .map((m) => JSON.stringify(m))
        .join(' ');
      expect(allMessageText).toContain(marker);

      await exitLiteInteg(testCase);
    },
    30000
  );

  it('lite→tui preserves interleaved system rows before completed model text', async () => {
    testCase = await launchLiteInteg('swap-lite-to-tui-interleaved-system');

    await testCase.typeAndSubmit('hello lite');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'INTERLEAVED_SYSTEM_BEFORE_MODEL',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'lite-interleaved-model',
      content: {
        type: 'text' as any,
        text: 'LITE_RESPONSE_AFTER_INTERLEAVED_SYSTEM',
      },
    });
    await testCase.completeTurn();
    await testCase.waitForStore((s) => !s.isProcessing, 10000);
    await testCase.waitForVisibleText('LITE_RESPONSE_AFTER_INTERLEAVED_SYSTEM');

    const tokenBefore = (await testCase.getStore()).liteScrollbackClearToken;
    await switchToTui(testCase);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('tui');
    expect(storeAfter.liteScrollbackClearToken).toBeGreaterThan(tokenBefore);

    const snap = testCase.getSnapshot();
    const statusRow = '⟳ Goal: "INTERLEAVED_SYSTEM_BEFORE_MODEL"';
    const liteGoalSetRow = 'goal set · INTERLEAVED_SYSTEM_BEFORE_MODEL';
    expect(visibleCount(snap, statusRow)).toBe(1);
    expect(visibleCount(snap, liteGoalSetRow)).toBe(1);

    const goalIdx = visibleIndex(snap, statusRow);
    const liteGoalSetIdx = visibleIndex(snap, liteGoalSetRow);
    const responseIdx = visibleIndex(
      snap,
      'LITE_RESPONSE_AFTER_INTERLEAVED_SYSTEM'
    );
    const switchIdx = visibleIndex(snap, 'Switched to TUI mode');

    expect(goalIdx).toBeGreaterThanOrEqual(0);
    expect(liteGoalSetIdx).toBeGreaterThan(goalIdx);
    expect(responseIdx).toBeGreaterThan(liteGoalSetIdx);
    expect(switchIdx).toBeGreaterThanOrEqual(0);

    await exitLiteInteg(testCase);
  }, 30000);

  it('post-switch TUI turn keeps interleaved system rows owned by the turn', async () => {
    testCase = await launchLiteInteg('swap-lite-to-tui-post-switch-system');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'lite-pre-switch-model',
      content: { type: 'text' as any, text: 'LITE_BEFORE_POST_SWITCH_SYSTEM' },
    });
    await testCase.typeAndSubmit('hello lite');
    await testCase.completeTurn();
    await testCase.waitForStore((s) => !s.isProcessing, 10000);
    await testCase.waitForVisibleText('LITE_BEFORE_POST_SWITCH_SYSTEM');

    await switchToTui(testCase);
    await testCase.waitForVisibleText('Switched to TUI mode');

    await testCase.typeAndSubmit('post switch system');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'POST_SWITCH_INTERLEAVED_SYSTEM',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'post-switch-interleaved-model',
      content: {
        type: 'text' as any,
        text: 'POST_SWITCH_MODEL_AFTER_SYSTEM',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'cleared',
      iteration: 0,
      maxIterations: 3,
    });
    await testCase.waitForStore((s) => s.goalStatus === null, 10000);
    await testCase.completeTurn();
    await testCase.waitForVisibleText('POST_SWITCH_MODEL_AFTER_SYSTEM');

    await testCase.typeAndSubmit('flush post switch system turn');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'post-switch-flush-model',
      content: { type: 'text' as any, text: 'POST_SWITCH_SYSTEM_FLUSHED' },
    });
    await testCase.completeTurn();
    await testCase.waitForVisibleText('POST_SWITCH_SYSTEM_FLUSHED');

    const snap = testCase.getSnapshot();
    const statusRow = '⟳ Goal: "POST_SWITCH_INTERLEAVED_SYSTEM"';
    expect(visibleCount(snap, statusRow)).toBe(1);

    const switchIdx = visibleIndex(snap, 'Switched to TUI mode');
    const systemIdx = visibleIndex(snap, statusRow);
    const modelIdx = visibleIndex(snap, 'POST_SWITCH_MODEL_AFTER_SYSTEM');
    const flushIdx = visibleIndex(snap, 'POST_SWITCH_SYSTEM_FLUSHED');

    expect(switchIdx).toBeGreaterThanOrEqual(0);
    expect(systemIdx).toBeGreaterThan(switchIdx);
    expect(modelIdx).toBeGreaterThan(systemIdx);
    expect(flushIdx).toBeGreaterThan(modelIdx);

    await exitLiteInteg(testCase);
  }, 30000);

  it('lite status-only turn completes before the /tui switch announcement', async () => {
    testCase = await launchLiteInteg('swap-lite-status-only-to-tui');

    await testCase.typeAndSubmit('lite status only');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'PRE_SWITCH_STATUS_ONLY_SYSTEM',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'cleared',
      iteration: 0,
      maxIterations: 3,
    });
    await testCase.waitForStore((s) => s.goalStatus === null, 10000);
    await testCase.completeTurn();
    await testCase.waitForStore((s) => !s.isProcessing, 10000);
    await testCase.waitForVisibleText('PRE_SWITCH_STATUS_ONLY_SYSTEM');

    await switchToTui(testCase);
    await testCase.waitForVisibleText('Switched to TUI mode');

    const snap = testCase.getSnapshot();
    const statusRow = '⟳ Goal: "PRE_SWITCH_STATUS_ONLY_SYSTEM"';
    expect(visibleCount(snap, statusRow)).toBe(1);
    expect(visibleCount(snap, 'Switched to TUI mode')).toBe(1);

    const systemIdx = visibleIndex(snap, statusRow);
    const switchIdx = visibleIndex(snap, 'Switched to TUI mode');

    expect(systemIdx).toBeGreaterThanOrEqual(0);
    expect(switchIdx).toBeGreaterThanOrEqual(0);

    await exitLiteInteg(testCase);
  }, 30000);

  it('post-switch TUI status-only turn still renders as cancelled', async () => {
    testCase = await launchLiteInteg('swap-lite-to-tui-status-only-cancelled');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'lite-before-status-only',
      content: { type: 'text' as any, text: 'LITE_BEFORE_STATUS_ONLY' },
    });
    await testCase.typeAndSubmit('hello lite');
    await testCase.completeTurn();
    await testCase.waitForStore((s) => !s.isProcessing, 10000);

    await switchToTui(testCase);
    await testCase.waitForVisibleText('Switched to TUI mode');

    await testCase.typeAndSubmit('status only turn');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'POST_SWITCH_STATUS_ONLY_SYSTEM',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'cleared',
      iteration: 0,
      maxIterations: 3,
    });
    await testCase.waitForStore((s) => s.goalStatus === null, 10000);
    await testCase.completeTurn();
    await testCase.waitForVisibleText('POST_SWITCH_STATUS_ONLY_SYSTEM');

    await testCase.typeAndSubmit('flush status only turn');
    await testCase.waitForStore((s) => s.isProcessing, 10000);
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'post-status-only-flush-model',
      content: { type: 'text' as any, text: 'POST_STATUS_ONLY_FLUSHED' },
    });
    await testCase.completeTurn();
    await testCase.waitForVisibleText('POST_STATUS_ONLY_FLUSHED');

    const snap = testCase.getSnapshot();
    const statusRow = '⟳ Goal: "POST_SWITCH_STATUS_ONLY_SYSTEM"';
    expect(visibleCount(snap, statusRow)).toBe(1);
    expect(visibleCount(snap, 'Cancelled')).toBe(1);

    const switchIdx = visibleIndex(snap, 'Switched to TUI mode');
    const systemIdx = visibleIndex(snap, statusRow);
    const cancelledIdx = visibleIndex(snap, 'Cancelled');
    const flushIdx = visibleIndex(snap, 'POST_STATUS_ONLY_FLUSHED');

    expect(switchIdx).toBeGreaterThanOrEqual(0);
    expect(systemIdx).toBeGreaterThanOrEqual(0);
    expect(systemIdx).toBeLessThan(cancelledIdx);
    expect(cancelledIdx).toBeGreaterThan(switchIdx);
    expect(flushIdx).toBeGreaterThan(cancelledIdx);
    expect(systemIdx).toBeGreaterThan(switchIdx);

    await exitLiteInteg(testCase);
  }, 30000);

  it('same-mode dispatch is a noop (bug 2.9)', async () => {
    testCase = await launchLiteInteg('swap-noop-same-mode');

    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('lite');
    const tokenBefore = storeBefore.liteScrollbackClearToken;

    // /lite while already in lite must NOT bump the clear token (bug 2.9).
    await switchToLite(testCase);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    expect(storeAfter.liteScrollbackClearToken).toBe(tokenBefore);

    await exitLiteInteg(testCase);
  }, 30000);
});
