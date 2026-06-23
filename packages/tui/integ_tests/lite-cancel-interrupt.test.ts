import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import {
  exitLiteInteg,
  finishAndExitLite,
  launchLiteInteg,
  startBusyTurn,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Bug-mine 5.1-5.7: cancel/interrupt invariants in lite mode. Non-obvious bits:
 * cancel must be idempotent (guard against double-fire) and must dispose the
 * stream handler so a late event can't re-open a cancelled turn.
 */
describe('lite cancel/interrupt invariants [bug-mine 5.1-5.7]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // Shared shape: start a busy turn, fire the cancel key(s), assert the turn
  // cleared, then run the case-specific post-cancel assertions. They differ
  // only in the cancel key and the invariant checked afterward.
  it.each([
    {
      bug: '5.1',
      testName: 'lite-cancel-no-duplicate',
      busy: { marker: 'PARTIAL_RESPONSE_ABC' },
      cancel: async (tc: TestCase) => {
        await tc.pressCtrlC();
        await tc.sleepMs(500);
      },
      afterCancel: (store: Awaited<ReturnType<TestCase['getStore']>>) => {
        // Partial content must not be duplicated by a stale flush after cancel.
        const dup = store.messages.filter(
          (m) =>
            m.role === 'model' && m.content?.includes('PARTIAL_RESPONSE_ABC')
        );
        expect(dup.length).toBeLessThanOrEqual(1);
      },
    },
    {
      bug: '5.2',
      testName: 'lite-cancel-idempotent',
      busy: { marker: 'some content', prompt: 'test double cancel' },
      cancel: async (tc: TestCase) => {
        // The second Ctrl+C must be a no-op due to the cancelInProgress guard.
        await tc.pressCtrlC();
        await tc.sleepMs(50);
        await tc.pressCtrlC();
        await tc.sleepMs(500);
      },
      afterCancel: (store: Awaited<ReturnType<TestCase['getStore']>>) => {
        expect(store.cancelInProgress).toBeNull();
        expect(store.agentError).toBeNull();
      },
    },
  ])(
    'busy turn cancels cleanly [bug-mine $bug]',
    async ({ testName, busy, cancel, afterCancel }) => {
      testCase = await launchLiteInteg(testName);
      await startBusyTurn(testCase, busy);
      let store = await testCase.getStore();
      expect(store.isProcessing).toBe(true);

      await cancel(testCase);

      store = await testCase.getStore();
      expect(store.isProcessing).toBe(false);
      afterCancel(store);

      await testCase.sendKeys([0x03, 0x03]);
      await testCase.expectExit();
    },
    30000
  );

  it('cancel disposes stream handler before async cancel — no ghost content from old turn [bug-mine 5.3]', async () => {
    testCase = await launchLiteInteg('lite-cancel-no-ghost');

    await startBusyTurn(testCase, {
      marker: 'TURN1_UNIQUE_MARKER',
      id: 'content-turn1',
      prompt: 'turn one',
    });

    await testCase.pressCtrlC();
    await testCase.sleepMs(500);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-turn2',
      content: { type: ContentType.Text, text: 'TURN2_UNIQUE_MARKER' },
    });

    await testCase.typeAndSubmit('turn two');
    await testCase.sleepMs(300);

    await testCase.completeTurn();
    await testCase.sleepMs(200);

    // Turn 2's model message must not bleed turn 1's cancelled content.
    store = await testCase.getStore();
    const modelMessages = store.messages.filter((m) => m.role === 'model');
    const lastModel = modelMessages[modelMessages.length - 1];
    expect(lastModel).toBeDefined();
    expect(lastModel!.content).toContain('TURN2_UNIQUE_MARKER');
    expect(lastModel!.content).not.toContain('TURN1_UNIQUE_MARKER');

    await exitLiteInteg(testCase);
  }, 30000);

  it('Ctrl+C when idle increments exitSequence but does not crash [bug-mine 5.4]', async () => {
    testCase = await launchLiteInteg('lite-cancel-idle-exit-seq');

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);
    expect(store.exitSequence).toBe(0);

    await testCase.pressCtrlC();
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.exitSequence).toBe(1);

    // exitSequence resets to 0 after the 2s exit timer elapses.
    await testCase.sleepMs(2200);

    store = await testCase.getStore();
    expect(store.exitSequence).toBe(0);

    const stillAlive = await testCase.getStore();
    expect(stillAlive).toBeDefined();

    await testCase.sendKeys([0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('Esc during subagent panel open does not cancel agent turn [bug-mine 5.5]', async () => {
    testCase = await launchLiteInteg('lite-cancel-esc-panel');

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'subagent-parent-p',
      name: 'subagent',
      args: { pipeline: 'test-pipeline' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-stage-1',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/file.txt' },
      sessionId: 'session-stage-1',
    });

    await testCase.typeAndSubmit('start turn');
    await testCase.sleepMs(300);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    await testCase.sendKeys('\x0f'); // Ctrl+O opens the subagent panel
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(true);
    expect(store.isProcessing).toBe(true);

    // Esc closes the panel; the invariant is it must NOT cancel the turn.
    await testCase.pressEscape();
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.subagentPanelOpen).toBe(false);
    expect(store.isProcessing).toBe(true);

    await finishAndExitLite(testCase);
  }, 30000);

  it('Esc cancels turn cleanly and app recovers for new input [bug-mine 5.6]', async () => {
    testCase = await launchLiteInteg('lite-cancel-esc-recovers');

    await startBusyTurn(testCase, {
      marker: 'background work',
      id: 'content-bg',
      prompt: 'first message',
    });
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    await testCase.pressEscape();
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    expect(store.isProcessing).toBe(false);
    expect(store.cancelInProgress).toBeNull();

    // A new turn after cancel must not desync ("Prompt already in progress").
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-recovery',
      content: { type: ContentType.Text, text: 'RECOVERY_CONTENT' },
    });
    await testCase.typeAndSubmit('recovery message');
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.agentError).toBeNull();
    const userMessages = store.messages.filter((m) => m.role === 'user');
    const recoveryMsg = userMessages.find((m) =>
      m.content?.includes('recovery message')
    );
    expect(recoveryMsg).toBeDefined();

    if (store.isProcessing) {
      await testCase.completeTurn();
      await testCase.sleepMs(100);
    }
    await exitLiteInteg(testCase);
  }, 30000);

  it('cancel drains queued message immediately after clearing isProcessing [bug-mine 5.7]', async () => {
    testCase = await launchLiteInteg('lite-cancel-drain-queue');

    await startBusyTurn(testCase, {
      marker: 'turn 1 content',
      id: 'content-t1',
      prompt: 'first message',
    });
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    await testCase.typeAndSubmit('queued follow up');
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.queuedMessages.length).toBeGreaterThanOrEqual(1);
    expect(store.queuedMessages[0]).toBe('queued follow up');

    await testCase.pressCtrlC();
    await testCase.sleepMs(800);

    // Cancel must drain the queue: either a new turn is processing it or it
    // was already submitted — either way the queue is empty afterward.
    store = await testCase.getStore();
    expect(store.queuedMessages.length).toBe(0);

    const userMessages = store.messages.filter((m) => m.role === 'user');
    const queuedMsg = userMessages.find((m) =>
      m.content?.includes('queued follow up')
    );
    expect(queuedMsg).toBeDefined();

    if (store.isProcessing) {
      await testCase.completeTurn();
      await testCase.sleepMs(100);
    }
    await exitLiteInteg(testCase);
  }, 30000);
});
