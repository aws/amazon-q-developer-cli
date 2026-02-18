/**
 * E2E tests for bracketed paste handling.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

// Bracketed paste escape sequences
const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';

describe('Bracketed Paste', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('single-chunk paste does not leak [201~ into input', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-single-chunk')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push a mock response so the message can be submitted
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Got it.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Send a bracketed paste as a single chunk (most common case)
    const pastedText = 'hello world';
    await testCase.sendKeys(`${PASTE_START}${pastedText}${PASTE_END}`);
    await testCase.sleepMs(200);

    // Check the terminal screen does NOT contain the leaked sequence
    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');

    // The pasted text should be visible in the input area
    expect(screenText).toContain('hello world');

    // Submit and verify the response comes back clean
    await testCase.pressEnter();
    await testCase.waitForText('Got it.', 10000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('split-chunk paste does not leak [201~ into input', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-split-chunk')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Got it.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Simulate Ghostty-style split: paste start + content in one chunk,
    // paste end in a separate chunk. This is the scenario that triggers
    // the bug — the second chunk containing \x1b[201~ arrives as a
    // separate stdin.read(), and Ink's parseKeypress can strip the \x1b
    // and pass "[201~" as printable input.
    const pastedText = 'split paste test';
    await testCase.sendKeys(`${PASTE_START}${pastedText}`);
    await testCase.sleepMs(10);
    await testCase.sendKeys(PASTE_END);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');
    expect(screenText).toContain('split paste test');

    await testCase.pressEnter();
    await testCase.waitForText('Got it.', 10000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('paste with content split across three chunks', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-three-chunks')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Got it.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Three separate chunks: start, content, end
    await testCase.sendKeys(PASTE_START);
    await testCase.sleepMs(10);
    await testCase.sendKeys('three chunk test');
    await testCase.sleepMs(10);
    await testCase.sendKeys(PASTE_END);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');
    expect(screenText).toContain('three chunk test');

    await testCase.pressEnter();
    await testCase.waitForText('Got it.', 10000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('multiple pastes in sequence do not leak escape fragments', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-multiple-sequential')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Got both.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // First paste (single chunk)
    await testCase.sendKeys(`${PASTE_START}first${PASTE_END}`);
    await testCase.sleepMs(100);

    // Type a space between pastes
    await testCase.sendKeys(' ');
    await testCase.sleepMs(50);

    // Second paste (split chunks)
    await testCase.sendKeys(`${PASTE_START}second`);
    await testCase.sleepMs(10);
    await testCase.sendKeys(PASTE_END);
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');
    expect(screenText).toContain('first');
    expect(screenText).toContain('second');

    await testCase.pressEnter();
    await testCase.waitForText('Got both.', 10000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});
