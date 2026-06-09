/**
 * E2E tests for the dual-mode interrupt behavior toggle (Ctrl+S).
 *
 * Mental model under test: "steer cuts the line, queue drains when idle."
 * - Steering mode routes input to the backend as a mid-turn steer (held until
 *   the next drain point). Multiple steers concatenate on the backend with
 *   "\n\n" into a single pending steer snapshot.
 * - Queueing mode buffers input locally as discrete FIFO `queuedMessages`.
 * - Toggling modes does NOT migrate pending messages — steer and queue coexist.
 * - At end-of-turn the pending steer replays first (as one message), then the
 *   queue drains one message at a time.
 *
 * Verifies:
 * 1. Ctrl+S toggles activeInterruptMode between 'steering' and 'queuing'
 * 2. Queueing mode buffers locally and sends after the turn ends
 * 3. The startup default is read from chat.defaultInterruptBehavior
 * 4. Steer-first / queue-FIFO drain ordering across mode toggles
 * 5. Clearing steer and queued messages individually via the activity tray
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { InterruptMode } from '../src/constants/interrupt-mode';

const CTRL_S = '\x13';
const CTRL_X = '\x18';

async function exitCleanly(tc: E2ETestCase) {
  await tc.sendKeys('\x01'); // Ctrl+A
  await tc.sleepMs(50);
  await tc.sendKeys('\x0b'); // Ctrl+K
  await tc.sleepMs(100);
  await tc.pressCtrlCTwice();
  await tc.expectExit();
}

/** Send a line of text followed by Enter. */
async function submit(tc: E2ETestCase, text: string) {
  await tc.sendKeys(text);
  await tc.sleepMs(80);
  await tc.pressEnter();
  await tc.sleepMs(200);
}

