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

describe('Queue editing', () => {
  describe('startEditingQueue', () => {
    it('sets editingQueueIndex and loads message into commandInputValue', () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['fix the bug', 'add tests'] });

      store.getState().startEditingQueue(1);

      expect(store.getState().editingQueueIndex).toBe(1);
      expect(store.getState().commandInputValue).toBe('add tests');
    });

    it('is a no-op for out-of-bounds index', () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['only one'] });

      store.getState().startEditingQueue(5);

      expect(store.getState().editingQueueIndex).toBeNull();
    });

    it('is a no-op for empty queue', () => {
      const store = createTestStore();

      store.getState().startEditingQueue(0);

      expect(store.getState().editingQueueIndex).toBeNull();
    });
  });

  describe('cancelEditingQueue', () => {
    it('clears editingQueueIndex and commandInputValue', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['hello'],
        editingQueueIndex: 0,
        commandInputValue: 'hello modified',
      });

      store.getState().cancelEditingQueue();

      expect(store.getState().editingQueueIndex).toBeNull();
      expect(store.getState().commandInputValue).toBe('');
    });

    it('preserves the original queued message', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['original message'],
        editingQueueIndex: 0,
        commandInputValue: 'modified text',
      });

      store.getState().cancelEditingQueue();

      expect(store.getState().queuedMessages).toEqual(['original message']);
    });
  });

  describe('replaceQueuedMessage', () => {
    it('replaces message at index and clears editing state', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['old message', 'keep this'],
        editingQueueIndex: 0,
      });

      store.getState().replaceQueuedMessage(0, 'new message');

      expect(store.getState().queuedMessages).toEqual([
        'new message',
        'keep this',
      ]);
      expect(store.getState().editingQueueIndex).toBeNull();
    });

    it('handles out-of-bounds index gracefully', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['only one'],
        editingQueueIndex: 5,
      });

      store.getState().replaceQueuedMessage(5, 'nope');

      // Should clear editing state without modifying queue
      expect(store.getState().queuedMessages).toEqual(['only one']);
      expect(store.getState().editingQueueIndex).toBeNull();
    });

    it('handles negative index gracefully', () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['a'], editingQueueIndex: 0 });

      store.getState().replaceQueuedMessage(-1, 'nope');

      expect(store.getState().queuedMessages).toEqual(['a']);
      expect(store.getState().editingQueueIndex).toBeNull();
    });
  });

  describe('removeQueuedMessage', () => {
    it('removes the item at the given index', () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['a', 'b', 'c'] });

      store.getState().removeQueuedMessage(1);

      expect(store.getState().queuedMessages).toEqual(['a', 'c']);
    });

    it('clears editing state when the edited item is removed', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a', 'b', 'c'],
        editingQueueIndex: 1,
        commandInputValue: 'b',
      });

      store.getState().removeQueuedMessage(1);

      expect(store.getState().editingQueueIndex).toBeNull();
      expect(store.getState().commandInputValue).toBe('');
    });

    it('shifts editing index down when an earlier item is removed', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a', 'b', 'c'],
        editingQueueIndex: 2,
        commandInputValue: 'c',
      });

      store.getState().removeQueuedMessage(0);

      expect(store.getState().queuedMessages).toEqual(['b', 'c']);
      expect(store.getState().editingQueueIndex).toBe(1);
      // commandInputValue preserved since we're still editing
      expect(store.getState().commandInputValue).toBe('c');
    });

    it('does not shift editing index when a later item is removed', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a', 'b', 'c'],
        editingQueueIndex: 0,
        commandInputValue: 'a',
      });

      store.getState().removeQueuedMessage(2);

      expect(store.getState().editingQueueIndex).toBe(0);
    });

    it('does not affect editing state when not editing', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a', 'b'],
        editingQueueIndex: null,
        commandInputValue: 'user typing',
      });

      store.getState().removeQueuedMessage(0);

      expect(store.getState().editingQueueIndex).toBeNull();
      // commandInputValue preserved since we weren't editing
      expect(store.getState().commandInputValue).toBe('user typing');
    });
  });
});

