/**
 * Integ test: Subagent kill ladder (Ctrl+X two-stage arm → kill).
 *
 * Filed in the original test brief as e2e because the kill ladder is a
 * user-facing keybinding documented in PR #2643's surface area. Implemented
 * here as integ because the assertions all live at the TUI layer
 * (store.sessions[id].status, store.subagentPanelOpen, no-op-after-window),
 * and a true e2e exercise would require driving a real backend subagent
 * pipeline — the mock-backed path covers the same regression class with
 * deterministic timing.
 *
 * 1. WHAT user-observable behavior does this assert?
 *    Per PR #2643 "Subagents > Kill ladder (Ctrl+X)":
 *      - First Ctrl+X while the subagent panel is open ARMS the kill (the
 *        store records armedKillSessionId; the LiteLayout shows a yellow
 *        "ctrl+x KILL" chip + "esc cancel" hint).
 *      - A second Ctrl+X within the 2s window invokes terminateSession
 *        and flips the focused stage's session status to 'terminated'.
 *      - A second Ctrl+X AFTER the 2s window does NOT kill — the prior
 *        arm timed out and re-pressing only re-arms (kills nothing).
 *
 * 2. WHAT class of regression would this catch?
 *    Anyone who edits the kill-ladder timing in
 *    src/components/layout/lite/LiteLayout.tsx (the 2000ms setTimeout
 *    or the armedKillSessionId comparison) or refactors
 *    `subagentSessionIdByName` and forgets to keep the focused-stage
 *    lookup, would break the Ctrl+X kill path. Symmetrically, anyone
 *    who removes the 2s safety window — making the FIRST press kill
 *    immediately — would also fail this test, because the test asserts
 *    the first press only arms (status stays 'busy').
 *
 * 3. Could the test pass even if the feature is broken?
 *    No. Each assertion targets a distinct observable transition:
 *      - After 1st Ctrl+X: status === 'busy' (NOT terminated)
 *      - After 2nd Ctrl+X within window: status === 'terminated'
 *      - After single Ctrl+X + 2.2s wait + 2nd Ctrl+X (re-arm only):
 *        status of a SECOND stage stays 'busy' (kill never fires)
 *    A broken implementation that ignored Ctrl+X, killed on first
 *    press, or ignored the window would fail at least one assertion.
 *
 * Anchor: PR #2643 surface area (Subagents > Kill ladder Ctrl+X) +
 *          src/components/layout/lite/LiteLayout.tsx:1611-1703 (kill ladder).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';

describe('lite subagent kill ladder Ctrl+X', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /**
   * Seeds the minimum state needed for activeSubagents to be non-empty.
   *
   * Ordering matters:
   *   - Inject the parent `subagent` ToolCall + the stage ToolCall first.
   *     These flow through the agent stream and stamp messages with
   *     agentName + sessionId.
   *   - typeAndSubmit triggers the mocked prompt, which drains the queue
   *     and processes those ToolCalls. The `subagent` ToolCall handler
   *     wipes any pre-existing ephemeral sessions (see
   *     src/stores/app-store.ts:2475 → SESSION_TOOL_NAMES) — so we
   *     can't seed the session BEFORE this point.
   *   - mockAddSession after the prompt is in flight (isProcessing=true)
   *     adds the stage to sessions.values() without being wiped.
   *
   * Returns the (name, sessionId) the test should focus on.
   */
  async function seedStage(
    tc: TestCase,
    sessionId: string,
    name: string,
    toolId: string
  ): Promise<{ sessionId: string; name: string }> {
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: `subagent-parent-${sessionId}`,
      name: 'subagent',
      args: { pipeline: 'kill-ladder-test' },
    });
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: toolId,
      name: 'Read',
      kind: 'read',
      args: { path: `/tmp/${name}.txt` },
      sessionId,
    });
    await tc.typeAndSubmit('arm');
    await tc.sleepMs(300);
    // Seed AFTER the parent subagent ToolCall has been processed so it
    // doesn't wipe our ephemeral session row.
    await tc.mockAddSession({ id: sessionId, name, status: 'busy' });
    await tc.sleepMs(150);
    return { sessionId, name };
  }

  it('first Ctrl+X arms (status stays busy); second within 2s terminates', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-kill-ladder-arm-then-kill')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const stage = await seedStage(
      testCase,
      'session-killtarget',
      'killtarget',
      'tool-killtarget-1'
    );

    // Sanity: the stage is in the store and still busy.
    let store = await testCase.getStore();
    const sessionsObj = store.sessions as unknown as Record<string, any>;
    expect(Object.keys(sessionsObj ?? {}).length).toBeGreaterThan(0);
    expect(sessionsObj[stage.sessionId]?.status).toBe('busy');

    // Open the subagent panel (Ctrl+O). The kill-ladder handler bails when
    // subagentOpenIndex == null, so the panel must be open for Ctrl+X
    // to even reach the kill branch.
    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // First Ctrl+X — must ARM only. Status stays 'busy'.
    await testCase.sendKeys('\x18');
    await testCase.sleepMs(150);

    store = await testCase.getStore();
    const afterFirst = (store.sessions as any)[stage.sessionId];
    expect(afterFirst?.status).toBe('busy');

    // Second Ctrl+X within the 2s window — kill fires. Status flips to
    // 'terminated' synchronously (LiteLayout calls updateSession() before
    // awaiting the kiro.terminateSession RPC).
    await testCase.sendKeys('\x18');
    await testCase.sleepMs(150);

    store = await testCase.getStore();
    const afterSecond = (store.sessions as any)[stage.sessionId];
    expect(afterSecond?.status).toBe('terminated');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('second Ctrl+X after the 2s window only re-arms (does not kill)', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-kill-ladder-window-expires')
      .withLite()
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    const stage = await seedStage(
      testCase,
      'session-survivor',
      'survivor',
      'tool-survivor-1'
    );

    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // First press arms.
    await testCase.sendKeys('\x18');
    await testCase.sleepMs(150);

    store = await testCase.getStore();
    const afterArm = (store.sessions as any)[stage.sessionId];
    expect(afterArm?.status).toBe('busy');

    // Wait past the 2s arm window. The prior arm should auto-disarm.
    await testCase.sleepMs(2300);

    // Second press AFTER the window. This must re-arm only — the prior
    // arm timed out so the equality check (armedKillSessionId === sessionId)
    // is false (armedKillSessionId is null), and we go into the arm branch
    // again. No terminate fires.
    await testCase.sendKeys('\x18');
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    const afterSecond = (store.sessions as any)[stage.sessionId];
    expect(afterSecond?.status).toBe('busy');

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
