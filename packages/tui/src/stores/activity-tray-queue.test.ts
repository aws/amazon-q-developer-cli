import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { InterruptMode } from '../constants/interrupt-mode.js';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    sendMessage: mock(),
    steerMessage: mock(),
    clearSteering: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function createTestStore() {
  const mockKiro = new Kiro();
  return createAppStore({ kiro: mockKiro });
}

describe('Queue state (simplified)', () => {
  describe('pendingSteerContent', () => {
    it('starts as null', () => {
      const store = createTestStore();
      expect(store.getState().pendingSteerContent).toBeNull();
    });

    it('can be set to a string value', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'fix the bug' });
      expect(store.getState().pendingSteerContent).toBe('fix the bug');
    });

    it('can be cleared back to null', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'some message' });
      store.setState({ pendingSteerContent: null });
      expect(store.getState().pendingSteerContent).toBeNull();
    });
  });

  describe('queueMessage action', () => {
    it('calls kiro.steerMessage with sessionId and trimmed content', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        sessionId: 'session-123',
        isInitialized: true,
        activeInterruptMode: InterruptMode.STEER,
      });

      store.getState().queueMessage('  hello world  ');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-123',
        'hello world'
      );
    });

    it('does not call sendMessage (sendMessage is for session wake/reply, not steering)', () => {
      const store = createTestStore();
      const mockSendMessage = mock(() => Promise.resolve());
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).sendMessage = mockSendMessage;
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        sessionId: 'session-123',
        isInitialized: true,
        activeInterruptMode: InterruptMode.STEER,
      });

      store.getState().queueMessage('please redirect');

      expect(mockSendMessage).not.toHaveBeenCalled();
      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-123',
        'please redirect'
      );
    });

    it('does nothing for empty/whitespace-only content', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: 'session-123', isInitialized: true });

      store.getState().queueMessage('   ');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('buffers onto pendingSteerContent when sessionId is null (does not drop)', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: null, isInitialized: false });

      store.getState().queueMessage('hello');

      // Pre-init input is buffered locally on the same pendingSteerContent
      // slot that the backend queue uses. No backend round-trip yet —
      // there's no session to steer against.
      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().pendingSteerContent).toBe('hello');
      // No transient alert — the user's input will be replayed on init
      expect(store.getState().transientAlert).toBeNull();
    });

    it('concatenates multiple pre-session messages with "\\n\\n" (matches backend queue format)', () => {
      const store = createTestStore();
      store.setState({ sessionId: null, isInitialized: false });

      store.getState().queueMessage('first');
      store.getState().queueMessage('second');
      store.getState().queueMessage('third');

      expect(store.getState().pendingSteerContent).toBe(
        'first\n\nsecond\n\nthird'
      );
    });
  });

  describe('clearSteerMessage action', () => {
    it('optimistically clears pendingSteerContent and calls kiro.clearSteering when session-live', () => {
      const store = createTestStore();
      const mockClearSteering = mock(() => Promise.resolve());
      (store.getState().kiro as any).clearSteering = mockClearSteering;
      store.setState({
        isInitialized: true,
        sessionId: 'session-abc',
        pendingSteerContent: 'pending steer',
      });

      store.getState().clearSteerMessage();

      expect(store.getState().pendingSteerContent).toBeNull();
      expect(mockClearSteering).toHaveBeenCalledWith('session-abc');
    });

    it('clears the pre-session buffer without a backend call', () => {
      // Pre-session: there is no backend session to clear. Pressing Del
      // on the tray must still wipe what the user sees.
      const store = createTestStore();
      const mockClearSteering = mock(() => Promise.resolve());
      (store.getState().kiro as any).clearSteering = mockClearSteering;
      store.setState({
        isInitialized: false,
        sessionId: null,
        pendingSteerContent: 'pre1\n\npre2',
      });

      store.getState().clearSteerMessage();

      expect(store.getState().pendingSteerContent).toBeNull();
      expect(mockClearSteering).not.toHaveBeenCalled();
    });

    it('is a no-op when there is no queued message', () => {
      const store = createTestStore();
      const mockClearSteering = mock(() => Promise.resolve());
      (store.getState().kiro as any).clearSteering = mockClearSteering;
      store.setState({ sessionId: 'session-abc', pendingSteerContent: null });

      store.getState().clearSteerMessage();

      expect(mockClearSteering).not.toHaveBeenCalled();
    });
  });

  describe('toggleActivityTray', () => {
    it('toggles activityTrayExpanded', () => {
      const store = createTestStore();
      store.setState({ activityTrayExpanded: false });

      store.getState().toggleActivityTray();
      expect(store.getState().activityTrayExpanded).toBe(true);

      store.getState().toggleActivityTray();
      expect(store.getState().activityTrayExpanded).toBe(false);
    });
  });

  describe('setTasks updates task state', () => {
    it('updates tasks in the store', () => {
      const store = createTestStore();

      store.getState().setTasks([
        { id: '1', subject: 'Task A', status: 'completed' },
        { id: '2', subject: 'Task B', status: 'pending' },
      ]);

      expect(store.getState().tasks).toEqual([
        { id: '1', subject: 'Task A', status: 'completed' },
        { id: '2', subject: 'Task B', status: 'pending' },
      ]);
    });
  });
});

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
      // Negative index is out-of-bounds, so editingQueueIndex should be cleared
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
  describe('processQueue with tasks present', () => {
    it('drains queue when some tasks are pending', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        activeInterruptMode: 'queue',
        queuedMessages: ['queued msg'],
        tasks: [
          { id: '1', subject: 'Task A', status: 'completed' },
          { id: '2', subject: 'Task B', status: 'pending' },
        ],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('drains queue even when tasks are pending', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        activeInterruptMode: 'queue',
        queuedMessages: ['queued msg'],
        tasks: [
          { id: '1', subject: 'Task A', status: 'pending' },
          { id: '2', subject: 'Task B', status: 'pending' },
        ],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('drains queue when all tasks are completed', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        activeInterruptMode: 'queue',
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
        activeInterruptMode: 'queue',
        queuedMessages: ['queued msg'],
        tasks: [],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('does not drain queue when isProcessing is true', async () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        queuedMessages: ['queued msg'],
        tasks: [],
        isProcessing: true,
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual(['queued msg']);
    });
  });

  describe('setTasks updates task state', () => {
    it('does not automatically drain queue', () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        queuedMessages: ['waiting msg'],
        isProcessing: false,
      });

      store.getState().setTasks([
        { id: '1', subject: 'Task A', status: 'completed' },
        { id: '2', subject: 'Task B', status: 'completed' },
      ]);

      // setTasks no longer triggers processQueue — queue drains via
      // normal turn-end flow
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
        isInitialized: true,
        activeInterruptMode: 'queue',
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
        activeInterruptMode: 'queue',
        queuedMessages: ['first', 'second', 'third'],
        editingQueueIndex: 2,
        commandInputValue: 'third',
        tasks: [],
      });

      await store.getState().processQueue();

      // Mock kiro completes sendMessage immediately, so processQueue
      // recurses and drains the entire queue.
      expect(store.getState().queuedMessages).toEqual([]);
      // The edited item (originally at index 2) was eventually dequeued.
      expect(store.getState().editingQueueIndex).toBeNull();
    });
  });
});
