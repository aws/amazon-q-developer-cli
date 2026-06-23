import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  injectApproval,
  expectApprovalVisible,
  expectApprovalDeferred,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';
import {
  finishAndExitLite,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/** Bug-mine 3.1-3.6: lite approval flow behavior (per-case rationale inline). */
describe('lite approval flow [bug-mine 3.1, 3.2, 3.3, 3.4, 3.6]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  it.each([
    {
      name: '[bug-mine 3.1] typing guard defers approval prompt via APPROVAL_IDLE_MS debounce',
      testName: 'lite-approval-typing-guard',
      // A keypress within APPROVAL_IDLE_MS (2000ms) defers the prompt.
      preSubmit: async (tc: TestCase) => {
        await tc.sendKeys('hello');
        await tc.sleepMs(50);
        await injectApproval(tc, {
          toolCallId: 'tool-guard-1',
          toolName: 'Shell',
        });
      },
      body: async (tc: TestCase) => {
        const store = await tc.getStore();
        expect(store.pendingApproval!.toolCall.toolCallId).toBe('tool-guard-1');
        // Deferred: event delivered to store but prompt not yet painted.
        expectApprovalDeferred(tc);
        // Past the 2000ms idle threshold the prompt appears.
        await tc.sleepMs(2200);
        expectApprovalVisible(tc);
      },
    },
    {
      name: '[bug-mine 3.2] approval prompt stays visible once shown despite further keystrokes',
      testName: 'lite-approval-stays-visible',
      preSubmit: async (tc: TestCase) => {
        // No recent typing -> approval shows immediately.
        await injectApproval(tc, {
          toolCallId: 'tool-visible-1',
          toolName: 'Shell',
        });
      },
      body: async (tc: TestCase) => {
        await tc.sleepMs(2500);
        expectApprovalVisible(tc);
        // A non-y/n/t key must NOT hide the prompt: the keypress handler skips
        // the lastKeypressRef update while an approval is shown.
        await tc.sendKeys('x');
        await tc.sleepMs(300);
        expectApprovalVisible(tc);
        const store = await tc.getStore();
        expect(store.pendingApproval!.toolCall.toolCallId).toBe(
          'tool-visible-1'
        );
      },
    },
    {
      name: '[bug-mine 3.3] sequential approvals: y keystroke is not counted as typing so next approval shows without delay',
      testName: 'lite-approval-sequential',
      // Queue BOTH before submitting: first -> pendingApproval, second -> queue.
      preSubmit: async (tc: TestCase) => {
        await injectApproval(tc, {
          toolCallId: 'tool-seq-1',
          toolName: 'Shell',
        });
        await injectApproval(tc, {
          toolCallId: 'tool-seq-2',
          toolName: 'Write',
        });
      },
      body: async (tc: TestCase) => {
        const store1 = await tc.getStore();
        expect(store1.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-1');
        expect(store1.approvalQueue.length).toBeGreaterThanOrEqual(2);
        await tc.sleepMs(2200);
        expectApprovalVisible(tc);
        // 'y' must NOT count as "user typing" (the useKeypress guard skips the
        // lastKeypressRef update while an approval is shown), so the promoted
        // second approval shows with no fresh 2s idle delay.
        await tc.sendKeys('y');
        await tc.sleepMs(500);
        const store2 = await tc.getStore();
        expect(store2.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-2');
        expectApprovalVisible(tc);
      },
    },
  ])(
    '$name',
    async ({ testName, preSubmit, body }) => {
      testCase = await launchLiteInteg(testName, { timeout: 25000 });
      await preSubmit(testCase);
      await testCase.typeAndSubmit('go');
      await testCase.sleepMs(300);
      expect((await testCase.getStore()).pendingApproval).not.toBeNull();
      await body(testCase);
      await finishAndExitLite(testCase);
    },
    45000
  );

  it('[bug-mine 3.4] trust submenu resets when approval changes', async () => {
    testCase = await launchLiteInteg('lite-approval-trust-reset', {
      timeout: 20000,
    });

    // First approval carries trust options so 't' opens the submenu.
    await injectApproval(testCase, {
      toolCallId: 'tool-trust-a',
      toolName: 'Shell',
      rawInput: { command: 'echo a' },
      trustOptions: [
        {
          label: 'Trust echo commands',
          display: 'echo *',
          setting_key: 'shell.echo',
          patterns: ['echo *'],
        },
      ],
    });

    await testCase.typeAndSubmit('go');
    await testCase.sleepMs(2500);

    expectApprovalVisible(testCase);

    await testCase.sendKeys('t');
    await testCase.sleepMs(300);
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('trust scope');

    // A second approval replacing the first must reset the submenu page to
    // 'default' (the component's useEffect on approvalToolCallId).
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-trust-b',
      name: 'Write',
      kind: 'write' as any,
      args: { path: '/tmp/b.txt' },
    });
    await injectApproval(testCase, {
      toolCallId: 'tool-trust-b',
      toolName: 'Write',
      rawInput: { path: '/tmp/b.txt', content: 'hello' },
      withPrecedingToolCall: false,
      settleMs: 500,
    });

    const store = await testCase.getStore();
    const currentApprovalId = store.pendingApproval?.toolCall.toolCallId;
    // Once the second approval is pending, the page must be back to default:
    // the [y] allow-once line shows and the trust submenu confirm line is gone.
    if (currentApprovalId === 'tool-trust-b') {
      const snapshot3 = testCase.getSnapshot().join('\n');
      expect(snapshot3).toContain('[y]');
      expect(snapshot3).not.toContain('[enter] confirm');
    }

    await finishAndExitLite(testCase);
  }, 40000);

  it('[bug-mine 3.6] subagent attribution only when agentName differs from main', async () => {
    testCase = await launchLiteInteg('lite-approval-subagent-attr', {
      env: { KIRO_MOCK_AGENT_NAME: 'main-agent' },
      timeout: 20000,
    });

    const SUBAGENT_SESSION = 'sub-session-1';

    // Queue both before submitting (drained synchronously when prompt() fires).
    // Main-agent tool: no sessionId -> agentName = currentAgent.name.
    await injectApproval(testCase, {
      toolCallId: 'tool-main-1',
      toolName: 'Shell',
      rawInput: { command: 'echo main' },
      options: ALLOW_REJECT_OPTIONS,
    });

    // Subagent tool: distinct sessionId -> distinct agentName.
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-sub-1',
      name: 'Write',
      kind: 'write' as any,
      args: { path: '/tmp/sub.txt' },
      sessionId: SUBAGENT_SESSION,
    });
    await injectApproval(testCase, {
      toolCallId: 'tool-sub-1',
      toolName: 'Write',
      sessionId: SUBAGENT_SESSION,
      rawInput: { path: '/tmp/sub.txt', content: 'sub' },
      options: ALLOW_REJECT_OPTIONS,
      withPrecedingToolCall: false,
    });

    await testCase.typeAndSubmit('check');
    await testCase.sleepMs(300);

    const store1 = await testCase.getStore();
    expect(store1.pendingApproval).not.toBeNull();
    expect(store1.pendingApproval!.toolCall.toolCallId).toBe('tool-main-1');

    await testCase.sleepMs(2200);

    // Main-agent approval: no "subagent request" chip, agentName = main.
    expectApprovalVisible(testCase);
    const snapshot1 = testCase.getSnapshot().join('\n');
    expect(snapshot1).not.toContain('subagent request');
    const mainToolMsg = store1.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'tool-main-1'
    );
    expect(mainToolMsg).toBeDefined();
    if (mainToolMsg && 'agentName' in mainToolMsg) {
      expect((mainToolMsg as any).agentName).toBe('main-agent');
    }

    await testCase.sendKeys('y');
    await testCase.sleepMs(500);

    // Subagent approval promoted: distinct agentName, "subagent request" chip.
    const store2 = await testCase.getStore();
    expect(store2.pendingApproval).not.toBeNull();
    expect(store2.pendingApproval!.toolCall.toolCallId).toBe('tool-sub-1');
    const subToolMsg = store2.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'tool-sub-1'
    );
    expect(subToolMsg).toBeDefined();
    if (subToolMsg && 'agentName' in subToolMsg) {
      expect((subToolMsg as any).agentName).not.toBe('main-agent');
    }
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('subagent request');

    await finishAndExitLite(testCase);
  }, 40000);
});
