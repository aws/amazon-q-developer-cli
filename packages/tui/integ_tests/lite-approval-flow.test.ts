import { afterEach, describe, expect, it } from 'bun:test';
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
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Bug-mine 3.1, 3.2, 3.3, 3.4, 3.6: Lite approval flow behavior.
 *
 * 3.1 — Approval typing guard (APPROVAL_IDLE_MS debounce): a keystroke typed
 *        before an approval arrives defers the prompt.
 * 3.2 — Approval prompt stays visible once shown: further keystrokes do NOT
 *        hide it.
 * 3.3 — Approval keypress skip while prompt visible: 'y' keystroke is not
 *        counted as "user is typing", so sequential approvals arrive without
 *        2s delay.
 * 3.4 — Trust submenu resets on approval change: switching approvals closes
 *        the trust submenu.
 * 3.6 — Subagent attribution only when agentName differs from main.
 */
describe('lite approval flow [bug-mine 3.1, 3.2, 3.3, 3.4, 3.6]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('[bug-mine 3.1] typing guard defers approval prompt via APPROVAL_IDLE_MS debounce', async () => {
    testCase = await launchLiteInteg('lite-approval-typing-guard', {
      timeout: 20000,
    });

    // Set lastKeypressRef to "now", then inject an approval — the guard must
    // defer the prompt while the keypress is within APPROVAL_IDLE_MS (2000ms).
    await testCase.sendKeys('hello');
    await testCase.sleepMs(50);
    await injectApproval(testCase, {
      toolCallId: 'tool-guard-1',
      toolName: 'Shell',
    });
    await testCase.typeAndSubmit('x');
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe('tool-guard-1');

    // Deferred: event delivered to store but prompt not yet painted.
    expectApprovalDeferred(testCase);

    // Past the 2000ms idle threshold the prompt appears.
    await testCase.sleepMs(2200);
    expectApprovalVisible(testCase);

    await finishAndExitLite(testCase);
  }, 40000);

  it('[bug-mine 3.2] approval prompt stays visible once shown despite further keystrokes', async () => {
    testCase = await launchLiteInteg('lite-approval-stays-visible', {
      timeout: 20000,
    });

    // No recent typing -> approval shows immediately.
    await injectApproval(testCase, {
      toolCallId: 'tool-visible-1',
      toolName: 'Shell',
    });
    await testCase.typeAndSubmit('go');
    await testCase.sleepMs(2500);

    expectApprovalVisible(testCase);

    // A non-y/n/t key must NOT hide the prompt: the keypress handler skips
    // the lastKeypressRef update while an approval is shown.
    await testCase.sendKeys('x');
    await testCase.sleepMs(300);

    expectApprovalVisible(testCase);

    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe('tool-visible-1');

    await finishAndExitLite(testCase);
  }, 40000);

  it('[bug-mine 3.3] sequential approvals: y keystroke is not counted as typing so next approval shows without delay', async () => {
    testCase = await launchLiteInteg('lite-approval-sequential', {
      timeout: 25000,
    });

    // Queue BOTH approvals before submitting so both drain when prompt() fires:
    // first -> pendingApproval, second -> approvalQueue.
    await injectApproval(testCase, {
      toolCallId: 'tool-seq-1',
      toolName: 'Shell',
    });
    await injectApproval(testCase, {
      toolCallId: 'tool-seq-2',
      toolName: 'Write',
    });

    await testCase.typeAndSubmit('start');
    await testCase.sleepMs(300);

    const store1 = await testCase.getStore();
    expect(store1.pendingApproval).not.toBeNull();
    expect(store1.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-1');
    expect(store1.approvalQueue.length).toBeGreaterThanOrEqual(2);

    await testCase.sleepMs(2200);
    expectApprovalVisible(testCase);

    // 'y' must NOT count as "user typing" (the useKeypress guard skips the
    // lastKeypressRef update while an approval is shown), so the promoted
    // second approval shows with no fresh 2s idle delay.
    await testCase.sendKeys('y');
    await testCase.sleepMs(500);

    const store2 = await testCase.getStore();
    expect(store2.pendingApproval).not.toBeNull();
    expect(store2.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-2');

    expectApprovalVisible(testCase);

    await finishAndExitLite(testCase);
  }, 45000);

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
    testCase = await TestCase.builder()
      .withTestName('lite-approval-subagent-attr')
      .withLite()
      .withEnv({ KIRO_MOCK_AGENT_NAME: 'main-agent' })
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

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
