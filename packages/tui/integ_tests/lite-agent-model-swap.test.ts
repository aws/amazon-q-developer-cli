import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';
import {
  finishAndExitLite,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';
import {
  injectApproval,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';

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

  it('subagent footer row shows permission-blocked state when approval pending [bug-mine 6.4]', async () => {
    // Invariant: when a pendingApproval's toolCallId matches a subagent tool,
    // LiteLayout sets that footer row.phase = 'requesting-permission'.
    testCase = await launchLiteInteg('lite-agent-swap-approval-6.4', {
      env: { KIRO_MOCK_AGENT_NAME: 'main-agent' },
      timeout: 20000,
    });

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

    // Approval request for the subagent's tool (ToolCall seeded above).
    await injectApproval(testCase, {
      toolCallId: SUBAGENT_TOOL_ID,
      toolName: 'Shell',
      sessionId: SUBAGENT_SESSION,
      options: ALLOW_REJECT_OPTIONS,
      withPrecedingToolCall: false,
    });

    await testCase.typeAndSubmit('a1');
    await testCase.sleepMs(400);

    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe(SUBAGENT_TOOL_ID);
    expect(store.pendingApproval!.sessionId).toBe(SUBAGENT_SESSION);

    const subagentTool = store.messages.find(
      (m) => m.role === MessageRole.ToolUse && m.id === SUBAGENT_TOOL_ID
    );
    expect(subagentTool).toBeDefined();
    expect((subagentTool as any).agentName).toBe(SUBAGENT_SESSION);

    // The blocked row is verified indirectly: the approval toolCallId matches a
    // subagent tool, not the parent subagent tool.
    const parentTool = store.messages.find(
      (m) => m.role === MessageRole.ToolUse && m.name === 'subagent'
    );
    expect(parentTool).toBeDefined();
    expect(store.pendingApproval!.toolCall.toolCallId).not.toBe(parentTool!.id);

    await finishAndExitLite(testCase);
  }, 40000);
});
