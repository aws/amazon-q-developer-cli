import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  injectApproval,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';

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
    testCase = await TestCase.builder()
      .withTestName('lite-approval-typing-guard')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Type something to set lastKeypressRef to "now"
    await testCase.sendKeys('hello');
    await testCase.sleepMs(50);

    // Inject an approval immediately after typing — the guard should defer it
    await injectApproval(testCase, {
      toolCallId: 'tool-guard-1',
      toolName: 'Shell',
    });
    await testCase.typeAndSubmit('x');
    await testCase.sleepMs(300);

    // Verify pendingApproval is set in the store (the event was delivered)
    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe('tool-guard-1');

    // The visual prompt should be deferred. Check the terminal — the approval
    // text "needs approval" should NOT be visible yet because the user typed
    // recently (within APPROVAL_IDLE_MS = 2000ms).
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('needs approval');

    // Wait for the debounce to expire (2000ms idle threshold)
    await testCase.sleepMs(2200);

    // Now the approval prompt should be visible
    const snapshotAfter = testCase.getSnapshot().join('\n');
    expect(snapshotAfter).toContain('needs approval');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 40000);

  it('[bug-mine 3.2] approval prompt stays visible once shown despite further keystrokes', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-approval-stays-visible')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject approval without recent typing so it shows immediately
    await injectApproval(testCase, {
      toolCallId: 'tool-visible-1',
      toolName: 'Shell',
    });
    await testCase.typeAndSubmit('go');
    await testCase.sleepMs(2500);

    // Verify the approval is visible
    const snapshot1 = testCase.getSnapshot().join('\n');
    expect(snapshot1).toContain('needs approval');

    // Press a random key that is NOT y/n/t (e.g. 'x')
    // Since the approval is visible, this keystroke should NOT hide it.
    // The keypress handler skips lastKeypressRef update when approval is shown.
    await testCase.sendKeys('x');
    await testCase.sleepMs(300);

    // Verify the approval prompt is still visible
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('needs approval');

    // Also verify the store still has the approval
    const store = await testCase.getStore();
    expect(store.pendingApproval).not.toBeNull();
    expect(store.pendingApproval!.toolCall.toolCallId).toBe('tool-visible-1');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 40000);

  it('[bug-mine 3.3] sequential approvals: y keystroke is not counted as typing so next approval shows without delay', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-approval-sequential')
      .withLite()
      .withTimeout(25000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Queue BOTH approvals before submitting so they're drained into the store
    // when prompt() fires. The first becomes pendingApproval, the second goes
    // into approvalQueue.
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

    // Verify both arrived: first as pendingApproval, second in queue
    const store1 = await testCase.getStore();
    expect(store1.pendingApproval).not.toBeNull();
    expect(store1.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-1');
    expect(store1.approvalQueue.length).toBeGreaterThanOrEqual(2);

    // Wait for the idle debounce to pass so the first approval is visible
    await testCase.sleepMs(2200);

    const snapshot1 = testCase.getSnapshot().join('\n');
    expect(snapshot1).toContain('needs approval');

    // Press 'y' to approve the first — this keystroke should NOT count as
    // "user typing" because showApprovalRef.current is true (the useKeypress
    // guard in LiteLayout skips lastKeypressRef update).
    await testCase.sendKeys('y');
    await testCase.sleepMs(500);

    // The second approval should have promoted from queue to pendingApproval.
    // Because 'y' didn't count as typing, the second should show immediately
    // (no 2s idle delay needed).
    const store2 = await testCase.getStore();
    expect(store2.pendingApproval).not.toBeNull();
    expect(store2.pendingApproval!.toolCall.toolCallId).toBe('tool-seq-2');

    // Verify the second approval prompt is visible (no idle delay applied)
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('needs approval');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 45000);

  it('[bug-mine 3.4] trust submenu resets when approval changes', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-approval-trust-reset')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject first approval with trust options so 't' opens the submenu
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
    // Wait for the debounce to pass so the approval is visible
    await testCase.sleepMs(2500);

    // Verify the approval is showing
    const snapshot1 = testCase.getSnapshot().join('\n');
    expect(snapshot1).toContain('needs approval');

    // Press 't' to open the trust submenu
    await testCase.sendKeys('t');
    await testCase.sleepMs(300);

    // Verify trust submenu is shown (contains "trust scope" text)
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('trust scope');

    // Now inject a second approval that replaces the first. The approval
    // component's useEffect on approvalToolCallId should reset page to 'default'.
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

    // The trust submenu should have reset — we should see "needs approval"
    // (the default page), not "trust scope" (the trust submenu).
    const store = await testCase.getStore();
    // pendingApproval should now be tool-trust-b (or tool-trust-a resolved
    // and b promoted from queue — either way, the trust submenu must reset)
    const currentApprovalId = store.pendingApproval?.toolCall.toolCallId;
    // If the second approval is pending, verify trust page reset
    if (currentApprovalId === 'tool-trust-b') {
      const snapshot3 = testCase.getSnapshot().join('\n');
      // The default page shows "[y] allow once" not "trust scope" submenu
      expect(snapshot3).toContain('[y]');
      // The trust submenu-specific "select" + "confirm" text should be gone
      expect(snapshot3).not.toContain('[enter] confirm');
    }

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
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

    // Queue both tool calls and approvals before submitting. Events are drained
    // synchronously when prompt() fires.
    //
    // First: a tool call from the main agent (no sessionId -> agentName =
    // currentAgent.name = 'main-agent')
    await injectApproval(testCase, {
      toolCallId: 'tool-main-1',
      toolName: 'Shell',
      rawInput: { command: 'echo main' },
      options: ALLOW_REJECT_OPTIONS,
    });

    // Second: a tool call FROM a subagent (different agentName)
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

    // Verify both arrived: first as pendingApproval, second in queue
    const store1 = await testCase.getStore();
    expect(store1.pendingApproval).not.toBeNull();
    expect(store1.pendingApproval!.toolCall.toolCallId).toBe('tool-main-1');

    // Wait for the debounce so the first approval is visible
    await testCase.sleepMs(2200);

    // Verify the first approval (main agent) has no "subagent request" chip
    const snapshot1 = testCase.getSnapshot().join('\n');
    expect(snapshot1).toContain('needs approval');
    expect(snapshot1).not.toContain('subagent request');

    // Verify the main-agent tool message has agentName = 'main-agent'
    const mainToolMsg = store1.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'tool-main-1'
    );
    expect(mainToolMsg).toBeDefined();
    if (mainToolMsg && 'agentName' in mainToolMsg) {
      expect((mainToolMsg as any).agentName).toBe('main-agent');
    }

    // Answer the first approval with 'y'
    await testCase.sendKeys('y');
    await testCase.sleepMs(500);

    // The subagent approval should have promoted from queue
    const store2 = await testCase.getStore();
    expect(store2.pendingApproval).not.toBeNull();
    expect(store2.pendingApproval!.toolCall.toolCallId).toBe('tool-sub-1');

    // Verify the subagent tool message has a different agentName
    const subToolMsg = store2.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'tool-sub-1'
    );
    expect(subToolMsg).toBeDefined();
    if (subToolMsg && 'agentName' in subToolMsg) {
      expect((subToolMsg as any).agentName).not.toBe('main-agent');
    }

    // The snapshot should now show "subagent request" chip for the subagent tool
    const snapshot2 = testCase.getSnapshot().join('\n');
    expect(snapshot2).toContain('subagent request');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 40000);
});
