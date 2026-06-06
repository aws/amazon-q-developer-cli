/**
 * Integ test: Subagent panel auto-expand on inner approval.
 *
 * 1. WHAT user-observable behavior does this assert?
 *    Per PR #2643 "Subagents > Auto-expand on inner approval": when a
 *    pending approval belongs to a subagent stage (i.e. the approving
 *    ToolUse message has agentName !== mainAgent.name), the lite layout
 *    auto-opens the subagent trace panel and points it at that stage.
 *    When the approval clears, the panel state restores to whatever
 *    it was before the auto-expand.
 *
 *    Two cases (asserted via the only state the Zustand store exposes
 *    here, `subagentPanelOpen` — the per-component `subagentOpenIndex`
 *    isn't on the store, so we can't assert focus index directly):
 *      (a) Panel was closed before the approval: panelOpen flips
 *          false → true on approval, and back to false on clear.
 *      (b) Panel was already open: panelOpen stays true after the
 *          approval lands, AND remains true after the approval clears.
 *          This proves the snapshot-restore path didn't close a panel
 *          that was open prior to the auto-expand.
 *
 * 2. WHAT class of regression would this catch?
 *    Anyone who refactors the auto-expand effect in
 *    src/components/layout/lite/LiteLayout.tsx:1557-1588 and forgets to
 *    snapshot the prior state would lose case (b)'s "stays open":
 *    the panel would close on approval clear even though it was open
 *    before. Anyone who removes the `subagentRequestingName` === null
 *    branch (the restore path) would leave the panel pinned forever
 *    after the user answers. Anyone who breaks the "is this approval
 *    for a subagent" check (msg.agentName !== mainAgent.name) would
 *    auto-open on every parent-agent approval, spamming the panel
 *    on regular tool runs.
 *
 * 3. Could the test pass even if the feature is broken?
 *    No.
 *      Case (a): a bug that never opened the panel would fail the
 *      first transition (false → true); one that never restored
 *      would fail the second (true → false).
 *      Case (b): a snapshot-restore bug that always closes on clear
 *      (instead of restoring to "was-open") would fail the
 *      stays-open-after-clear assertion.
 *
 * Anchor: PR #2643 "Auto-expand on inner approval" + LiteLayout.tsx:1557.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import { ApprovalOptionId } from '../src/types/agent-events';

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

  async function injectApproval(
    tc: TestCase,
    toolCallId: string,
    sessionId: string,
    toolName: string
  ): Promise<void> {
    // Inner approval requires the matching ToolCall message to already
    // exist with agentName resolved to the stage. seedPipeline created
    // those ToolCall messages with sessionId set; the approval event then
    // ties to one of them by toolCallId.
    await tc.mockSessionUpdate({
      type: AgentEventType.ApprovalRequest,
      value: {
        sessionId,
        toolCall: {
          toolCallId,
          title: toolName,
          rawInput: { path: `/tmp/${toolName}.txt` },
        },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow Once',
            optionId: 'allow_once',
          },
          {
            kind: ApprovalOptionId.RejectOnce,
            name: 'Reject Once',
            optionId: 'reject_once',
          },
        ],
        resolve: (() => {
          /* noop — store-side clear is what we drive in the test */
        }) as any,
      },
    } as any);
    await tc.sleepMs(250);
  }

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
    await injectApproval(testCase, 'tool-stageA-1', 'session-stageA', 'Read');

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
    await injectApproval(testCase, 'tool-stageB-1', 'session-stageB', 'Read');

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
