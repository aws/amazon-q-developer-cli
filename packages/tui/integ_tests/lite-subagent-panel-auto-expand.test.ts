/**
 * Subagent panel auto-expand on inner approval (PR #2643;
 * LiteLayout.tsx:1557-1588 snapshot/restore effect).
 *
 * When a pending approval belongs to a subagent stage (agentName !==
 * mainAgent.name) the panel auto-opens; on clear it restores its prior state.
 * Two cases, asserted via store `subagentPanelOpen`:
 *   (a) was-closed: false→true on approval, back to false on clear.
 *   (b) was-open: stays true on approval AND after clear (proves the
 *       snapshot-restore didn't close a panel that was open beforehand).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import {
  injectApproval,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';

describe('lite subagent panel auto-expand on inner approval', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  /** Seed parent subagent + N stage rows, with sessions populated. */
  async function seedPipeline(
    tc: TestCase,
    stages: Array<{ sessionId: string; name: string; toolId: string }>
  ): Promise<void> {
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'subagent-parent-autoexpand',
      name: 'subagent',
      args: { pipeline: 'auto-expand-test' },
    });
    for (const s of stages) {
      await tc.mockSessionUpdate({
        type: AgentEventType.ToolCall,
        id: s.toolId,
        name: 'Read',
        kind: 'read',
        args: { path: `/tmp/${s.name}.txt` },
        sessionId: s.sessionId,
      });
    }
    await tc.typeAndSubmit('begin pipeline');
    await tc.sleepMs(300);
    // Seed sessions AFTER the parent subagent ToolCall fires so its
    // "wipe ephemeral sessions" branch doesn't clobber them.
    for (const s of stages) {
      await tc.mockAddSession({
        id: s.sessionId,
        name: s.name,
        status: 'busy',
      });
    }
    await tc.sleepMs(150);
  }

  // Inner approval ties to a ToolCall message seedPipeline already created
  // (with sessionId set), so no preceding ToolCall is injected here.
  const injectInnerApproval = (
    tc: TestCase,
    toolCallId: string,
    sessionId: string,
    toolName: string
  ) =>
    injectApproval(tc, {
      toolCallId,
      toolName,
      sessionId,
      rawInput: { path: `/tmp/${toolName}.txt` },
      options: ALLOW_REJECT_OPTIONS,
      withPrecedingToolCall: false,
      settleMs: 250,
    });

  it('auto-opens panel from CLOSED on inner approval; closes on clear', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-auto-expand-from-closed')
      .withLite()
      // Same as the case-(b) test: keep isProcessing alive past the
      // APPROVAL_IDLE_MS gate so the user-response path is reachable.
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await seedPipeline(testCase, [
      {
        sessionId: 'session-stageA',
        name: 'stageA',
        toolId: 'tool-stageA-1',
      },
    ]);

    // Panel starts closed — typing-guard / autoclose tests confirm this is
    // the default state in lite mode.
    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);

    // Inject an approval for stage A's tool.
    await injectInnerApproval(
      testCase,
      'tool-stageA-1',
      'session-stageA',
      'Read'
    );

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);
    expect(store.pendingApproval).not.toBeNull();

    // Clear the approval by responding (NOT by Esc — Esc cancels the
    // entire turn, which would auto-clamp the panel closed regardless
    // of the snapshot-restore behavior we want to test). ApprovalPrompt
    // is gated by APPROVAL_IDLE_MS so its keypress handler doesn't bind
    // to 'n' until 2s after the last keystroke. Wait that out, then
    // press 'n' (RejectOnce) to clear pendingApproval cleanly.
    await testCase.sleepMs(2200);
    await testCase.sendKeys('n');
    await testCase.sleepMs(250);

    store = await testCase.getStore();
    expect(store.pendingApproval).toBeNull();
    // Restored to prior (closed) state — the snapshot-restore path fired.
    expect(store.subagentPanelOpen).toBe(false);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('switches focus from stage A to stage B on B-approval; restores to A on clear', async () => {
    testCase = await TestCase.builder()
      .withTestName('lite-auto-expand-switch-stages')
      .withLite()
      // The mock client auto-resolves the prompt after 2s by default. This
      // test must keep isProcessing=true past the APPROVAL_IDLE_MS gate
      // (also 2s) so ApprovalPrompt's 'n' handler is mounted. Bump the
      // mock turn timeout for this single test.
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
      .withTimeout(20000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    await seedPipeline(testCase, [
      {
        sessionId: 'session-stageA',
        name: 'stageA',
        toolId: 'tool-stageA-1',
      },
      { sessionId: 'session-stageB', name: 'stageB', toolId: 'tool-stageB-1' },
    ]);

    // Open the panel manually (Ctrl+O).
    await testCase.sendKeys('\x0f');
    await testCase.sleepMs(200);

    let store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);

    // Approval comes from stage B. Auto-expand snapshots the prior state
    // (panel open) and points the panel at stage B. The panel remains
    // visibly open during the approval — observable via subagentPanelOpen.
    await injectInnerApproval(
      testCase,
      'tool-stageB-1',
      'session-stageB',
      'Read'
    );

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);
    expect(store.pendingApproval).not.toBeNull();

    // Clear approval by responding (n = RejectOnce). The panel must
    // remain open afterwards (it was open before the swap), which proves
    // the snapshot-restore path didn't close a panel that was open prior
    // to the auto-expand. Esc would cancel the whole turn and auto-clamp
    // the panel — which would mask whether the restore actually fired.
    await testCase.sleepMs(2200);
    await testCase.sendKeys('n');
    // Allow a couple of render passes for the restore effect to run after
    // pendingApproval clears. 250ms wasn't always enough for the React
    // batched updates + ref read + setSubagentOpenIndex round-trip.
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    expect(store.pendingApproval).toBeNull();
    expect(store.subagentPanelOpen).toBe(true);

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