describe('Dual-mode queue toggle', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Ctrl+S toggles mode from steering to queuing and back', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-toggle')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Verify initial mode is steering
    let store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.STEER);

    // Press Ctrl+S to toggle to queueing
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.QUEUE);

    // Verify transient alert was shown
    await testCase.waitForText('Queue mode', 3000);

    // Press Ctrl+S again to toggle back to steering
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.STEER);

    await exitCleanly(testCase);
  }, 30000);

  it('queueing mode buffers messages locally and sends after turn ends', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-queue-buffer')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Switch to queueing mode
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);

    // Queue a response but keep the stream open (agent stays processing)
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Working on it...' },
          },
        },
      ],
      { silent: true }
    );

    // Send a prompt to start agent processing
    await submit(testCase, 'do something');

    // Wait for agent to start processing
    await testCase.waitForText('Thinking', 10000);

    // Type a follow-up while agent is processing — should be queued locally
    await submit(testCase, 'also do this');

    // Verify message was queued locally
    let store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['also do this']);

    // Activity tray should show the queued message
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Queue');

    // Close the stream — agent finishes the turn
    await testCase.pushSendMessageResponse(null);
    await testCase.sleepMs(500);

    // The queued message should have been auto-sent via processQueue
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Done with the second thing!' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Wait for the queued message response to appear
    await testCase.waitForText('Done with the second thing!', 15000);

    // Queue should be empty now
    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual([]);

    await exitCleanly(testCase);
  }, 45000);

  it('respects chat.defaultInterruptBehavior setting for startup default', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-setting-default')
      .withGlobalSettings({
        'chat.defaultInterruptBehavior': InterruptMode.QUEUE,
      })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Verify the session starts in queueing mode because the persisted
    // Verify the session starts in queue mode because the persisted
    // setting value 'queue' is the runtime mode token verbatim.
    const store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.QUEUE);

    await exitCleanly(testCase);
  }, 30000);

  it('Ctrl+S works while agent is processing without interrupting', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-toggle-during-processing')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Queue a response but keep stream open
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'still working' },
          },
        },
      ],
      { silent: true }
    );

    // Start a turn
    await submit(testCase, 'hello');
    await testCase.waitForText('Thinking', 10000);

    // Toggle mode while processing
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);

    // Should have toggled without interrupting the agent
    let store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.QUEUE);
    expect(store.isProcessing).toBe(true);

    // Toggle back
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);

    store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.STEER);
    expect(store.isProcessing).toBe(true);

    // Close stream to finish
    await testCase.pushSendMessageResponse(null);
    await testCase.waitForIdle();

    await exitCleanly(testCase);
  }, 30000);

  // --- Part 2: queue first, then toggle to steer; steer drains first, then
  // queued messages dequeue in sequence. ----------------------------------
  it('queued messages coexist with a later steer; steer drains first, then queue in order', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-steer-cuts-queue-line')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Switch to queueing mode and start a turn that stays open.
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Working...' },
          },
        },
      ],
      { silent: true }
    );
    await submit(testCase, 'start task');
    await testCase.waitForText('Thinking', 10000);

    // Queue two messages while the agent is busy.
    await submit(testCase, 'queued one');
    await submit(testCase, 'queued two');

    let store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['queued one', 'queued two']);

    // Toggle back to steering and submit a steer. It must NOT migrate the
    // queue — both coexist, and the steer jumps in front.
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);
    await submit(testCase, 'urgent steer');

    store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.STEER);
    // Queue is preserved intact (sent with queue intent earlier).
    expect(store.queuedMessages).toEqual(['queued one', 'queued two']);
    // The steer is held on the backend as the pending steer snapshot.
    expect(store.pendingSteerContent).toBe('urgent steer');

    // Pre-stage the three follow-up turns the drain will trigger, in the
    // order we expect them to run: steer first, then queue FIFO. The null
    // that ends the original (held-open) turn must come first so the mock's
    // FIFO lines up with the sequence of turns.
    await testCase.pushSendMessageResponse(null); // end the original turn
    for (const content of [
      'Reply to steer',
      'Reply to queued one',
      'Reply to queued two',
    ]) {
      await testCase.pushSendMessageResponse([
        {
          kind: 'event',
          data: { kind: 'AssistantResponseEvent', data: { content } },
        },
      ]);
      await testCase.pushSendMessageResponse(null);
    }

    // Steer drains first.
    await testCase.waitForText('Reply to steer', 15000);
    // Then queued messages dequeue one after the other, in order.
    await testCase.waitForText('Reply to queued one', 15000);
    await testCase.waitForText('Reply to queued two', 15000);

    // Wait for the drain to fully settle before inspecting final state.
    await testCase.waitForIdle();

    store = await testCase.getStore();
    expect(store.pendingSteerContent).toBeNull();
    expect(store.queuedMessages).toEqual([]);
    expect(store.isProcessing).toBe(false);

    // Verify the model replies landed in the conversation in the expected
    // order: steer reply first, then queued replies in FIFO order. Reading
    // from store.messages (not the visible snapshot) is scroll-proof.
    const modelContents = store.messages
      .filter((m) => m.role === 'model')
      .map((m) => m.content);
    const idxSteer = modelContents.findIndex((c) =>
      c.includes('Reply to steer')
    );
    const idxOne = modelContents.findIndex((c) =>
      c.includes('Reply to queued one')
    );
    const idxTwo = modelContents.findIndex((c) =>
      c.includes('Reply to queued two')
    );
    expect(idxSteer).toBeGreaterThanOrEqual(0);
    expect(idxSteer).toBeLessThan(idxOne);
    expect(idxOne).toBeLessThan(idxTwo);

    await exitCleanly(testCase);
  }, 60000);

  // --- Part 3: multiple steers, then toggle to queue and queue 2 messages;
  // all drain in order (steer snapshot first, then queue FIFO). ------------
  it('multiple steers then queued messages all drain in order', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-multi-steer-then-queue')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Start a turn in the default steering mode and keep it open.
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Processing...' },
          },
        },
      ],
      { silent: true }
    );
    await submit(testCase, 'hello');
    await testCase.waitForText('Thinking', 10000);

    // Submit two steers mid-turn. The backend concatenates them with "\n\n"
    // into a single pending steer snapshot.
    await submit(testCase, 'steer alpha');
    await submit(testCase, 'steer beta');

    let store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.STEER);
    expect(store.pendingSteerContent).toBe('steer alpha\n\nsteer beta');

    // Toggle to queueing and queue two messages. Steer stays intact.
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);
    await submit(testCase, 'queued one');
    await submit(testCase, 'queued two');

    store = await testCase.getStore();
    expect(store.activeInterruptMode).toBe(InterruptMode.QUEUE);
    expect(store.pendingSteerContent).toBe('steer alpha\n\nsteer beta');
    expect(store.queuedMessages).toEqual(['queued one', 'queued two']);

    // Pre-stage drain turns: combined steer first, then queue FIFO. The null
    // ending the original held-open turn comes first to align the mock FIFO.
    await testCase.pushSendMessageResponse(null); // end the original turn
    for (const content of [
      'Reply to steers',
      'Reply to queued one',
      'Reply to queued two',
    ]) {
      await testCase.pushSendMessageResponse([
        {
          kind: 'event',
          data: { kind: 'AssistantResponseEvent', data: { content } },
        },
      ]);
      await testCase.pushSendMessageResponse(null);
    }

    await testCase.waitForText('Reply to steers', 15000);
    await testCase.waitForText('Reply to queued one', 15000);
    await testCase.waitForText('Reply to queued two', 15000);

    await testCase.waitForIdle();

    store = await testCase.getStore();
    expect(store.pendingSteerContent).toBeNull();
    expect(store.queuedMessages).toEqual([]);
    expect(store.isProcessing).toBe(false);

    const modelContents = store.messages
      .filter((m) => m.role === 'model')
      .map((m) => m.content);
    const idxSteer = modelContents.findIndex((c) =>
      c.includes('Reply to steers')
    );
    const idxOne = modelContents.findIndex((c) =>
      c.includes('Reply to queued one')
    );
    const idxTwo = modelContents.findIndex((c) =>
      c.includes('Reply to queued two')
    );
    expect(idxSteer).toBeGreaterThanOrEqual(0);
    expect(idxSteer).toBeLessThan(idxOne);
    expect(idxOne).toBeLessThan(idxTwo);

    await exitCleanly(testCase);
  }, 60000);

  // --- Part 4a: clear a pending steer message via the activity tray. ------
  it('clears a pending steer message via the activity tray', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-clear-steer')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Start a turn (steering mode) and keep it open.
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Working...' },
          },
        },
      ],
      { silent: true }
    );
    await submit(testCase, 'start');
    await testCase.waitForText('Thinking', 10000);

    // Submit a steer mid-turn.
    await submit(testCase, 'steer to clear');
    let store = await testCase.getStore();
    expect(store.pendingSteerContent).toBe('steer to clear');
    expect(store.queuedMessages).toEqual([]);

    // Open the activity tray and press del — with no queued messages, del
    // clears the pending steer.
    await testCase.sendKeys(CTRL_X);
    await testCase.sleepMs(200);
    await testCase.sendKeys([0x7f]); // delete/backspace
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.pendingSteerContent).toBeNull();
    expect(store.queuedMessages).toEqual([]);

    // Close the stream to end the turn cleanly.
    await testCase.pushSendMessageResponse(null);
    await testCase.waitForIdle();

    await exitCleanly(testCase);
  }, 45000);

  // --- Part 4b: clear queued messages via the activity tray. --------------
  it('clears queued messages one at a time via the activity tray', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-clear-queue')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Queueing mode, start a turn that stays open.
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Working...' },
          },
        },
      ],
      { silent: true }
    );
    await submit(testCase, 'start');
    await testCase.waitForText('Thinking', 10000);

    // Queue two messages.
    await submit(testCase, 'queued one');
    await submit(testCase, 'queued two');
    let store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['queued one', 'queued two']);

    // Open the tray; del removes the selected (first) queued message.
    await testCase.sendKeys(CTRL_X);
    await testCase.sleepMs(200);
    await testCase.sendKeys([0x7f]);
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['queued two']);

    // Remove the remaining one.
    await testCase.sendKeys([0x7f]);
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual([]);

    await testCase.pushSendMessageResponse(null);
    await testCase.waitForIdle();

    await exitCleanly(testCase);
  }, 45000);

  // --- Part 4c: submit both a steer and queued messages, then clear them
  // in order, verifying the correct message is removed after each clear. ---
  it('clears queued messages then the steer, verifying order after each clear', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('dual-mode-clear-both')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Start a turn (steering) and keep it open.
    await testCase.pushSendMessageResponse(
      [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Working...' },
          },
        },
      ],
      { silent: true }
    );
    await submit(testCase, 'start');
    await testCase.waitForText('Thinking', 10000);

    // Submit a steer, then toggle to queue and add two queued messages.
    await submit(testCase, 'the steer');
    await testCase.sendKeys(CTRL_S);
    await testCase.sleepMs(200);
    await submit(testCase, 'queued one');
    await submit(testCase, 'queued two');

    let store = await testCase.getStore();
    expect(store.pendingSteerContent).toBe('the steer');
    expect(store.queuedMessages).toEqual(['queued one', 'queued two']);

    // Open the tray. del removes queued messages first (queue is shown
    // before the steer is targeted), one at a time, in FIFO order.
    await testCase.sendKeys(CTRL_X);
    await testCase.sleepMs(200);

    // First clear removes 'queued one'; steer untouched.
    await testCase.sendKeys([0x7f]);
    await testCase.sleepMs(300);
    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual(['queued two']);
    expect(store.pendingSteerContent).toBe('the steer');

    // Second clear removes 'queued two'; steer still untouched.
    await testCase.sendKeys([0x7f]);
    await testCase.sleepMs(300);
    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual([]);
    expect(store.pendingSteerContent).toBe('the steer');

    // Third clear, with the queue empty, clears the pending steer.
    await testCase.sendKeys([0x7f]);
    await testCase.sleepMs(300);
    store = await testCase.getStore();
    expect(store.queuedMessages).toEqual([]);
    expect(store.pendingSteerContent).toBeNull();

    await testCase.pushSendMessageResponse(null);
    await testCase.waitForIdle();

    await exitCleanly(testCase);
  }, 60000);
});
