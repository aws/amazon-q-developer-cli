import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole, type MessageType } from '../src/stores/app-store';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';
import { seedSubagentPipeline } from '../e2e_tests/lite/helpers/subagents';

type ToolUseMessage = Extract<MessageType, { role: MessageRole.ToolUse }>;

/**
 * Bug-mine 4.1, 4.2, 4.6: lite subagent panel behavior.
 * - 4.1 rows seeded from sessions, not just messages.
 * - 4.2 terminated sessions keep position (no reshuffle to tail).
 * - 4.6 panel auto-clamps/closes when the focused subagent disappears.
 */
describe('lite subagent panel [bug-mine 4.1, 4.2, 4.6]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

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

  it('panel keyboard + footer behavior: seed (4.1), Ctrl+O/Esc toggle, arrows do not leak to prompt', async () => {
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

    // Ctrl+O opens.
    await testCase.sendKeys('\x0f');
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

    // Esc closes.
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    // Ctrl+O reopens and Ctrl+O again closes (toggle).
    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);
    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);
    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await exitLiteInteg(testCase);
  }, 30000);

  it('panel auto-closes when all subagents complete (4.6)', async () => {
    testCase = await launchLiteInteg('lite-subagent-panel-autoclose');

    const delivered = await injectAndWaitForMessages(testCase);
    expect(delivered).toBe(true);

    // Open the panel
    await testCase.sendKeys('\x0f'); // Ctrl+O
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

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await exitLiteInteg(testCase);
  }, 30000);

  /**
   * Bug-mine 4.2: a completed stage keeps its footer slot (no reshuffle to tail)
   * when its summary tool finishes. The driving invariant: stage messages keep
   * their original interleaved order in `messages`, and the summary tool's
   * agentName carries through via sessionId fallback (so it isn't filtered out
   * of the stage walk). Asserts BOTH count >= 3 (summary stays in walk) AND
   * firstAlpha < firstBeta (no reorder). Anchor: PR #2643 footer ordering.
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

    // Verify messages: stage A's tools should appear BEFORE stage B's tools.
    // This validates 4.2: terminated sessions are seeded in original order so
    // the message-walk doesn't re-introduce them at the tail. The store is
    // read directly so no render-trigger keystroke is needed (a stray 'x'
    // in the prompt buffer breaks the Ctrl+C exit ladder downstream).
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

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await exitLiteInteg(testCase);
  }, 30000);
});
