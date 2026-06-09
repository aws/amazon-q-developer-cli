/**
 * E2E tests for /rewind slash command (session fork).
 *
 * /rewind shows a picker listing previous user prompts. Selecting one
 * clones the session log up through that turn into a new session and
 * the TUI auto-loads that new session.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Rewind Command', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('shows picker with prompt previews when there are turns', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('rewind-shows-picker')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    // Send a few turns so there's something to rewind to.
    const prompts = ['First unique prompt', 'Second different ask', 'Third final query'];
    for (const p of prompts) {
      await testCase.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'ok' } } },
      ]);
      await testCase.pushSendMessageResponse(null);
      for (const c of p) {
        await testCase.sendKeys(c);
      }
      await testCase.pressEnter();
      await testCase.sleepMs(800);
    }

    // Invoke /rewind — should open the picker via options.
    for (const c of '/rewind') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(1500);

    // All three prompt previews should appear in the picker.
    try {
      await testCase.waitForText('Third final query', 10000);
      await testCase.waitForText('Second different ask', 2000);
      await testCase.waitForText('First unique prompt', 2000);
    } catch (e) {
      console.log('FAILED snapshot:\n' + testCase.getSnapshotFormatted());
      throw e;
    }

    // Ordering: newest first. Scope to the picker area (after the "Fork from" header)
    // so we don't match the chat-history copies of the same prompts above.
    const snapshot = testCase.getSnapshotFormatted();
    const pickerStart = snapshot.indexOf('Fork from a previous prompt');
    expect(pickerStart).toBeGreaterThanOrEqual(0);
    const pickerSection = snapshot.slice(pickerStart);
    const thirdIdx = pickerSection.indexOf('Third final query');
    const firstIdx = pickerSection.indexOf('First unique prompt');
    expect(thirdIdx).toBeGreaterThanOrEqual(0);
    expect(firstIdx).toBeGreaterThanOrEqual(0);
    expect(thirdIdx).toBeLessThan(firstIdx);
  }, 60000);

  it('shows preview footer with colored role markers when hovering', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 140, height: 40 })
      .withTestName('rewind-preview-colors')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    // One turn with a distinctive response so we can spot the preview.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'The answer is 42.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    for (const c of 'What is the meaning of life?') {
      await testCase.sendKeys(c);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(1500);

    // Open the /rewind picker.
    for (const c of '/rewind') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(2000);

    // Picker should be open with one row highlighted → preview footer visible.
    const snapshot = testCase.getSnapshotFormatted();
    console.log('PREVIEW snapshot:\n' + snapshot);

    // Explorer shows "Response Snippet" heading and the assistant's reply.
    // The prompt text is the hovered row label (shown in the list).
    expect(snapshot).toContain('Turn Activity');
    expect(snapshot).toContain('The answer is 42.');
    expect(snapshot).toContain('What is the meaning of life?');
  }, 60000);

  it('shows an empty result when the session has no turns', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('rewind-empty-session')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    for (const c of '/rewind') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(1500);

    // With no turns, the picker should either be empty or show a notice.
    // We assert that no numbered turn rows appear.
    const snapshot = testCase.getSnapshotFormatted();
    expect(snapshot).not.toContain('[1]');
  }, 30000);

  it('selecting a turn forks the session', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('rewind-forks-session')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    const originalSessionId = await testCase.getSessionId();

    // Send 2 turns.
    for (const p of ['Turn alpha', 'Turn beta']) {
      await testCase.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'reply to ' + p } } },
      ]);
      await testCase.pushSendMessageResponse(null);
      for (const c of p) await testCase.sendKeys(c);
      await testCase.pressEnter();
      await testCase.sleepMs(800);
    }

    // Open /rewind picker.
    for (const c of '/rewind') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(1500);

    await testCase.waitForText('Turn beta', 10000);
    await testCase.waitForText('Turn alpha', 2000);

    // Press Enter on the default hovered row ([1], most recent = Turn beta).
    await testCase.pressEnter();

    // Poll for session switch: the rewindAction effect queues a /rewind <idx>
    // message, which the queue processes asynchronously. Give it up to 10s.
    let newSessionId = originalSessionId;
    const start = Date.now();
    while (Date.now() - start < 10000) {
      newSessionId = await testCase.getSessionId();
      if (newSessionId && newSessionId !== originalSessionId) break;
      await testCase.sleepMs(250);
    }

    expect(newSessionId).not.toBe(originalSessionId);
    expect(newSessionId).toBeTruthy();
  }, 90000);
});
