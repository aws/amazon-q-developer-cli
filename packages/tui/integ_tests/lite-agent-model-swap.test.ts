import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ApprovalOptionId } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';

/**
 * Bug-mine 6.4: Subagent footer "needs approval" attribution at the integ
 * layer. The remaining 6.x cases (pendingSwap latch lifecycle) are pure
 * hook-state and live in the vitest unit suite for usePendingSwap — a
 * Zustand integ test cannot observe React-local useState.
 */
describe('lite agent/model swap [bug-mine 6.4]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // bug-mine 6.1 / 6.2 / 6.3 (pendingSwap latch, 30s safety timeout,
  // live-region "queued" state) are covered by the unit test in
  // src/components/layout/lite/__tests__/usePendingSwap.test.tsx — the
  // hook owns its state in React-local useState, so a Zustand-store-level
  // integ test cannot observe it.

  it('subagent footer row shows permission-blocked state when approval pending [bug-mine 6.4]', async () => {
    // Bug 6.4: When a subagent requests approval, the footer row should
    // show permission-blocked state ("requesting-permission" phase) instead
    // of appearing as though the tool is still running normally.
    //
    // Strategy: Inject a subagent tool call followed by an approval request.
    // Verify the store has pendingApproval and the approval's toolCallId
    // matches the subagent's tool. The LiteLayout memo sets row.phase to
    // 'requesting-permission' when the approval toolCallId matches, which
    // renders the "needs approval" indicator.

    testCase = await TestCase.builder()
      .withTestName('lite-agent-swap-approval-6.4')
      .withLite()
      .withEnv({ KIRO_MOCK_AGENT_NAME: 'main-agent' })
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const SUBAGENT_SESSION = 'subagent-session-swap';
    const SUBAGENT_TOOL_ID = 'sub-tool-shell-swap-1';

    // Inject parent subagent tool (unfinished — gates activeSubagents).
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'parent-subagent-swap',
      name: 'subagent',
      args: { agent: SUBAGENT_SESSION },
    });

    // Inject a tool call FROM the subagent (sessionId stamps agentName).
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: SUBAGENT_TOOL_ID,
      name: 'Shell',
      kind: 'shell' as any,
      args: { command: 'rm -rf /tmp/test' },
      sessionId: SUBAGENT_SESSION,
    });

    // Inject approval request for the subagent's tool.
    await testCase.mockSessionUpdate({
      type: AgentEventType.ApprovalRequest,
      value: {
        sessionId: SUBAGENT_SESSION,
        toolCall: {
          toolCallId: SUBAGENT_TOOL_ID,
          title: 'Execute shell command',
          rawInput: {},
        },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow Once',
            optionId: 'allow_once',
          },
          {
            kind: ApprovalOptionId.RejectOnce,
            name: 'Reject Once',
            optionId: 'reject_once',
          },
        ],
        resolve: (() => {}) as any,
      },
    } as any);

    // Submit to trigger event drain.
    await testCase.typeAndSubmit('a1');
    await testCase.sleepMs(400);

    // Verify approval is pending in the store.
    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe(SUBAGENT_TOOL_ID);
    expect(store.pendingApproval!.sessionId).toBe(SUBAGENT_SESSION);

    // Verify the subagent tool message exists with proper agentName.
    const subagentTool = store.messages.find(
      (m) =>
        m.role === MessageRole.ToolUse &&
        m.id === SUBAGENT_TOOL_ID
    );
    expect(subagentTool).toBeDefined();
    expect((subagentTool as any).agentName).toBe(SUBAGENT_SESSION);

    // The LiteLayout memo uses approvalToolCallId === m.id to set
    // row.phase = 'requesting-permission'. Since pendingApproval.toolCallId
    // matches the subagent tool's id, the row is correctly marked as blocked.
    // Verify this indirectly: the approval toolCallId must match a subagent
    // tool (not the parent subagent tool or the main agent's tools).
    const parentTool = store.messages.find(
      (m) => m.role === MessageRole.ToolUse && m.name === 'subagent'
    );
    expect(parentTool).toBeDefined();
    expect(store.pendingApproval!.toolCall.toolCallId).not.toBe(parentTool!.id);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 40000);
});
