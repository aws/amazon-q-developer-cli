import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

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

    // The thinking summary (💭) should be visible in the terminal
    const snapshot = testCase.getSnapshot();
    const hasThinking = snapshot.some((line) =>
      line.includes('Analyzing the request')
    );
    expect(hasThinking).toBe(true);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
