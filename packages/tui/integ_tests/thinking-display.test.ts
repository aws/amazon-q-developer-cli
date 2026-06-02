import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

/**
 * Exercises the redesigned `<ThinkingDisplay>`:
 *   - collapsed by default — only a header + "ctrl+o for details" hint, the
 *     reasoning body is hidden.
 *   - ctrl+o expands the full stream (shared with tool outputs) and collapses
 *     it again — ctrl+o is a toggle in collapsed mode.
 *   - once a turn is flushed to the static (history) buffer it shows just the
 *     "Thought for Ns" hint, frozen: no body, no ctrl+o affordance.
 *
 * The `chat.showThinking` mode (collapsed/expanded/off) is set explicitly via
 * `withGlobalSettings` so these are resilient to default changes. The setting
 * gate itself is covered by `e2e_tests/show-thinking-setting.test.ts`.
 */
describe('Thinking display', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  const REASONING = Array.from(
    { length: 6 },
    (_, i) => `Reasoning step ${i + 1}.`
  ).join('\n');

  it('stores thinking text on the model message', async () => {
    testCase = await TestCase.builder()
      .withGlobalSettings({ 'chat.showThinking': 'collapsed' })
      .withTestName('thinking-store')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-1',
      content: { type: ContentType.Text, text: 'Let me reason about this.' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-1',
      content: { type: ContentType.Text, text: 'Here is my answer.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(1000);

    const store = await testCase.getStore();
    const modelMsg = store.messages.find((m) => m.role === 'model');
    expect(modelMsg).toBeDefined();
    expect(modelMsg!.content).toContain('Here is my answer.');
    expect((modelMsg as Record<string, unknown>).thinking).toBe(
      'Let me reason about this.'
    );
    // Duration is captured once content follows the thought.
    expect(
      typeof (modelMsg as Record<string, unknown>).thinkingMs
    ).toBe('number');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('collapses by default: header + hint shown, reasoning body hidden', async () => {
    testCase = await TestCase.builder()
      .withGlobalSettings({ 'chat.showThinking': 'collapsed' })
      .withTestName('thinking-collapsed-default')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-collapsed',
      content: { type: ContentType.Text, text: REASONING },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-collapsed',
      content: { type: ContentType.Text, text: 'All done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    const flatten = testCase.getSnapshot().join('\n');
    // Completed header + collapse hint, but no reasoning body.
    expect(flatten).toContain('Thought for');
    expect(flatten).toContain('ctrl+o to view');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).not.toContain('Reasoning step 6.');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('ctrl+o expands the full stream, then collapses it again', async () => {
    testCase = await TestCase.builder()
      .withGlobalSettings({ 'chat.showThinking': 'collapsed' })
      .withTestName('thinking-ctrl-o-toggle')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-toggle',
      content: { type: ContentType.Text, text: REASONING },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-toggle',
      content: { type: ContentType.Text, text: 'All done.' },
    });

    await testCase.sendKeys('hello\r');
    await testCase.sleepMs(500);

    // Collapsed: body hidden.
    let flatten = testCase.getSnapshot().join('\n');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).toContain('ctrl+o to view');

    // ctrl+o (0x0f) → expanded: every line visible, hint flips to collapse.
    await testCase.sendKeys([0x0f]);
    await testCase.sleepMs(200);
    flatten = testCase.getSnapshot().join('\n');
    for (let i = 1; i <= 6; i++) {
      expect(flatten).toContain(`Reasoning step ${i}.`);
    }
    expect(flatten).toContain('ctrl+o to collapse details');

    // ctrl+o again → collapsed: body hidden, hint flips back.
    await testCase.sendKeys([0x0f]);
    await testCase.sleepMs(200);
    flatten = testCase.getSnapshot().join('\n');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).not.toContain('Reasoning step 6.');
    expect(flatten).toContain('ctrl+o to view');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('static (history) buffer shows only the "Thought for Ns" hint, frozen', async () => {
    testCase = await TestCase.builder()
      .withGlobalSettings({ 'chat.showThinking': 'collapsed' })
      .withTestName('thinking-static-hint')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // Turn 1: thinking + content.
    await testCase.mockSessionUpdate({
      type: AgentEventType.Thought,
      id: 'thought-static',
      content: { type: ContentType.Text, text: REASONING },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-static',
      content: { type: ContentType.Text, text: 'First answer.' },
    });
    await testCase.sendKeys('hello\r');
    await testCase.waitForVisibleText('First answer.');

    // A second prompt completes turn 1, flushing it to <Static>. We only need
    // the new user message to register — turn 1 becomes a completed turn as
    // soon as a newer user message exists.
    await testCase.sendKeys('again\r');
    await testCase.waitForVisibleText('again');
    await testCase.sleepMs(300);

    const flatten = testCase.getSnapshot().join('\n');
    // Historical thinking is just a frozen hint: no body, no ctrl+o affordance.
    expect(flatten).toContain('Thought for');
    expect(flatten).not.toContain('Reasoning step 1.');
    expect(flatten).not.toContain('Reasoning step 6.');
    expect(flatten).not.toContain('ctrl+o to view');
    expect(flatten).not.toContain('ctrl+o to collapse details');

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
