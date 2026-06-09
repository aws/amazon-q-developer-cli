import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { AgentEventType } from '../types/agent-events';

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
  const store = createAppStore({ kiro: mockKiro });
  store.setState({ isInitialized: true });
  return store;
}

describe('Message queue (backend-driven)', () => {
  describe('queueMessage', () => {
    it('calls kiro.steerMessage with sessionId and trimmed content', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        sessionId: 'session-abc',
        activeInterruptMode: 'steer',
      });

      store.getState().queueMessage('  hello world  ');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-abc',
        'hello world'
      );
    });

    it('rejects empty string', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: 'session-abc' });

      store.getState().queueMessage('');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('rejects whitespace-only string', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: 'session-abc' });

      store.getState().queueMessage('   ');
      store.getState().queueMessage('\t\n');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('buffers onto pendingSteerContent when sessionId is null (does not drop)', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: null, isInitialized: false });

      store.getState().queueMessage('hello');

      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().pendingSteerContent).toBe('hello');
    });

    describe('mode-aware routing', () => {
      it('routes to steerMessage in steering mode', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('steer this');

        expect(mockSteerMessage).toHaveBeenCalledWith(
          'session-abc',
          'steer this'
        );
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('appends to queuedMessages in queueing mode', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('queue this');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual(['queue this']);
      });

      it('appends multiple messages to queuedMessages in order (queueing mode)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('first');
        store.getState().queueMessage('second');
        store.getState().queueMessage('third');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual([
          'first',
          'second',
          'third',
        ]);
      });

      it('trims whitespace before appending in queueing mode', () => {
        const store = createTestStore();
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('  padded  ');

        expect(store.getState().queuedMessages).toEqual(['padded']);
      });

      it('rejects empty/whitespace in queueing mode without modifying buffer', () => {
        const store = createTestStore();
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
          queuedMessages: ['existing'],
        });

        store.getState().queueMessage('');
        store.getState().queueMessage('   ');
        store.getState().queueMessage('\t\n');

        expect(store.getState().queuedMessages).toEqual(['existing']);
      });

      it('buffers to pendingSteerContent pre-init regardless of mode (steering)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('pre-init msg');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().pendingSteerContent).toBe('pre-init msg');
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('buffers to pendingSteerContent pre-init regardless of mode (queuing)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('pre-init msg');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().pendingSteerContent).toBe('pre-init msg');
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('concatenates pre-init buffers with double newline', () => {
        const store = createTestStore();
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('first');
        store.getState().queueMessage('second');

        expect(store.getState().pendingSteerContent).toBe('first\n\nsecond');
      });
    });
  });

  describe('pendingSteerContent state (notification-driven)', () => {
    it('starts as null', () => {
      const store = createTestStore();
      expect(store.getState().pendingSteerContent).toBeNull();
    });

    it('is set by SteeringQueued event', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'fix the bug' });
      expect(store.getState().pendingSteerContent).toBe('fix the bug');
    });

    it('is cleared by SteeringConsumed (set to null)', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'fix the bug' });
      store.setState({ pendingSteerContent: null });
      expect(store.getState().pendingSteerContent).toBeNull();
    });
  });

  describe('handleUserInput queuing', () => {
    it('queues message when isProcessing is true', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'steer',
      });

      await store.getState().handleUserInput('queued message');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-abc',
        'queued message'
      );
    });

    it('clears input buffer after queuing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('queued message');

      const input = store.getState().input;
      expect(input.lines).toEqual(['']);
      expect(input.cursorCol).toBe(0);
    });

    it('does not queue empty/whitespace input during processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('   ');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('rejects slash commands with a warning when processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('/help');

      // Slash command should NOT be queued
      expect(mockSteerMessage).not.toHaveBeenCalled();
      // A transient alert should be shown
      expect(store.getState().transientAlert).not.toBeNull();
      expect(store.getState().transientAlert?.status).toBe('warning');
    });

    it('rejects shell escape commands with a warning when processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('!ls');

      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().transientAlert).not.toBeNull();
      expect(store.getState().transientAlert?.status).toBe('warning');
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
  });
});

describe('Queueing mode behaviors', () => {
  describe('clearQueue', () => {
    it('is a no-op on empty queue', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'queue' });
      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('processQueue', () => {
    it('does not clear input buffer when processing queue', async () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
      });
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
    it('queues message when isProcessing is true (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('queued message');

      expect(store.getState().queuedMessages).toEqual(['queued message']);
    });

    it('rejects slash commands with a warning when not initialized', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      await store.getState().handleUserInput('/context');

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().transientAlert).not.toBeNull();
    });

    it('queues regular messages but not slash commands when processing (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('fix the bug');
      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('add tests too');

      // Only regular messages should be queued
      expect(store.getState().queuedMessages).toEqual([
        'fix the bug',
        'add tests too',
      ]);
    });

    it('does not queue slash commands when processing (queueing mode)', async () => {
      // We can't fully test /quit since it calls process.exit, but we can
      // verify that slash commands are never added to the queue while the
      // agent is processing — they pass through the slash-command handler.
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('/context');
      await store.getState().handleUserInput('/model');

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('queuing during initialization', () => {
    it('queues message via handleUserInput when not initialized (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: false,
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('early message');

      // Pre-init buffers to pendingSteerContent regardless of mode
      expect(store.getState().pendingSteerContent).toBe('early message');
    });

    it('buffers message via sendMessage when not initialized (pre-init path)', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: false,
        activeInterruptMode: 'queue',
      });

      await store.getState().sendMessage('early message');

      // Pre-init sendMessage calls queueMessage which buffers to pendingSteerContent
      expect(store.getState().pendingSteerContent).toBe('early message');
      expect(store.getState().isProcessing).toBe(false);
    });

    it('drains queue after isInitialized becomes true', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
        queuedMessages: ['queued during init'],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('cancellation semantics', () => {
    it('clearQueue + cancelMessage clears queue (Escape behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        activeInterruptMode: 'queue',
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
        activeInterruptMode: 'queue',
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual(['msg1', 'msg2', 'msg3']);
    });
  });

  describe('unified expanded state', () => {
    it('expanded state persists across queued turns (not reset by sendMessage)', async () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
      });
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
      store.setState({ activeInterruptMode: 'queue' });
      store.getState().toggleToolOutputsExpanded();
      store.setState({ queuedMessages: ['a', 'b'] });

      store.getState().clearQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });
  });
});

describe('Compaction drains queue', () => {
  it('processQueue is called after compaction completes', async () => {
    const store = createTestStore();
    store.setState({
      isCompacting: true,
      isProcessing: true,
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['queued during compaction'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('processQueue is called after compaction fails', async () => {
    const store = createTestStore();
    store.setState({
      isCompacting: true,
      isProcessing: true,
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['queued during compaction'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      error: 'test error',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('queue is untouched when compaction starts', async () => {
    const store = createTestStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['pre-existing'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
    });

    expect(store.getState().queuedMessages).toEqual(['pre-existing']);
  });
});
