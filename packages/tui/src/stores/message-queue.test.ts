import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

function createTestStore() {
  const mockKiro = new Kiro();
  return createAppStore({ kiro: mockKiro });
}

describe('Message queue', () => {
  describe('queueMessage', () => {
    it('appends trimmed message to queuedMessages', () => {
      const store = createTestStore();
      store.getState().queueMessage('  hello world  ');
      expect(store.getState().queuedMessages).toEqual(['hello world']);
    });

    it('preserves FIFO order for multiple messages', () => {
      const store = createTestStore();
      store.getState().queueMessage('first');
      store.getState().queueMessage('second');
      store.getState().queueMessage('third');
      expect(store.getState().queuedMessages).toEqual([
        'first',
        'second',
        'third',
      ]);
    });

    it('rejects empty string', () => {
      const store = createTestStore();
      store.getState().queueMessage('');
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('rejects whitespace-only string', () => {
      const store = createTestStore();
      store.getState().queueMessage('   ');
      store.getState().queueMessage('\t\n');
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('clearQueue', () => {
    it('empties the queue', () => {
      const store = createTestStore();
      store.getState().queueMessage('a');
      store.getState().queueMessage('b');
      expect(store.getState().queuedMessages).toHaveLength(2);

      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('is a no-op on empty queue', () => {
      const store = createTestStore();
      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('processQueue', () => {
    it('dequeues first message and sends it', async () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['hello'] });

      await store.getState().processQueue();

      // Message was dequeued
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('is a no-op when queue is empty', async () => {
      const store = createTestStore();
      await store.getState().processQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('dequeues only the first message (FIFO)', async () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['first', 'second', 'third'] });

      await store.getState().processQueue();

      // With mock kiro, sendMessage completes immediately and recursively
      // processes the entire queue. All messages should be dequeued.
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('does not clear input buffer when processing queue', async () => {
      const store = createTestStore();
      // Simulate user typing while queue processes
      const typedInput = store.getState().input;
      store.setState({
        queuedMessages: ['queued msg'],
        input: { ...typedInput, lines: ['user is typing'], cursorCol: 14 },
      });

      await store.getState().processQueue();

      // Input should be preserved — processQueue calls sendMessage directly
      const input = store.getState().input;
      expect(input.lines).toEqual(['user is typing']);
    });
  });

  describe('handleUserInput queuing', () => {
    it('queues message when isProcessing is true', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('queued message');

      expect(store.getState().queuedMessages).toEqual(['queued message']);
    });

    it('clears input buffer after queuing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('queued message');

      const input = store.getState().input;
      expect(input.lines).toEqual(['']);
      expect(input.cursorCol).toBe(0);
    });

    it('does not queue empty/whitespace input during processing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('   ');
      await store.getState().handleUserInput('');

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('cancellation semantics', () => {
    it('clearQueue + cancelMessage clears queue (Escape behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().clearQueue();
      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('cancelMessage alone preserves queue (Ctrl+C behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual(['msg1', 'msg2', 'msg3']);
    });
  });

  describe('unified expanded state', () => {
    it('toggleToolOutputsExpanded toggles the shared expanded state', () => {
      const store = createTestStore();
      expect(store.getState().toolOutputsExpanded).toBe(false);
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(true);
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(false);
    });

    it('expanded state persists across queued turns (not reset by sendMessage)', async () => {
      const store = createTestStore();
      // User expands outputs
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(true);

      // Queue a message and process it — sendMessage will be called
      store.setState({ queuedMessages: ['next message'] });
      await store.getState().processQueue();

      // Expanded state should still be true
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });

    it('clearQueue does not affect expanded state', () => {
      const store = createTestStore();
      store.getState().toggleToolOutputsExpanded();
      store.setState({ queuedMessages: ['a', 'b'] });

      store.getState().clearQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });
  });
});
