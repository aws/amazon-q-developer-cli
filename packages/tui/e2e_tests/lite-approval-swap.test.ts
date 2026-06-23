/**
 * Mode-swap around an active/just-resolved write-approval [bug-mine 2.1, 3.5].
 * Two regression families share one launch+approval preamble:
 *   - Esc-cancel: Esc in an approval cancels the ENTIRE turn (3.5), then a swap
 *     must find clean state (no stale pendingApproval/isProcessing). TUI->lite
 *     also asserts the user message lands in lite scrollback (2.1 cursor
 *     realignment via useLayoutEffect).
 *   - Pressure: with the approval still pending, ApprovalPrompt captures all
 *     keystrokes so the swap is blocked until idle.
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
  sendUserMessage,
} from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';
import { pushWriteApprovalEvent } from './lite/helpers/approvals';

type Launch = typeof launchLiteE2E;

/**
 * Launch in `start` mode, push a write-approval, send the user prompt, and wait
 * for the approval to paint. `reply` (optional) pre-stages the tool's success
 * stream (pressure cases need it; the Esc-cancel cases never let the tool run).
 */
async function setupApprovalTurn(opts: {
  launch: Launch;
  testName: string;
  toolUseId: string;
  path: string;
  prompt: string;
  approvalText: string;
  reply?: string;
}): Promise<E2ETestCase> {
  const tc = await opts.launch(opts.testName, {
    terminal: { width: 120, height: 40 },
    waitForCommands: false,
  });
  await pushWriteApprovalEvent(tc, {
    toolUseId: opts.toolUseId,
    path: opts.path,
    content: 'test content',
  });
  if (opts.reply) await streamReply(tc, opts.reply);
  await sendUserMessage(tc, opts.prompt);
  await tc.waitForText(opts.approvalText, 15000);
  return tc;
}

describe('lite approval + mode swap [bug-mine 2.1, 3.5]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it.each([
    {
      label: 'lite -> tui',
      launch: launchLiteE2E,
      testName: 'swap-approval-lite-esc-then-tui',
      toolUseId: 'write-needs-approval-lite',
      path: '/tmp/approval-swap-lite.txt',
      approvalText: 'needs approval',
      swapCmd: CMD_TUI,
      target: 'tui' as const,
      postSwapVisible: undefined as string | undefined,
    },
    {
      label: 'tui -> lite',
      launch: launchTuiE2E,
      testName: 'swap-approval-tui-esc-then-lite',
      toolUseId: 'write-needs-approval-tui',
      path: '/tmp/approval-swap-tui.txt',
      approvalText: 'requires approval',
      swapCmd: CMD_LITE,
      target: 'lite' as const,
      postSwapVisible: 'write a file',
    },
  ])(
    'Esc in $label approval cancels the turn, then swap finds no stale state',
    async ({
      launch,
      testName,
      toolUseId,
      path,
      approvalText,
      swapCmd,
      target,
      postSwapVisible,
    }) => {
      testCase = await setupApprovalTurn({
        launch,
        testName,
        toolUseId,
        path,
        prompt: 'write a file',
        approvalText,
      });

      await testCase.pressEscape();

      const afterEsc = await testCase.waitForStoreCondition(
        (s) => !s.isProcessing,
        10000
      );
      expect(afterEsc.pendingApproval).toBeNull();
      expect(afterEsc.isProcessing).toBe(false);

      await testCase.waitForSlashCommands();
      await testCase.sendKeys(swapCmd);
      await testCase.sleepMs(100);
      await testCase.pressEnter();

      await testCase.waitForStoreCondition((s) => s.uiMode === target, 10000);

      if (postSwapVisible) await testCase.waitForText(postSwapVisible, 10000);

      const afterSwap = await testCase.getStore();
      expect(afterSwap.uiMode).toBe(target);
      expect(afterSwap.pendingApproval).toBeNull();
      expect(afterSwap.isProcessing).toBe(false);
    },
    45000
  );

  it('pressure lite->tui: approval captures keystrokes, "t" enters trust submenu, mode swap blocked', async () => {
    testCase = await setupApprovalTurn({
      launch: launchLiteE2E,
      testName: 'approval-pressure-lite-to-tui',
      toolUseId: 'pressure-swap-lite-write',
      path: '/tmp/pressure-swap-lite.txt',
      prompt: 'write a test file',
      approvalText: 'needs approval',
      reply: 'File created successfully.',
    });

    const duringApproval = await testCase.waitForStoreCondition(
      (s) => s.pendingApproval != null,
      5000
    );
    expect(duringApproval.pendingApproval).not.toBeNull();
    expect(duringApproval.isProcessing).toBe(true);
    expect(duringApproval.uiMode).toBe('lite');

    // `/` is not a hotkey, so ApprovalPrompt drops it and stays active.
    await testCase.sendKeys('/');
    await testCase.sleepMs(100);
    const afterSlash = await testCase.getStore();
    expect(afterSlash.pendingApproval).not.toBeNull();
    expect(afterSlash.uiMode).toBe('lite');

    // `t` IS a hotkey: it enters the trust submenu (if tiers exist) or trusts
    // directly. Either way the `/tui` swap never reaches input — proving the
    // swap is blocked during an active approval (mode stays 'lite').
    await testCase.sendKeys('t');
    await testCase.sleepMs(300);
    expect((await testCase.getStore()).uiMode).toBe('lite');

    // Esc twice: back out of the trust submenu (if entered), then cancel the turn.
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    await testCase.waitForStoreCondition((s) => !s.isProcessing, 15000);
    const afterCancel = await testCase.getStore();
    expect(afterCancel.uiMode).toBe('lite');
    expect(afterCancel.pendingApproval).toBeNull();
    expect(afterCancel.isProcessing).toBe(false);

    // A proper /tui swap works once idle.
    await testCase.waitForSlashCommands();
    await testCase.sendKeys(CMD_TUI);
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition((s) => s.uiMode === 'tui', 10000);
    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('tui');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);
  }, 60000);

  it('pressure tui->lite: Enter resolves approval via Menu then /lite works after idle', async () => {
    testCase = await setupApprovalTurn({
      launch: launchTuiE2E,
      testName: 'approval-pressure-tui-to-lite',
      toolUseId: 'pressure-swap-tui-write',
      path: '/tmp/pressure-swap-tui.txt',
      prompt: 'write a test file',
      approvalText: 'requires approval',
      reply: 'File written successfully.',
    });

    const duringApproval = await testCase.waitForStoreCondition(
      (s) => s.pendingApproval != null,
      5000
    );
    expect(duringApproval.pendingApproval).not.toBeNull();
    expect(duringApproval.isProcessing).toBe(true);
    expect(duringApproval.uiMode).toBe('tui');

    // No trailing space: Enter fires BOTH Menu.onSelect (Allow Once) AND
    // PromptBar.onSubmit (/lite). The approval resolves, but /lite is rejected
    // because isProcessing is still true while the tool runs.
    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForIdle(15000);

    // Still TUI: /lite was rejected mid-processing (TUI can't queue commands).
    const afterIdle = await testCase.getStore();
    expect(afterIdle.uiMode).toBe('tui');
    expect(afterIdle.pendingApproval).toBeNull();
    expect(afterIdle.isProcessing).toBe(false);

    // /lite works once idle.
    await testCase.waitForSlashCommands();
    await typeSlashCommand(testCase, CMD_LITE);
    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);

    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('lite');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);

    // History preserved across the swap.
    const hasUserMsg = afterSwap.messages.some((m) =>
      JSON.stringify(m).includes('write a test file')
    );
    expect(hasUserMsg).toBe(true);
  }, 60000);
});
