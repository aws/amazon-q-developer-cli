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

  it('Esc in lite approval cancels the entire turn, then /tui has no stale state [bug-mine 3.5]', async () => {
    // In lite mode, the approval prompt captures all keystrokes. The only
    // way out is Esc, which cancels the ENTIRE turn (not just the approval).
    // After cancellation, /tui should find a clean state.
    testCase = await launchLiteE2E('swap-approval-lite-esc-then-tui', {
      terminal: { width: 120, height: 40 },
      waitForCommands: false,
    });

    await pushWriteApprovalEvent(testCase, {
      toolUseId: 'write-needs-approval-lite',
      path: '/tmp/approval-swap-lite.txt',
      content: 'test content',
    });

    // Send a message to trigger the response stream
    await sendUserMessage(testCase, 'write a file');

    // Wait for the approval prompt to appear
    await testCase.waitForText('needs approval', 15000);

    // Press Esc — should cancel the entire turn (bug-mine 3.5),
    // not just dismiss this one approval
    await testCase.pressEscape();

    // Verify the turn is fully cancelled: isProcessing becomes false
    // AND pendingApproval is cleared
    const afterEsc = await testCase.waitForStoreCondition(
      (s) => !s.isProcessing,
      10000
    );
    expect(afterEsc.pendingApproval).toBeNull();
    expect(afterEsc.isProcessing).toBe(false);

    // Now swap to TUI mode — should work cleanly with no stale approval
    await testCase.waitForSlashCommands();
    await testCase.sendKeys(CMD_TUI);
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition((s) => s.uiMode === 'tui', 10000);

    // Verify no stale approval state leaked into TUI mode
    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('tui');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);
  }, 45000);

  it('Esc in TUI approval cancels turn, then /lite has no stale state [bug-mine 2.1, 3.5]', async () => {
    // In TUI mode, Esc from the approval dropdown cancels the entire turn
    // (same as lite — bug-mine 3.5). Then swapping to /lite must not carry
    // stale pendingApproval or isProcessing state, and the user's original
    // message must render in lite scrollback (bug-mine 2.1 cursor realignment).
    testCase = await E2ETestCase.builder()
      .withTestName('swap-approval-tui-esc-then-lite')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    await pushWriteApprovalEvent(testCase, {
      toolUseId: 'write-needs-approval-tui',
      path: '/tmp/approval-swap-tui.txt',
      content: 'approval test content',
    });

    // Send a message to trigger the response stream
    await sendUserMessage(testCase, 'write a file');

    // Wait for the approval prompt to appear
    await testCase.waitForText('requires approval', 15000);

    // Press Esc — cancels the entire turn (not just the approval)
    await testCase.pressEscape();

    // Verify the turn is fully cancelled
    const afterEsc = await testCase.waitForStoreCondition(
      (s) => !s.isProcessing,
      10000
    );
    expect(afterEsc.pendingApproval).toBeNull();
    expect(afterEsc.isProcessing).toBe(false);

    // Now swap to lite mode
    await testCase.waitForSlashCommands();
    await testCase.sendKeys(CMD_LITE);
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForStoreCondition((s) => s.uiMode === 'lite', 10000);

    // Bug-mine 2.1: cursor realignment via useLayoutEffect must ensure
    // the user's original message appears in lite scrollback after swap.
    await testCase.waitForText('write a file', 10000);

    const afterSwap = await testCase.getStore();
    expect(afterSwap.uiMode).toBe('lite');
    expect(afterSwap.pendingApproval).toBeNull();
    expect(afterSwap.isProcessing).toBe(false);
  }, 45000);
});
