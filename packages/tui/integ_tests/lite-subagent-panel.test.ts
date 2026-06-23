import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole, type MessageType } from '../src/stores/app-store';
import {
  finishAndExitLite,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';
import { seedSubagentPipeline } from '../e2e_tests/lite/helpers/subagents';
import {
  injectApproval,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';

type ToolUseMessage = Extract<MessageType, { role: MessageRole.ToolUse }>;

/**
 * Bug-mine 4.1, 4.2, 4.6: lite subagent panel behavior.
 * - 4.1 rows seeded from sessions, not just messages.
 * - 4.2 terminated sessions keep position (no reshuffle to tail).
 * - 4.6 panel auto-clamps/closes when the focused subagent disappears.
 */
describe('lite subagent panel [bug-mine 4.1, 4.2, 4.6]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  async function injectAndWaitForMessages(tc: TestCase): Promise<boolean> {
    await seedSubagentPipeline(tc, {
      parentId: 'subagent-parent-001',
      prompt: 't0',
      stages: [
        {
          toolId: 'tool-stage-a-1',
          name: 'Read',
          kind: 'read',
          args: { path: '/tmp/alpha.txt' },
          sessionId: 'session-alpha',
        },
        {
          toolId: 'tool-stage-b-1',
          name: 'Shell',
          kind: 'shell',
          args: { command: 'echo beta' },
          sessionId: 'session-beta',
        },
      ],
    });
    const store = await tc.getStore();
    const toolMessages = store.messages.filter((m) => m.role === 'tool_use');
    return toolMessages.length >= 2;
  }

  it('panel keyboard behavior: seed (4.1), Ctrl+O open / Esc close, arrows do not leak to prompt', async () => {
    testCase = await launchLiteInteg('lite-subagent-panel-keyboard');

    // Type input first so we can later prove arrows don't leak to the prompt.
    await testCase.sendKeys('test input');
    await testCase.sleepMs(100);

    const delivered = await injectAndWaitForMessages(testCase);
    expect(delivered).toBe(true);

    // 4.1: parent subagent tool + >=2 stage tools (agentName != main agent) seeded.
    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);
    expect(
      store.messages.find((m) => m.role === 'tool_use' && m.name === 'subagent')
    ).toBeDefined();
    const stageTools = store.messages.filter(
      (m): m is ToolUseMessage =>
        m.role === MessageRole.ToolUse &&
        m.name !== 'subagent' &&
        !!m.agentName &&
        m.agentName !== store.currentAgent?.name
    );
    expect(stageTools.length).toBeGreaterThanOrEqual(2);

    await testCase.sendKeys('\x0f'); // Ctrl+O opens
    await testCase.sleepMs(200);
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Arrows while open scroll the panel, not the prompt cursor.
    const inputBefore = store.input;
    await testCase.sendKeys('\x1b[A');
    await testCase.sleepMs(100);
    await testCase.sendKeys('\x1b[B');
    await testCase.sleepMs(100);
    store = await testCase.getStore();
    expect(store.input.lines).toEqual(inputBefore.lines);
    expect(store.input.cursorCol).toBe(inputBefore.cursorCol);
    expect(store.input.cursorRow).toBe(inputBefore.cursorRow);

    await testCase.pressEscape(); // Esc closes
    await testCase.sleepMs(200);
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    await finishAndExitLite(testCase);
  }, 30000);

  it('panel auto-closes when all subagents complete (4.6)', async () => {
    testCase = await launchLiteInteg('lite-subagent-panel-autoclose');

    const delivered = await injectAndWaitForMessages(testCase);
    expect(delivered).toBe(true);

    await testCase.sendKeys('\x0f'); // Ctrl+O opens panel
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Now finish the parent subagent tool — this should drain activeSubagents
    // and trigger auto-close of the panel.
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'subagent-parent-001',
      result: { status: 'success', output: 'Pipeline completed' },
    });
    await testCase.sleepMs(300);

    // After the parent finishes, activeSubagents should drain (guard:
    // anyParentSubagentRunning = false). The auto-clamp effect closes panel.
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    await finishAndExitLite(testCase);
  }, 30000);

  /**
   * Bug-mine 4.2: a completed stage keeps its footer slot (no reshuffle to tail)
   * when its summary tool finishes — stage messages keep their interleaved order
   * and the summary tool's agentName carries through via sessionId fallback (so
   * it isn't filtered out of the stage walk). Anchor: PR #2643 footer ordering.
   */
  it('completed stage stays in original position (4.2: terminated session seed)', async () => {
    testCase = await launchLiteInteg('lite-subagent-panel-order');

    const delivered = await injectAndWaitForMessages(testCase);
    expect(delivered).toBe(true);

    // Inject a summary tool for stage A that finishes (marks stage A complete)
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-summary-a',
      name: 'summary',
      args: { text: 'Stage A done' },
      sessionId: 'session-alpha',
    });
    await testCase.sleepMs(50);
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-summary-a',
      result: { status: 'success', output: 'Summary of stage A' },
    });
    await testCase.sleepMs(200);

    // Read the store directly (no render-trigger keystroke): a stray 'x' in the
    // prompt buffer breaks the Ctrl+C exit ladder downstream.
    await testCase.sleepMs(200);
    const store = await testCase.getStore();
    const stageMessages = store.messages.filter(
      (m): m is ToolUseMessage =>
        m.role === MessageRole.ToolUse &&
        !!m.agentName &&
        m.agentName !== store.currentAgent?.name &&
        m.name !== 'subagent'
    );

    expect(stageMessages.length).toBeGreaterThanOrEqual(3);

    // Find the first message from each stage
    const firstAlpha = stageMessages.findIndex(
      (m) => m.agentName === 'session-alpha' || m.agentName === 'stage-alpha'
    );
    const firstBeta = stageMessages.findIndex(
      (m) => m.agentName === 'session-beta' || m.agentName === 'stage-beta'
    );

    // Stage A's messages should come before Stage B's messages (spawn order)
    expect(firstAlpha).not.toBe(-1);
    expect(firstBeta).not.toBe(-1);
    expect(firstAlpha).toBeLessThan(firstBeta);

    await finishAndExitLite(testCase);
  }, 30000);

  /**
   * Panel auto-expand on inner approval (PR #2643; LiteLayout snapshot/restore
   * effect): when a pending approval belongs to a subagent stage the panel
   * auto-opens; on clear it restores its prior state. Both cases clear via 'n'
   * (RejectOnce — Esc would cancel the turn and auto-clamp the panel, masking
   * the restore); they differ only by the panel's PRIOR state.
   * KIRO_TEST_MOCK_TURN_TIMEOUT_MS keeps isProcessing alive past the 2s
   * APPROVAL_IDLE_MS gate so ApprovalPrompt's 'n' handler is mounted.
   */
  it.each([
    {
      label: 'auto-opens from CLOSED; closes on clear',
      testName: 'lite-auto-expand-from-closed',
      stages: [
        {
          sessionId: 'session-stageA',
          name: 'stageA',
          toolId: 'tool-stageA-1',
        },
      ],
      openFirst: false,
      approveToolId: 'tool-stageA-1',
      approveSessionId: 'session-stageA',
      postClearMs: 250,
      expectAfterClear: false,
    },
    {
      label: 'stays OPEN through B-approval; restores to open on clear',
      testName: 'lite-auto-expand-switch-stages',
      stages: [
        {
          sessionId: 'session-stageA',
          name: 'stageA',
          toolId: 'tool-stageA-1',
        },
        {
          sessionId: 'session-stageB',
          name: 'stageB',
          toolId: 'tool-stageB-1',
        },
      ],
      openFirst: true,
      approveToolId: 'tool-stageB-1',
      approveSessionId: 'session-stageB',
      // 250ms wasn't always enough for the React batched updates + ref read +
      // setSubagentOpenIndex round-trip when restoring to an open panel.
      postClearMs: 500,
      expectAfterClear: true,
    },
  ])(
    'auto-expand: inner approval $label',
    async ({
      testName,
      stages,
      openFirst,
      approveToolId,
      approveSessionId,
      postClearMs,
      expectAfterClear,
    }) => {
      testCase = await launchLiteInteg(testName, {
        env: { KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' },
        timeout: 20000,
      });

      await seedSubagentPipeline(testCase, {
        parentId: 'subagent-parent-autoexpand',
        pipeline: 'auto-expand-test',
        prompt: 'begin pipeline',
        stages: stages.map((s) => ({
          toolId: s.toolId,
          name: 'Read',
          kind: 'read',
          args: { path: `/tmp/${s.name}.txt` },
          sessionId: s.sessionId,
        })),
        addSessionsAfter: stages.map((s) => ({
          id: s.sessionId,
          name: s.name,
          status: 'busy',
        })),
      });

      let store = await testCase.getStore();
      if (openFirst) {
        await testCase.sendKeys('\x0f'); // Ctrl+O — prior state is "open".
        await testCase.sleepMs(200);
        store = await testCase.getStore();
        expect(store.subagentPanelOpen).toBe(true);
      } else {
        expect(store.subagentPanelOpen).toBe(false);
      }

      // Inner approval ties to the stage ToolCall seeded above (sessionId set),
      // so no preceding ToolCall is injected.
      await injectApproval(testCase, {
        toolCallId: approveToolId,
        toolName: 'Read',
        sessionId: approveSessionId,
        rawInput: { path: '/tmp/Read.txt' },
        options: ALLOW_REJECT_OPTIONS,
        withPrecedingToolCall: false,
        settleMs: 250,
      });

      store = await testCase.getStore();
      expect(store.subagentPanelOpen).toBe(true);
      expect(store.pendingApproval).not.toBeNull();

      // ApprovalPrompt binds 'n' only 2s after the last keystroke
      // (APPROVAL_IDLE_MS), so wait it out before responding.
      await testCase.sleepMs(2200);
      await testCase.sendKeys('n');
      await testCase.sleepMs(postClearMs);

      store = await testCase.getStore();
      expect(store.pendingApproval).toBeNull();
      expect(store.subagentPanelOpen).toBe(expectAfterClear);

      await finishAndExitLite(testCase);
    },
    30000
  );
});
