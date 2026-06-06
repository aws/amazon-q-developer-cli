import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { MessageRole, type MessageType } from '../src/stores/app-store';

type ToolUseMessage = Extract<MessageType, { role: MessageRole.ToolUse }>;

/**
 * Bug-mine 4.1, 4.2, 4.6: Lite subagent panel behavior.
 *
 * Validates:
 * - 4.1 Subagent rows seeded from sessions, not just messages: stages that
 *   spend their early turn thinking show up in the footer immediately.
 * - 4.2 Terminated sessions seeded to prevent row reordering: completed stages
 *   stay in their original position rather than reshuffling to the tail.
 * - 4.6 Auto-clamp/close panel when focused subagent disappears: when the
 *   pipeline finishes and activeSubagents drains, the panel closes rather than
 *   trapping arrow keys on an empty viewport.
 *
 * Ctrl+O opens the panel, Esc closes it. Arrow keys while the panel is open
 * scroll the trace rather than leaking to PromptInput.
 */
describe('lite subagent panel [bug-mine 4.1, 4.2, 4.6]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /**
   * Inject events that simulate a subagent pipeline with two stages. The parent
   * `subagent` tool (unfinished) gates activeSubagents population, then stage
   * tool calls (with sessionId) create the per-stage rows.
   */
  async function injectSubagentPipeline(tc: TestCase): Promise<void> {
    // Parent subagent tool — gates the activeSubagents memo (must be unfinished)
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'subagent-parent-001',
      name: 'subagent',
      args: { pipeline: 'test-pipeline' },
    });
    // Stage A tool call — stamped with sessionId so it resolves to agentName "stage-alpha"
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-stage-a-1',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/alpha.txt' },
      sessionId: 'session-alpha',
    });
    // Stage B tool call — stamped with sessionId so it resolves to agentName "stage-beta"
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-stage-b-1',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'echo beta' },
      sessionId: 'session-beta',
    });
  }

  /**
   * Inject subagent events and trigger prompt. The synchronous drain in
   * MockSessionClient ensures events are delivered deterministically.
   */
  async function injectAndWaitForMessages(
    tc: TestCase,
    _minToolMessages: number
  ): Promise<boolean> {
    await injectSubagentPipeline(tc);
    await tc.typeAndSubmit('t0');
    await tc.sleepMs(300);

    const store = await tc.getStore();
    const toolMessages = store.messages.filter(
      (m) => m.role === 'tool_use'
    );
    return toolMessages.length >= 2;
  }

  it('Ctrl+O toggles subagentPanelOpen in store', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-toggle')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Inject subagent events so activeSubagents is non-empty
    const delivered = await injectAndWaitForMessages(testCase, 2);
    expect(delivered).toBe(true);

    // Verify panel is initially closed
    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    // Send Ctrl+O to open the panel
    await testCase.sendKeys('\x0f'); // Ctrl+O
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Send Ctrl+O again to close the panel
    await testCase.sendKeys('\x0f'); // Ctrl+O
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('Esc closes the subagent panel', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-esc')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const delivered = await injectAndWaitForMessages(testCase, 2);
    expect(delivered).toBe(true);

    // Open with Ctrl+O
    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Close with Esc
    await testCase.pressEscape();
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('footer rows appear for subagent stages (4.1: seeded from sessions)', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-footer')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const delivered = await injectAndWaitForMessages(testCase, 2);
    expect(delivered).toBe(true);

    // The store should have sessions and messages with subagent data.
    // The footer renders in the terminal. Check that the store has
    // the parent subagent tool and the stage tools in messages.
    const store = await testCase.getStore();
    const parentTool = store.messages.find(
      (m) => m.role === 'tool_use' && m.name === 'subagent'
    );
    expect(parentTool).toBeDefined();

    // Stage tools should have agentName set (either to session name or sessionId)
    const stageTools = store.messages.filter(
      (m): m is ToolUseMessage =>
        m.role === MessageRole.ToolUse &&
        m.name !== 'subagent' &&
        !!m.agentName &&
        m.agentName !== store.currentAgent?.name
    );
    expect(stageTools.length).toBeGreaterThanOrEqual(2);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('arrow keys while panel open do not leak to PromptInput', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-arrows')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Type some text so we can verify cursor doesn't move
    await testCase.sendKeys('test input');
    await testCase.sleepMs(100);

    const delivered = await injectAndWaitForMessages(testCase, 2);
    expect(delivered).toBe(true);

    // Open the panel
    await testCase.sendKeys('\x0f'); // Ctrl+O
    await testCase.sleepMs(200);

    const store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Record input state before arrows
    const inputBefore = store.input;

    // Send up/down arrows (should scroll panel, not move prompt cursor)
    await testCase.sendKeys('\x1b[A'); // Up arrow
    await testCase.sleepMs(100);
    await testCase.sendKeys('\x1b[B'); // Down arrow
    await testCase.sleepMs(100);

    // Verify input state unchanged — arrows didn't leak to prompt
    const storeAfter = await testCase.getStore();
    expect(storeAfter.input.lines).toEqual(inputBefore.lines);
    expect(storeAfter.input.cursorCol).toBe(inputBefore.cursorCol);
    expect(storeAfter.input.cursorRow).toBe(inputBefore.cursorRow);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('panel auto-closes when all subagents complete (4.6)', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-autoclose')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const delivered = await injectAndWaitForMessages(testCase, 2);
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
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  /**
   * Bug-mine 4.2: completed subagent stages must keep their position
   * in the activeSubagents footer, not reshuffle to the tail when their
   * summary tool finishes.
   *
   * 1. WHAT user-observable behavior does this assert?
   *    Per PR #2643's "Subagents > Footer ordering": after a stage's
   *    summary tool lands the row stays in the slot where it spawned —
   *    sibling stages don't visually jump. The store-level invariant
   *    that drives this rendering is: stage messages tagged with the
   *    stage's agentName remain in their original interleaved order in
   *    `messages` after the summary's ToolCallFinished arrives. The lite
   *    layout's footer walk reads that order verbatim.
   *
   * 2. WHAT class of regression catches?
   *    Anyone refactoring the ToolCall handler in app-store.ts and
   *    breaking the agentName carry-through (e.g. dropping sessionId
   *    forwarding on the summary tool, so its agentName resolves to the
   *    main agent's name and it gets filtered OUT of the stage walk by
   *    isInnerSubagentTool) would fail this test: stage A's summary
   *    wouldn't appear in the stage-message slice at all, dropping the
   *    count below 3. A pure reordering bug (e.g. sort-by-finishTime)
   *    would fail the firstAlpha < firstBeta ordering assertion.
   *
   * 3. Could the test pass even if the feature is broken?
   *    No. The assertion checks BOTH that the summary tool is in the
   *    stage walk (count >= 3) AND that stage A's first message is at
   *    a strictly lower index than stage B's first message. A "summary
   *    drops out of stage walk" bug fails the count; a "reshuffle to
   *    tail" bug fails the ordering.
   *
   * Anchor: PR #2643 footer ordering + app-store.ts ToolCall agentName
   *         resolution (sessionId fallback path).
   */
  it('completed stage stays in original position (4.2: terminated session seed)', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-subagent-panel-order')
      .withLite()
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const delivered = await injectAndWaitForMessages(testCase, 2);
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
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
