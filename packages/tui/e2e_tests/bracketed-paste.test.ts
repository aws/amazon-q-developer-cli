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

    // While a turn is processing, Ctrl+C cancels instead of arming exit —
    // wait for idle so both presses count toward the exit sequence.
    await testCase.waitForIdle();
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

    await testCase.waitForIdle();
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

    await testCase.waitForIdle();
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

    await testCase.waitForIdle();
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('pasted shell-escaped file path from Finder is unescaped', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-file-path-unescape')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Got the path.' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Type a prefix so the pasted path isn't interpreted as a slash command
    await testCase.sendKeys('look at ');
    await testCase.sleepMs(50);

    // Simulate dragging a file from macOS Finder — the terminal sends
    // the path as a bracketed paste with shell-escaped spaces and parens.
    const shellEscapedPath =
      '/Users/name/my\\ folder/report\\ \\(1\\).pdf';
    await testCase.sendKeys(
      `${PASTE_START}${shellEscapedPath}${PASTE_END}`
    );
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');

    // The unescaped path should be displayed
    expect(screenText).toContain('my folder/report (1).pdf');
    expect(screenText).not.toContain('my\\ folder');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');

    await testCase.pressEnter();
    await testCase.waitForText('Got the path.', 10000);

    await testCase.waitForIdle();
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});

/** Bracketed-paste 12 lines (exceeds 10-line collapse threshold). */
const pasteCollapsible = (tc: E2ETestCase) => {
  const lines = Array.from({ length: 12 }, (_, i) => `line ${i + 1}`);
  return tc.sendKeys(`${PASTE_START}${lines.join('\n')}${PASTE_END}`);
};

describe('Paste Chip Expand', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('paste shows chip with ▸ and hint, Tab expands to inline text and clears hint', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-expand-tab')
      .launch();

    await testCase.waitForText('ask a question', 10000);

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);

    let snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('12 lines');
    expect(snapshot).toContain('▸');
    expect(snapshot).toContain('Press Tab to expand');
    expect(snapshot).not.toContain('line 1');

    await testCase.sendKeys('\t');
    await testCase.sleepMs(300);

    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('12 lines');
    expect(snapshot).not.toContain('Press Tab to expand');
    expect(snapshot).toContain('line 1');
  }, 30000);

  it('hint clears on keypress and reappears on second paste', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-expand-hint-lifecycle')
      .launch();

    await testCase.waitForText('ask a question', 10000);

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);
    let snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('Press Tab to expand');

    await testCase.sendKeys('x');
    await testCase.sleepMs(200);
    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('Press Tab to expand');

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);
    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('Press Tab to expand');
  }, 30000);

  it('typing after paste inserts after the chip', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-cursor-position')
      .launch();

    await testCase.waitForText('ask a question', 10000);

    await testCase.sendKeys('before');
    await testCase.sleepMs(100);

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);

    await testCase.sendKeys('after');
    await testCase.sleepMs(200);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('before');
    expect(snapshot).toContain('12 lines');
    expect(snapshot).toContain('after');
    expect(snapshot.indexOf('before')).toBeLessThan(snapshot.indexOf('12 lines'));
    expect(snapshot.indexOf('12 lines')).toBeLessThan(snapshot.indexOf('after'));
  }, 30000);

  it('undo after expand restores the chip', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-expand-undo')
      .launch();

    await testCase.waitForText('ask a question', 10000);

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);

    let snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('12 lines');

    await testCase.sendKeys('\t');
    await testCase.sleepMs(300);
    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('12 lines');
    expect(snapshot).toContain('line 1');

    await testCase.sendKeys('\x1f');
    await testCase.sleepMs(300);
    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('12 lines');
    expect(snapshot).not.toContain('line 1');
  }, 30000);

  it('pasting collapsible text while cursor is on a chip inserts second chip', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-onto-chip')
      .launch();

    await testCase.waitForText('ask a question', 10000);

    await pasteCollapsible(testCase);
    await testCase.sleepMs(300);

    let snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('12 lines');

    const lines2 = Array.from({ length: 15 }, (_, i) => `second ${i + 1}`);
    await testCase.sendKeys(
      `${PASTE_START}${lines2.join('\n')}${PASTE_END}`
    );
    await testCase.sleepMs(300);

    snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('12 lines');
    expect(snapshot).toContain('15 lines');
  }, 30000);
});
