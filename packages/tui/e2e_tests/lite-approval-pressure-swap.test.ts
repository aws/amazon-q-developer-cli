/**
 * Mode swap with a pending (undismissed) approval [bug-mine 2.1, 3.5].
 * Discovered contracts:
 * - LITE->TUI: ApprovalPrompt captures ALL keystrokes (y/n/t/Esc/Ctrl+C), so
 *   `/tui` is swallowed (`t` enters the trust submenu) — dismiss approval first.
 * - TUI->LITE: approval Menu + PromptBar are both live, so Enter fires BOTH
 *   Menu.onSelect (resolves approval) AND PromptBar.onSubmit (/lite, rejected
 *   while isProcessing).
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

describe('lite approval pressure swap [bug-mine 2.1, 3.5]', () => {
  let testCase: E2ETestCase | null = null;
  trackCleanup(() => testCase);

  it('lite->tui: approval captures keystrokes, "t" enters trust submenu, mode swap blocked', async () => {
    testCase = await launchLiteE2E('approval-pressure-lite-to-tui', {
      terminal: { width: 120, height: 40 },
      waitForCommands: false,
    });

    await pushWriteApprovalEvent(testCase, {
      toolUseId: 'pressure-swap-lite-write',
      path: '/tmp/pressure-swap-lite.txt',
      content: 'pressure swap test',
    });

    await streamReply(testCase, 'File created successfully.');

    await sendUserMessage(testCase, 'write a test file');

    await testCase.waitForText('needs approval', 15000);

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

    const afterT = await testCase.getStore();
    expect(afterT.uiMode).toBe('lite');

    // Esc twice: back out of the trust submenu (if entered), then cancel the
    // turn — reaching a clean idle state either way.
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

  it('tui->lite: Enter resolves approval via Menu then /lite works after idle', async () => {
    testCase = await launchTuiE2E('approval-pressure-tui-to-lite', {
      waitForCommands: false,
    });

    await pushWriteApprovalEvent(testCase, {
      toolUseId: 'pressure-swap-tui-write',
      path: '/tmp/pressure-swap-tui.txt',
      content: 'pressure swap test',
    });

    await streamReply(testCase, 'File written successfully.');

    await sendUserMessage(testCase, 'write a test file');

    await testCase.waitForText('requires approval', 15000);

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
