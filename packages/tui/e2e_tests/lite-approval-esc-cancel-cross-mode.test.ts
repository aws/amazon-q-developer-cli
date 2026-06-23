/**
 * E2E test: Mode-swap around approval lifecycle must not leave stale state.
 *
 * Validates bug-mine entries:
 *   2.1 — Mode-swap cursor realignment via useLayoutEffect
 *         (Chat in TUI mode advances cursor, switch to /lite: first lite
 *          message must actually appear in scrollback.)
 *   3.5 — Esc in approval cancels the agent turn (not just the approval)
 *         (cancelMessage() calls cancelApproval() internally, so sibling
 *          approvals in the batch are also dropped.)
 *
 * Key invariants:
 *   - Esc during an approval cancels the entire turn, not just the prompt.
 *   - Mode swap after approval resolution carries correct message history.
 *   - No stale pendingApproval/isProcessing leaks across mode boundaries.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import {
  CMD_LITE,
  CMD_TUI,
  launchLiteE2E,
  launchTuiE2E,
  sendUserMessage,
} from './lite/helpers/commands';
import { pushWriteApprovalEvent } from './lite/helpers/approvals';

describe('lite approval Esc-cancel then cross-mode swap [bug-mine 2.1, 3.5]', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // Esc in an approval cancels the ENTIRE turn (bug-mine 3.5), not just the
  // prompt; after that, swapping to the other mode must find a clean state
  // (no stale pendingApproval / isProcessing). The TUI->lite direction also
  // asserts the original user message lands in lite scrollback after the swap
  // (bug-mine 2.1 cursor realignment via useLayoutEffect).
  const cases = [
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
  ];

  it.each(cases)(
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
      testCase = await launch(testName, {
        terminal: { width: 120, height: 40 },
        waitForCommands: false,
      });

      await pushWriteApprovalEvent(testCase, {
        toolUseId,
        path,
        content: 'test content',
      });

      await sendUserMessage(testCase, 'write a file');
      await testCase.waitForText(approvalText, 15000);

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

      if (postSwapVisible) {
        await testCase.waitForText(postSwapVisible, 10000);
      }

      const afterSwap = await testCase.getStore();
      expect(afterSwap.uiMode).toBe(target);
      expect(afterSwap.pendingApproval).toBeNull();
      expect(afterSwap.isProcessing).toBe(false);
    },
    45000
  );
});
