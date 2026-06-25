/**
 * Subagent kill ladder (Ctrl+X): first press arms, a second within 2s
 * terminates, a second after the window only re-arms (kills nothing).
 * Integ (not e2e): all assertions are at the TUI store layer and the mock
 * backend gives deterministic timing for the 2s window.
 * Anchor: PR #2643 (Subagents > Kill ladder Ctrl+X);
 *          src/components/layout/lite/LiteLayout.tsx:1611-1703.
 */

import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import {
  finishAndExitLite,
  launchLiteInteg,
  trackCleanup,
} from './helpers/integ-lifecycle';
import { seedSubagentPipeline } from './helpers/subagents';

describe('lite subagent kill ladder Ctrl+X', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  async function seedStage(
    tc: TestCase,
    sessionId: string,
    name: string,
    toolId: string
  ): Promise<{ sessionId: string; name: string }> {
    await seedSubagentPipeline(tc, {
      parentId: `subagent-parent-${sessionId}`,
      pipeline: 'kill-ladder-test',
      prompt: 'arm',
      stages: [
        {
          toolId,
          name: 'Read',
          kind: 'read',
          args: { path: `/tmp/${name}.txt` },
          sessionId,
        },
      ],
      addSessionsAfter: [{ id: sessionId, name, status: 'busy' }],
    });
    return { sessionId, name };
  }

  // Both cases seed a stage, open the panel, and arm with one Ctrl+X (status
  // stays 'busy'). They differ only in the gap before the SECOND Ctrl+X:
  //  - within the 2s window: kill fires, status flips to 'terminated'
  //    synchronously (LiteLayout updateSession() before the terminate RPC).
  //  - after the window: the prior arm auto-disarmed (armedKillSessionId is
  //    null), so the second press only re-arms — no terminate, stays 'busy'.
  it.each([
    {
      label: 'within 2s window terminates',
      testName: 'lite-kill-ladder-arm-then-kill',
      sessionId: 'session-killtarget',
      name: 'killtarget',
      toolId: 'tool-killtarget-1',
      waitBeforeSecond: 0,
      expectedAfterSecond: 'terminated',
    },
    {
      label: 'after 2s window only re-arms',
      testName: 'lite-kill-ladder-window-expires',
      sessionId: 'session-survivor',
      name: 'survivor',
      toolId: 'tool-survivor-1',
      waitBeforeSecond: 2300,
      expectedAfterSecond: 'busy',
    },
  ])(
    'first Ctrl+X arms; second $label',
    async ({
      testName,
      sessionId,
      name,
      toolId,
      waitBeforeSecond,
      expectedAfterSecond,
    }) => {
      testCase = await launchLiteInteg(testName, { timeout: 20000 });

      const stage = await seedStage(testCase, sessionId, name, toolId);

      // Sanity: the stage is in the store and still busy.
      let store = await testCase.getStore();
      const sessionsObj = store.sessions as unknown as Record<string, any>;
      expect(Object.keys(sessionsObj ?? {}).length).toBeGreaterThan(0);
      expect(sessionsObj[stage.sessionId]?.status).toBe('busy');

      // Open the subagent panel (Ctrl+O). The kill-ladder handler bails when
      // subagentOpenIndex == null, so the panel must be open for Ctrl+X to
      // reach the kill branch.
      await testCase.sendKeys('\x0f');
      await testCase.sleepMs(200);
      store = await testCase.getStore();
      expect(store.subagentPanelOpen).toBe(true);

      // First Ctrl+X — arms only, status stays 'busy'.
      await testCase.sendKeys('\x18');
      await testCase.sleepMs(150);
      store = await testCase.getStore();
      expect((store.sessions as any)[stage.sessionId]?.status).toBe('busy');

      if (waitBeforeSecond) await testCase.sleepMs(waitBeforeSecond);

      // Second Ctrl+X.
      await testCase.sendKeys('\x18');
      await testCase.sleepMs(200);
      store = await testCase.getStore();
      expect((store.sessions as any)[stage.sessionId]?.status).toBe(
        expectedAfterSecond
      );

      await finishAndExitLite(testCase);
    },
    30000
  );
});
