import { describe, it, expect, mock } from 'bun:test';
import { createAppStore, MessageRole } from './app-store';
import { AgentEventType, ContentType } from '../types/agent-events';
import { Kiro } from '../kiro';

// Mock Kiro
mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

describe('AppStore input buffer', () => {
  it('backspace removes character at cursor', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    // Set up initial state with text
    store.getState().insert('h');
    store.getState().insert('i');

    // Verify initial state
    expect(store.getState().input.lines[0]).toBe('hi');
    expect(store.getState().input.cursorCol).toBe(2);

    // Test backspace
    store.getState().backspace();

    // Verify character was removed
    expect(store.getState().input.lines[0]).toBe('h');
    expect(store.getState().input.cursorCol).toBe(1);
  });

  it('delete removes character at cursor', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    store.getState().insert('h');
    store.getState().insert('i');
    // Position cursor at start
    const input = store.getState().input;
    store.setState({
      input: { ...input, cursorCol: 0, preferredCursorCol: 0 },
    });

    store.getState().delete();

    expect(store.getState().input.lines[0]).toBe('i');
    expect(store.getState().input.cursorCol).toBe(0);
  });

  it('delete merges with next line at end of line', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    store.getState().insert('a');
    store.getState().newline();
    store.getState().insert('b');
    // Position cursor at end of first line
    const input = store.getState().input;
    store.setState({
      input: { ...input, cursorRow: 0, cursorCol: 1, preferredCursorCol: 1 },
    });

    store.getState().delete();

    expect(store.getState().input.lines).toEqual(['ab']);
  });
});

describe('Streaming content flush', () => {
  function createStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    // Mark as initialized so sendMessage works
    store.setState({ isInitialized: true });
    return store;
  }

  it('flushContentToStore updates last model message in place without double spread', async () => {
    const store = createStore();

    // Seed a user message and a model message
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'hello' },
        { id: 'm1', role: MessageRole.Model, content: 'initial' },
      ],
    });

    const handler = store.getState().createStreamEventHandler();

    // Send a content event — this buffers the text
    handler!({
      type: AgentEventType.Content,
      id: 'm1',
      content: { type: ContentType.Text, text: 'updated response' },
    });

    // Manually trigger the batched flush by advancing the timer
    // The flush is scheduled via setTimeout(fn, 16), so we wait for it
    await new Promise((resolve) => setTimeout(resolve, 50));

    const messages = store.getState().messages;
    expect(messages).toHaveLength(2);
    expect(messages[1]?.content).toBe('updated response');
    // The user message should be the exact same object reference (not copied)
    expect(messages[0]?.id).toBe('u1');
  });

  it('flushContentToStore appends new model message when last is not model', async () => {
    const store = createStore();

    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hello' }],
    });

    const handler = store.getState().createStreamEventHandler();

    handler!({
      type: AgentEventType.Content,
      id: 'new-m1',
      content: { type: ContentType.Text, text: 'first chunk' },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    const messages = store.getState().messages;
    expect(messages).toHaveLength(2);
    expect(messages[1]?.role).toBe(MessageRole.Model);
    expect(messages[1]?.content).toBe('first chunk');
  });

  it('commitBufferedContent returns empty when no model message exists', () => {
    const store = createStore();

    // Only a user message — no model message to update
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hello' }],
    });

    // Create handler which installs the streamingBuffer
    store.getState().createStreamEventHandler();
    const { streamingBuffer } = store.getState();

    // Start buffering, add content, then stop (triggers commitBufferedContent)
    streamingBuffer?.startBuffering();

    // We need to set bufferedContent — simulate by sending a content event
    // while buffering is active (it won't schedule a flush, just buffers)
    const handler = store.getState().createStreamEventHandler();
    handler!({
      type: AgentEventType.Content,
      id: 'x',
      content: { type: ContentType.Text, text: 'buffered text' },
    });

    // stopBuffering calls commitBufferedContent
    store.getState().streamingBuffer?.stopBuffering();

    // Messages should not have been unnecessarily replaced
    const msgsAfter = store.getState().messages;
    expect(msgsAfter).toHaveLength(1);
  });
});
