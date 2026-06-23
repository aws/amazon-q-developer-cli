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

import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import {
  injectApproval,
  ALLOW_REJECT_OPTIONS,
} from '../e2e_tests/lite/helpers/approvals';
import {
  finishAndExitLite,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';
import { seedSubagentPipeline } from '../e2e_tests/lite/helpers/subagents';

describe('lite subagent panel auto-expand on inner approval', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  async function seedPipeline(
    tc: TestCase,
    stages: ReadonlyArray<{ sessionId: string; name: string; toolId: string }>
  ): Promise<void> {
    await seedSubagentPipeline(tc, {
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

  // Both cases inject an inner-subagent approval (auto-opens / re-points the
  // panel) then clear it via 'n' (RejectOnce — Esc would cancel the whole turn
  // and auto-clamp the panel, masking the snapshot-restore behavior under
  // test). They differ only by the panel's PRIOR state and what restore yields:
  //  - was-closed (1 stage): false→true on approval, restores to false.
  //  - was-open  (2 stages, opened via Ctrl+O; approval on stage B): stays true
  //    through approval AND after clear (proves restore didn't close a panel
  //    that was open beforehand).
  // KIRO_TEST_MOCK_TURN_TIMEOUT_MS keeps isProcessing alive past the 2s
  // APPROVAL_IDLE_MS gate so ApprovalPrompt's 'n' handler is mounted.
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
    'inner approval $label',
    async ({
      testName,
      stages,
      openFirst,
      approveToolId,
      approveSessionId,
      postClearMs,
      expectAfterClear,
    }) => {
      testCase = await TestCase.builder()
        .withTestName(testName)
        .withLite()
        .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
        .withTimeout(20000)
        .launch();

      await testCase.waitForVisibleText('ask a question', 10000);

      await seedPipeline(testCase, stages);

      let store = await testCase.getStore();
      if (openFirst) {
        // Open the panel manually (Ctrl+O) so its prior state is "open".
        await testCase.sendKeys('\x0f');
        await testCase.sleepMs(200);
        store = await testCase.getStore();
        expect(store.subagentPanelOpen).toBe(true);
      } else {
        // Panel starts closed (default state in lite mode).
        expect(store.subagentPanelOpen).toBe(false);
      }

      await injectInnerApproval(
        testCase,
        approveToolId,
        approveSessionId,
        'Read'
      );

      store = await testCase.getStore();
      expect(store.subagentPanelOpen).toBe(true);
      expect(store.pendingApproval).not.toBeNull();

      // Clear by responding 'n'; ApprovalPrompt's keypress handler binds 'n'
      // only 2s after the last keystroke (APPROVAL_IDLE_MS), so wait it out.
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
