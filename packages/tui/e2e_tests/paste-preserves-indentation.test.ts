/**
 * E2E tests for the paste-leading-whitespace fix in PromptInput.
 *
 * Regression: buildContent used to call `.replace(/  +/g, ' ').trim()`, which
 * collapsed every run of 2+ spaces into a single space and trimmed the whole
 * assembled prompt. That destroyed indentation in pasted code both in the
 * rendered user message and in what was sent to the agent. The fix drops the
 * regex and trim from buildContent and moves the empty-submit guard to the
 * Enter call sites.
 *
 * These tests drive the real TUI in a PTY, paste content with multi-space
 * runs via bracketed paste, submit, and then assert on the user message
 * recorded in the Zustand store — the post-buildContent string.
 *
 * Note: the end-to-end path (PTY -> TUI input buffer -> submit) can reshape
 * newline indentation in ways unrelated to this fix (e.g. terminal auto-
 * indent), so these tests deliberately assert on the two properties the
 * pre-fix code specifically broke:
 *   1. Runs of >= 2 consecutive spaces survive (no collapse).
 *   2. The submitted content is not empty / not trimmed.
 * Byte-exact round-trip is covered by the colocated unit tests in
 * PromptInput.test.ts.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

// Bracketed paste escape sequences
const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';

describe('Paste preserves indentation', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('preserves runs of consecutive spaces in pasted aligned content', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-ws-indent-align')
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

    // Aligned columns (think pasted log lines or table rows). The pre-fix
    // regex `.replace(/  +/g, ' ')` collapsed every run of 2+ spaces to a
    // single space, destroying alignment. Single-line paste avoids terminal
    // newline autoindent noise so we can assert byte-exact equality.
    const aligned = 'foo      bar     baz     qux';
    await testCase.sendKeys(`${PASTE_START}${aligned}${PASTE_END}`);
    await testCase.sleepMs(200);

    // Sanity: no escape fragments leaked into the buffer.
    const screenText = testCase.getSnapshot().join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');
    expect(screenText).not.toContain('200~');
    expect(screenText).not.toContain('201~');

    await testCase.pressEnter();
    await testCase.waitForText('Got it.', 10000);

    // The user message in the store is exactly what PromptInput.onSubmit
    // produced (i.e. the output of buildContent) and is what was sent
    // to the agent. Pre-fix, this would be `'foo bar baz qux'`.
    const store = await testCase.getStore();
    const userMsg = store.messages.find(
      (m) => m.role === 'user' && m.content.includes('foo')
    );
    expect(userMsg).toBeDefined();
    expect(userMsg!.content).toBe(aligned);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('preserves indentation in pasted multi-line code (consecutive spaces survive)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-ws-indent-code')
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

    // Classic indented snippet. Pre-fix, every run of `  +` spaces would
    // be collapsed to one, so `if (x) {\n      return y;\n    }` would
    // become `if (x) { return y; }` in the submitted content.
    const indentedCode = '    if (x) {\n      return y;\n    }';
    await testCase.sendKeys(`${PASTE_START}${indentedCode}${PASTE_END}`);
    await testCase.sleepMs(200);

    const screenText = testCase.getSnapshot().join('\n');
    expect(screenText).not.toContain('[200~');
    expect(screenText).not.toContain('[201~');

    await testCase.pressEnter();
    await testCase.waitForText('Got it.', 10000);

    const store = await testCase.getStore();
    const userMsg = store.messages.find(
      (m) => m.role === 'user' && m.content.includes('if (x)')
    );
    expect(userMsg).toBeDefined();

    // Pin the pre-fix regressions rather than exact bytes: the PTY /
    // terminal layer can re-shape newline-leading whitespace in ways
    // unrelated to this fix.
    //
    // 1. At least one run of >= 2 spaces survived (`.replace(/  +/g, ' ')`
    //    would have collapsed them all).
    expect(userMsg!.content).toMatch(/ {2,}/);
    // 2. The inner indented line still has its indent.
    expect(userMsg!.content).toMatch(/\n {2,}return y;/);
    // 3. Content was not collapsed to a single space-separated line.
    expect(userMsg!.content).not.toBe('if (x) { return y; }');
    // 4. Newlines are preserved.
    expect(userMsg!.content.split('\n').length).toBeGreaterThanOrEqual(3);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('does not submit a whitespace-only paste (empty-submit guard still holds)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-ws-empty')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // No pushSendMessageResponse: if the guard breaks and we submit, the
    // test fails by finding a whitespace-only user message in the store.
    const whitespaceOnly = '     ';
    await testCase.sendKeys(`${PASTE_START}${whitespaceOnly}${PASTE_END}`);
    await testCase.sleepMs(200);

    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // The Enter-handler guard is `if (content.trim())`. A whitespace-only
    // paste must not produce a submitted user message.
    const store = await testCase.getStore();
    const whitespaceOnlyUserMsg = store.messages.find(
      (m) =>
        m.role === 'user' && m.content.length > 0 && m.content.trim() === ''
    );
    expect(whitespaceOnlyUserMsg).toBeUndefined();

    // The input buffer still contains the pasted whitespace. First Ctrl+C
    // clears a non-empty input (readline convention) and resets the exit
    // sequence, so pressCtrlCTwice() alone doesn't exit. Send a third
    // Ctrl+C after clearing to drive the exit sequence to completion.
    await testCase.pressCtrlC();
    await testCase.sleepMs(50);
    await testCase.pressCtrlC();
    await testCase.sleepMs(50);
    await testCase.pressCtrlC();
    await testCase.expectExit();
  }, 30000);
});
