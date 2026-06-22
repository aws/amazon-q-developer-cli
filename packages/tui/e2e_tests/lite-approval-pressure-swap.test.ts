/**
 * E2E test: Mode swap with pending (undismissed) approval.
 *
 * Validates bug-mine entries:
 *   2.1 — Mode-swap cursor realignment via useLayoutEffect
 *   3.5 — Approval interaction during mode boundary
 *
 * Discovered contracts:
 *
 *   LITE -> TUI (with active approval):
 *     The lite ApprovalPrompt replaces the PromptInput entirely while
 *     showApproval is true. The ApprovalPrompt captures ALL keystrokes
 *     via useKeypress, with y/n/t/Esc/Ctrl+C as active hotkeys. Attempting
 *     to type `/tui` causes the `t` character to open the trust scope
 *     submenu (if trust tiers are available for the tool) or auto-trust
 *     the tool (if no tiers). Either way, input is captured by the approval
 *     UI — the mode-swap command never reaches the input box. The user must
 *     first dismiss the approval (Esc to cancel the turn, or y/n to
 *     respond) before typing /tui.
 *
 *   TUI -> LITE (with active approval):
 *     The TUI renders an ApprovalRequest Panel with a Menu (dropdown mode).
 *     The PromptBar input is simultaneously visible. However, pressing
 *     Enter fires BOTH the Menu's onSelect (resolving approval with the
 *     highlighted option) AND the PromptBar's onSubmit. The net effect is:
 *     the approval gets auto-resolved by the Menu, and the PromptBar
 *     submits the typed text to handleUserInput. Since isProcessing is
 *     still true (tool is executing after approval), handleUserInput rejects
 *     the slash command. After the tool finishes and the turn completes,
 *     the user can then successfully issue /lite.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { CMD_LITE, CMD_TUI, typeSlashCommand } from './lite/helpers/commands';

describe('lite approval pressure swap [bug-mine 2.1, 3.5]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('lite->tui: approval captures keystrokes, "t" enters trust submenu, mode swap blocked', async () => {
    // Start in lite mode
    testCase = await E2ETestCase.builder()
      .withTestName('approval-pressure-lite-to-tui')
      .withTerminal({ width: 120, height: 40 })
      .withLite()
      .launch();

    await testCase.waitForText('>', 15000);
    await testCase.getSessionId();

    // Stream 1: ToolUseEvent that requires approval
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

    // Stream 2: Continuation after tool approval (assistant response)
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'File created successfully.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send a message to trigger the response stream
    await testCase.sendKeys('write a test file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the approval prompt to appear
    await testCase.waitForText('needs approval', 15000);

    // Confirm approval is pending
    const duringApproval = await testCase.waitForStoreCondition(
      (s) => s.pendingApproval != null,
      5000,
    );
    expect(duringApproval.pendingApproval).not.toBeNull();
    expect(duringApproval.isProcessing).toBe(true);
    expect(duringApproval.uiMode).toBe('lite');

    // Type `/` — not a hotkey, dropped by ApprovalPrompt handler
    await testCase.sendKeys('/');
    await testCase.sleepMs(100);

    // Approval is still active after `/`
    const afterSlash = await testCase.getStore();
    expect(afterSlash.pendingApproval).not.toBeNull();
    expect(afterSlash.uiMode).toBe('lite');

    // Type `t` — this IS a hotkey. In the default approval page, `t`
    // enters the trust submenu (if trust tiers exist) or directly trusts
    // the tool (if no tiers).
    await testCase.sendKeys('t');
    await testCase.sleepMs(300);

    // Check if the approval entered trust submenu (tiers available) or
    // resolved directly (no tiers). Either way, mode is still 'lite'.
    const afterT = await testCase.getStore();
    expect(afterT.uiMode).toBe('lite');

    // Whether the approval resolved or entered submenu, the mode-swap
    // command `/tui` was never processed — the `t` was captured by
    // the approval UI, proving mode-swap is blocked during active approval.

    // Cancel the entire turn with Esc to get to a clean state.
    // If in trust submenu: Esc goes back to default page.
    // If approval resolved: Esc cancels the current stream.
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    // If still in default approval page after first Esc, press again to cancel turn.
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    // Wait for everything to settle (either cancelled or completed)
    await testCase.waitForStoreCondition(
      (s) => !s.isProcessing,
      15000,
    );

    const afterCancel = await testCase.getStore();
    expect(afterCancel.uiMode).toBe('lite');
    expect(afterCancel.pendingApproval).toBeNull();
    expect(afterCancel.isProcessing).toBe(false);

    // Now verify a proper /tui swap works from a clean state
    await testCase.waitForSlashCommands();
    await testCase.sendKeys(CMD_TUI);
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition(
      (s) => s.uiMode === 'tui',
      10000,
    );

    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('tui');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);
  }, 60000);

  it('tui->lite: Enter resolves approval via Menu then /lite works after idle', async () => {
    // Start in TUI mode
    testCase = await E2ETestCase.builder()
      .withTestName('approval-pressure-tui-to-lite')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    // Stream 1: ToolUseEvent that requires approval
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

    // Stream 2: Continuation after tool approval (assistant response)
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'File written successfully.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send a message to trigger the response stream
    await testCase.sendKeys('write a test file');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the approval prompt to appear
    await testCase.waitForText('requires approval', 15000);

    // Confirm approval is pending
    const duringApproval = await testCase.waitForStoreCondition(
      (s) => s.pendingApproval != null,
      5000,
    );
    expect(duringApproval.pendingApproval).not.toBeNull();
    expect(duringApproval.isProcessing).toBe(true);
    expect(duringApproval.uiMode).toBe('tui');

    // In TUI mode, type /lite into the PromptBar (Menu only captures
    // arrows/Enter/Esc, not printable chars). No trailing space: Enter must
    // simultaneously fire Menu.onSelect (Allow Once) AND PromptBar.onSubmit
    // (/lite). The approval resolves; handleUserInput('/lite') is rejected
    // because isProcessing is still true.
    await typeSlashCommand(testCase, CMD_LITE);

    // Wait for the tool to complete and turn to finish
    await testCase.waitForIdle(15000);

    // After idle, we should be in TUI mode still (/lite was rejected during
    // processing because TUI mode can't queue slash commands)
    const afterIdle = await testCase.getStore();
    expect(afterIdle.uiMode).toBe('tui');
    expect(afterIdle.pendingApproval).toBeNull();
    expect(afterIdle.isProcessing).toBe(false);

    // Now that everything is idle, /lite should work
    await testCase.waitForSlashCommands();
    await typeSlashCommand(testCase, CMD_LITE);

    await testCase.waitForStoreCondition(
      (s) => s.uiMode === 'lite',
      10000,
    );

    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('lite');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);

    // History preserved across the swap
    const hasUserMsg = afterSwap.messages.some(
      (m) => JSON.stringify(m).includes('write a test file')
    );
    expect(hasUserMsg).toBe(true);
  }, 60000);
});
