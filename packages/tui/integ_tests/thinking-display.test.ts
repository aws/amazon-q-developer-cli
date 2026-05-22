import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

/**
 * These tests exercise the `<ThinkingDisplay>` component (rendering, tail
 * truncation, ctrl+o expand/collapse). Rendering is gated by
 * `chat.showThinking` (default `true`), so each test explicitly sets it via
 * `withGlobalSettings({ 'chat.showThinking': true })` to be resilient
 * against future default changes.
 *
 * The setting itself is exercised by `e2e_tests/show-thinking-setting.test.ts`.
 */
describe('Thinking display', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('stores thinking text on model message', async () => {
    testCase = await TestCase.builder()
          .withGlobalSettings({ 'chat.showThinking': true })
          .withTestName('thinking-store')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // Queue thinking + content events before sending the prompt so they're
    // dispatched when MockSessionClient.prompt() processes the queue.
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-1',
      content: {
        type: ContentType.Text,
        text: 'Let me reason about this.',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-1',
      content: { type: ContentType.Text, text: 'Here is my answer.' },
    });

    // Send the user message — prompt() will drain the event queue
    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(300);

    // Verify the store has a model message with both content and thinking
    const store = await testCase.getStore();
    const modelMsg = store.messages.find((m) => m.role === 'model');
    expect(modelMsg).toBeDefined();
    expect(modelMsg!.content).toContain('Here is my answer.');
    expect((modelMsg as Record<string, unknown>).thinking).toBe(
      'Let me reason about this.'
    );

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('renders thinking text in terminal output', async () => {
    testCase = await TestCase.builder()
          .withGlobalSettings({ 'chat.showThinking': true })
          .withTestName('thinking-renders')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // Queue thinking + content
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-2',
      content: {
        type: ContentType.Text,
        text: 'Analyzing the request carefully.',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-2',
      content: { type: ContentType.Text, text: 'Done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    // The thinking text should be visible in the terminal output.
    const snapshot = testCase.getSnapshot();
    const hasThinking = snapshot.some((line) =>
      line.includes('Analyzing the request')
    );
    expect(hasThinking).toBe(true);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('long thinking shows tail and ctrl+o hint after the turn ends', async () => {
    testCase = await TestCase.builder()
          .withGlobalSettings({ 'chat.showThinking': true })
          .withTestName('thinking-tail-hint')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // 10-line thinking trace; only the last 4 should be visible when collapsed.
    const lines = Array.from(
      { length: 10 },
      (_, i) => `Reasoning step ${i + 1}.`
    );
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-tail',
      content: { type: ContentType.Text, text: lines.join('\n') },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-tail',
      content: { type: ContentType.Text, text: 'All done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    const snapshot = testCase.getSnapshot();
    const flatten = snapshot.join('\n');

    // Tail: the last 4 lines must be present.
    for (const tailLine of lines.slice(-4)) {
      expect(flatten).toContain(tailLine);
    }
    // Earlier lines must be hidden.
    for (const hiddenLine of lines.slice(0, -4)) {
      expect(flatten).not.toContain(hiddenLine);
    }
    // ctrl+o hint must be visible since the turn is no longer streaming.
    expect(flatten).toContain('ctrl+o to toggle');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('ctrl+o toggles between collapsed tail and expanded full thinking', async () => {
    testCase = await TestCase.builder()
          .withGlobalSettings({ 'chat.showThinking': true })
          .withTestName('thinking-ctrl-o-expand')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    const lines = Array.from(
      { length: 10 },
      (_, i) => `Reasoning step ${i + 1}.`
    );
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-expand',
      content: { type: ContentType.Text, text: lines.join('\n') },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-expand',
      content: { type: ContentType.Text, text: 'All done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    // Initial collapsed state: tail visible, earlier lines hidden, top
    // hint with the "ctrl+o to toggle" suffix.
    let flatten = testCase.getSnapshot().join('\n');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).toContain('Reasoning step 10.');
    expect(flatten).toContain('lines above');
    expect(flatten).toContain('ctrl+o to toggle');

    // Press ctrl+o (0x0f) to expand.
    await testCase.sendKeys([0x0f]);
    await testCase.sleepMs(200);

    // All 10 lines should now be visible.
    flatten = testCase.getSnapshot().join('\n');
    for (const line of lines) {
      expect(flatten).toContain(line);
    }
    // The "lines above" hint disappears when expanded — there's nothing
    // above the visible body anymore, so the count would be 0. (Different
    // from Read/Grep, whose head-truncation hint stays visible as a
    // "ctrl+o to collapse" affordance.)
    expect(flatten).not.toContain('lines above');

    // Press ctrl+o again to collapse — confirms ctrl+o is a toggle, not a
    // one-way expand.
    await testCase.sendKeys([0x0f]);
    await testCase.sleepMs(200);

    flatten = testCase.getSnapshot().join('\n');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).not.toContain('Reasoning step 6.');
    expect(flatten).toContain('Reasoning step 7.');
    expect(flatten).toContain('Reasoning step 10.');
    // Hint reappears on collapse.
    expect(flatten).toContain('lines above');
    expect(flatten).toContain('ctrl+o to toggle');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('preserves paragraph breaks and trims leading empty in the tail', async () => {
    testCase = await TestCase.builder()
          .withGlobalSettings({ 'chat.showThinking': true })
          .withTestName('thinking-paragraphed')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // Thinking with paragraph breaks. The slice(-4) lands on
    //   ["", "Para C.", "", "Para D."]
    // — leading empty must be trimmed; internal empty must be preserved.
    const text = [
      'Para A.',
      '',
      'Para B.',
      '',
      'Para C.',
      '',
      'Para D.',
    ].join('\n');
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-paragraph',
      content: { type: ContentType.Text, text },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-paragraph',
      content: { type: ContentType.Text, text: 'All done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    const snapshot = testCase.getSnapshot();
    const flatten = snapshot.join('\n');

    // Tail of the visible body — Para C and Para D must be present.
    expect(flatten).toContain('Para C.');
    expect(flatten).toContain('Para D.');
    // Earlier paragraphs must be hidden behind the "lines above" hint.
    expect(flatten).not.toContain('Para A.');
    expect(flatten).not.toContain('Para B.');
    // Top-hint shape: "above" makes direction explicit; "ctrl+o to toggle"
    // matches the wording the rest of the app uses.
    expect(flatten).toContain('lines above');
    expect(flatten).toContain('ctrl+o to toggle');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