describe('Task-aware queue draining', () => {
  describe('processQueue with pending tasks', () => {
    it('does not drain queue when tasks are pending', async () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['queued msg'],
        tasks: [
          { id: '1', subject: 'Task A', status: 'pending' },
          { id: '2', subject: 'Task B', status: 'pending' },
        ],
      });

      await store.getState().processQueue();

      // Queue should be untouched
      expect(store.getState().queuedMessages).toEqual(['queued msg']);
    });

    it('does not drain queue when some tasks are pending', async () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['queued msg'],
        tasks: [
          { id: '1', subject: 'Task A', status: 'completed' },
          { id: '2', subject: 'Task B', status: 'pending' },
        ],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual(['queued msg']);
    });

    it('drains queue when all tasks are completed', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        queuedMessages: ['queued msg'],
        tasks: [
          { id: '1', subject: 'Task A', status: 'completed' },
          { id: '2', subject: 'Task B', status: 'completed' },
        ],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('drains queue when there are no tasks at all', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        queuedMessages: ['queued msg'],
        tasks: [],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('does not drain queue when isProcessing is true', async () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['queued msg'],
        tasks: [],
        isProcessing: true,
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual(['queued msg']);
    });
  });

  describe('setTasks triggers queue drain', () => {
    it('drains queue when all tasks become completed', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        queuedMessages: ['waiting msg'],
        isProcessing: false,
      });

      store.getState().setTasks([
        { id: '1', subject: 'Task A', status: 'completed' },
        { id: '2', subject: 'Task B', status: 'completed' },
      ]);

      // Allow async processQueue to complete
      await new Promise((r) => setTimeout(r, 10));

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('does not drain queue when some tasks are still pending', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['waiting msg'],
        isProcessing: false,
      });

      store.getState().setTasks([
        { id: '1', subject: 'Task A', status: 'completed' },
        { id: '2', subject: 'Task B', status: 'pending' },
      ]);

      expect(store.getState().queuedMessages).toEqual(['waiting msg']);
    });

    it('does not drain queue when isProcessing is true even if all tasks done', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['waiting msg'],
        isProcessing: true,
      });

      store
        .getState()
        .setTasks([{ id: '1', subject: 'Task A', status: 'completed' }]);

      expect(store.getState().queuedMessages).toEqual(['waiting msg']);
    });

    it('does not trigger drain for empty task list', () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['waiting msg'] });

      store.getState().setTasks([]);

      // Empty task list doesn't trigger drain (allDone requires tasks.length > 0)
      // Queue drains via normal processQueue after turn ends
      expect(store.getState().queuedMessages).toEqual(['waiting msg']);
    });
  });
});

describe('Editing state cleanup', () => {
  describe('toggleActivityTray', () => {
    it('clears editing state when collapsing', () => {
      const store = createTestStore();
      store.setState({
        activityTrayExpanded: true,
        editingQueueIndex: 0,
      });

      store.getState().toggleActivityTray();

      expect(store.getState().activityTrayExpanded).toBe(false);
      expect(store.getState().editingQueueIndex).toBeNull();
    });

    it('preserves editing state when expanding', () => {
      const store = createTestStore();
      store.setState({
        activityTrayExpanded: false,
        editingQueueIndex: null,
      });

      store.getState().toggleActivityTray();

      expect(store.getState().activityTrayExpanded).toBe(true);
      expect(store.getState().editingQueueIndex).toBeNull();
    });
  });

  describe('clearQueue', () => {
    it('clears editing state and input when editing', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a', 'b'],
        editingQueueIndex: 1,
        commandInputValue: 'editing b',
      });

      store.getState().clearQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().editingQueueIndex).toBeNull();
      expect(store.getState().commandInputValue).toBe('');
    });

    it('preserves commandInputValue when not editing', () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['a'],
        editingQueueIndex: null,
        commandInputValue: 'user typing something',
      });

      store.getState().clearQueue();

      expect(store.getState().commandInputValue).toBe('user typing something');
    });
  });

  describe('processQueue editing index adjustment', () => {
    it('clears editing state when the edited item (index 0) is dequeued', async () => {
      const store = createTestStore();
      store.setState({
        queuedMessages: ['being edited', 'next'],
        editingQueueIndex: 0,
        commandInputValue: 'being edited modified',
        tasks: [],
      });

      await store.getState().processQueue();

      expect(store.getState().editingQueueIndex).toBeNull();
      expect(store.getState().commandInputValue).toBe('');
    });

    it('drains entire queue recursively with mock kiro, clearing editing state', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        queuedMessages: ['first', 'second', 'third'],
        editingQueueIndex: 2,
        commandInputValue: 'third',
        tasks: [],
      });

      await store.getState().processQueue();

      // Mock kiro completes sendMessage immediately, so processQueue
      // recurses and drains the entire queue
      expect(store.getState().queuedMessages).toEqual([]);
      // The edited item (originally at index 2) was eventually dequeued
      expect(store.getState().editingQueueIndex).toBeNull();
    });
  });
});
