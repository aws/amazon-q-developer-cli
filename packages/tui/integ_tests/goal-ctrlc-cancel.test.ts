import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  launchLiteInteg,
  trackCleanup,
} from './helpers/integ-lifecycle';

/**
 * Ctrl+C during a goal loop must step down gracefully — pause the loop, then
 * cancel the goal — and never fall through to the process-exit sequence while
 * a goal is still set. Exercises the real PTY → dispatchAppKeypress →
 * cancelGoal path end to end.
 */
describe('goal Ctrl+C cancel path', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  it('double Ctrl+C pauses then cancels the goal without exiting the CLI', async () => {
    testCase = await launchLiteInteg('goal-ctrlc-cancel');

    // Queue a goal-status event plus streamed content, then start the turn —
    // the queue drains into the stream, mirroring a backend goal iteration.
    await testCase.mockSessionUpdate({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 0,
      maxIterations: 5,
      message: 'test goal',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'goal-content-1',
      content: { type: ContentType.Text, text: 'working on the goal' },
    });
    await testCase.typeAndSubmit('start the goal');
    await testCase.sleepMs(300);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);
    expect(store.goalStatus?.state).toBe('active');

    // Ctrl+C #1: cancels the turn, goal drops to paused — not cleared.
    await testCase.pressCtrlC();
    await testCase.waitForStore((s) => !s.isProcessing, 5000);
    store = await testCase.getStore();
    expect(store.goalStatus?.state).toBe('paused');
    expect(store.exitSequence).toBe(0);

    // Ctrl+C #2: cancels the goal and returns a clean prompt — the exit
    // sequence must not arm while a goal was set.
    await testCase.pressCtrlC();
    await testCase.waitForStore((s) => s.goalStatus == null, 5000);
    store = await testCase.getStore();
    expect(store.exitSequence).toBe(0);

    // The process is still alive and exits normally now that no goal is set.
    await exitLiteInteg(testCase);
  }, 30000);
});
