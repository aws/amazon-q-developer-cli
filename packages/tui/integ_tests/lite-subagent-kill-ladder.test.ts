/**
 * Subagent kill ladder (Ctrl+X): first press arms, a second within 2s
 * terminates, a second after the window only re-arms (kills nothing).
 * Integ (not e2e): all assertions are at the TUI store layer and the mock
 * backend gives deterministic timing for the 2s window.
 * Anchor: PR #2643 (Subagents > Kill ladder Ctrl+X);
 *          src/components/layout/lite/LiteLayout.tsx:1611-1703.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

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
    testCase = await launchLiteInteg('lite-kill-ladder-arm-then-kill', {
      timeout: 20000,
    });

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
    await exitLiteInteg(testCase);
  }, 30000);

  it('second Ctrl+X after the 2s window only re-arms (does not kill)', async () => {
    testCase = await launchLiteInteg('lite-kill-ladder-window-expires', {
      timeout: 20000,
    });

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
    await exitLiteInteg(testCase);
  }, 30000);
});
