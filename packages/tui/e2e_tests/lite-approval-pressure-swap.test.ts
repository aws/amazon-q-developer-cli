/**
 * Mode swap with a pending (undismissed) approval. Bug-mine 2.1 (cursor
 * realignment), 3.5 (approval at mode boundary). Discovered contracts:
 *
 * LITE -> TUI: the ApprovalPrompt replaces PromptInput and captures ALL
 *   keystrokes (y/n/t/Esc/Ctrl+C hotkeys), so typing `/tui` is swallowed — the
 *   `t` opens the trust submenu / auto-trusts. The swap command never reaches
 *   the input; the user must dismiss the approval first.
 *
 * TUI -> LITE: the approval Menu and PromptBar are both visible, and Enter
 *   fires BOTH Menu.onSelect (auto-resolves approval) AND PromptBar.onSubmit.
 *   isProcessing is still true while the tool runs, so the slash command is
 *   rejected until the turn completes.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_LITE, CMD_TUI, typeSlashCommand } from './lite/helpers/commands';
import { streamReply } from './lite/helpers/responses';

describe('lite approval pressure swap [bug-mine 2.1, 3.5]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite->tui: approval captures keystrokes, "t" enters trust submenu, mode swap blocked', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('approval-pressure-lite-to-tui')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.getSessionId();

    // Stream 1: a write ToolUseEvent that requires approval.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'pressure-swap-lite-write',
            name: 'write',
            input: JSON.stringify({
              command: 'create',
              path: '/tmp/pressure-swap-lite.txt',
              content: 'pressure swap test',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: continuation after the tool is approved.
    await streamReply(testCase, 'File created successfully.');

    await testCase.sendKeys('write a test file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

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
    testCase = await E2ETestCase.builder()
      .withTestName('approval-pressure-tui-to-lite')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    // Stream 1: a write ToolUseEvent that requires approval.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'pressure-swap-tui-write',
            name: 'write',
            input: JSON.stringify({
              command: 'create',
              path: '/tmp/pressure-swap-tui.txt',
              content: 'pressure swap test',
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Stream 2: continuation after the tool is approved.
    await streamReply(testCase, 'File written successfully.');

    await testCase.sendKeys('write a test file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

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
