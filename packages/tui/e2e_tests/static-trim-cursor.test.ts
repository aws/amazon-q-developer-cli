/**
 * E2E test: static item trimming must not break new message rendering.
 *
 * Reproduces a bug where trimStaticItems splices items from the front of the
 * Static array, causing twinki's monotonic totalStaticWritten cursor to be
 * ahead of the new array length.  New items appended after trimming are
 * silently skipped by renderTree (slice(cursor) on a shorter array = empty).
 *
 * Symptom: after a long conversation, new assistant responses stop appearing
 * in scrollback and the dynamic tail keeps updating instead.  Resizing the
 * terminal fixes it (resetStatic sets cursor to 0).
 *
 * We set KIRO_MAX_STATIC_ITEMS=10 so trimming triggers quickly, then verify
 * that a response sent *after* trimming is still visible on screen.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/** Push a simple turn: user types `text`, agent responds with `response`. */
async function pushSimpleTurn(
  tc: E2ETestCase,
  userText: string,
  responseText: string
) {
  await tc.pushSendMessageResponse(
    [
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: responseText },
        },
      },
    ],
    { silent: true }
  );
  await tc.pushSendMessageResponse(null, { silent: true });

  await tc.sendKeys(userText);
  await tc.sleepMs(50);
  await tc.pressEnter();
  await tc.waitForIdle(10000);
}

describe('Static trim cursor desync', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('new messages render after static items are trimmed', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('static-trim-cursor')
      .withTerminal({ width: 100, height: 50 })
      // Low cap so trimming triggers after ~12 static items (cap * 1.1)
      .withEnv({ KIRO_MAX_STATIC_ITEMS: '10' })
      .withGlobalSettings({ 'chat.greeting.enabled': false })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push 12 turns to trigger trimming (cap=10, hysteresis=1.1x → fires at 12).
    for (let i = 1; i <= 12; i++) {
      await pushSimpleTurn(testCase, `msg${i}`, `reply${i}`);
    }

    // Trimming has fired: 12 items → splice 2 → array is 10, cursor stuck at 12.
    // The next turn's static item (index 11) will be skipped by slice(12).
    // Push the canary turn — this is the one that should be swallowed by the bug.
    await pushSimpleTurn(testCase, 'after-trim', 'CANARY13');

    // Push one more to complete the canary turn (move it to static)
    await pushSimpleTurn(testCase, 'flush-turn', 'FLUSH14');

    await testCase.sleepMs(500);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Verify store has all messages
    const store = await testCase.getStore();
    const modelMsgs = store.messages.filter((m) => m.role === 'model');
    expect(modelMsgs.length).toBe(14);

    // The critical assertion: CANARY13 must be visible on screen.
    // With the bug, it's swallowed because twinki's cursor (12) > array length
    // after trim, so slice(12) on a 10-11 length array returns nothing.
    // The response exists in the store but was never written to terminal output.
    const allText = snapshot.join('\n');
    expect(allText).toContain('CANARY13');
  }, 60000);

  it('new messages render after resize following trim', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('static-trim-resize')
      .withTerminal({ width: 100, height: 50 })
      .withEnv({ KIRO_MAX_STATIC_ITEMS: '10' })
      .withGlobalSettings({ 'chat.greeting.enabled': false })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push enough turns to trigger trimming
    for (let i = 1; i <= 15; i++) {
      await pushSimpleTurn(testCase, `msg${i}`, `reply${i}`);
    }

    // Simulate resize by sending SIGWINCH (resets twinki's static cursor)
    const pid = testCase.getPid();
    if (pid) process.kill(pid, 'SIGWINCH');
    await testCase.sleepMs(500);

    // Push more turns after resize — these must still render
    await pushSimpleTurn(testCase, 'post-resize', 'AFTER-RESIZE-OK');
    await pushSimpleTurn(testCase, 'final', 'FINAL-OK');

    await testCase.sleepMs(300);

    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    const allText = snapshot.join('\n');
    expect(allText).toContain('AFTER-RESIZE-OK');
    expect(allText).toContain('FINAL-OK');
  }, 60000);
});
