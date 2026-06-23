import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

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
  store.setState({ isInitialized: true, sessionId: 'test-session' });
  return store;
}

describe('processQueue', () => {
  it('dequeues the first message and sends it via sendMessage', async () => {
    const store = createTestStore();
    // Mock streamMessage to set isProcessing=true to prevent recursive drain
    // (simulates real behavior where a turn takes time)
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves — simulates an in-flight turn
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['first message', 'second message'],
    });

    // processQueue will dequeue 'first message' and call sendMessage.
    // sendMessage sets isProcessing=true, so subsequent processQueue calls are no-ops.
    // We don't await because the mock never resolves.
    const _processPromise = store.getState().processQueue();

    // Give the event loop a tick
    await new Promise((r) => setTimeout(r, 10));

    // First message should have been dequeued, second remains
    expect(store.getState().queuedMessages).toEqual(['second message']);
    // sendMessage was triggered with the first message
    expect(mockStreamMessage).toHaveBeenCalledTimes(1);
    const callArgs = mockStreamMessage.mock.calls[0] as unknown as string[];
    expect(callArgs?.[0]).toContain('first message');
  });

  it('replays pendingSteerContent before draining queue (steer cuts the line)', async () => {
    const store = createTestStore();
    const sentMessages: string[] = [];
    const mockStreamMessage = mock(async (content: string) => {
      sentMessages.push(content);
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      pendingSteerContent: 'urgent steer',
      queuedMessages: ['queued first', 'queued second'],
    });

    await store.getState().processQueue();

    // Steer should be sent first, queue untouched this round
    expect(sentMessages[0]).toBe('urgent steer');
    expect(store.getState().pendingSteerContent).toBeNull();
  });

  it('does not double-send a steered message (steer stays out of queuedMessages)', async () => {
    // The unified preview list surfaces the steer from pendingSteerContent, NOT
    // by copying it into queuedMessages. This pins the invariant the lite
    // visibility fix relies on: a mid-turn steer is drained exactly ONCE here
    // (the steer-first replay), and the local queue contains only genuine
    // queue entries — so processQueue can't send the steer text a second time
    // on top of the backend's own injection.
    const store = createTestStore();
    const sentMessages: string[] = [];
    // Never-resolving stream: the steer replay starts an in-flight turn, so
    // processQueue's `if (isProcessing) return` guard prevents the queue from
    // also draining in the same pass — exactly like a real turn.
    const mockStreamMessage = mock((content: string) => {
      sentMessages.push(content);
      store.setState({ isProcessing: true });
      return new Promise(() => {});
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'steer',
      isProcessing: false,
      // Steer lives ONLY here (set by the SteeringQueued echo), never copied
      // into queuedMessages.
      pendingSteerContent: 'steered text',
      queuedMessages: ['a genuine queue entry'],
    });

    // Don't await — the mock never resolves (in-flight turn). Give the event
    // loop a tick for the synchronous steer replay to fire.
    void store.getState().processQueue();
    await new Promise((r) => setTimeout(r, 10));

    // The steer was sent exactly once; the queue entry is still pending.
    expect(sentMessages).toEqual(['steered text']);
    expect(store.getState().pendingSteerContent).toBeNull();
    expect(store.getState().queuedMessages).toEqual(['a genuine queue entry']);
    // The steer text never leaked into the local queue.
    expect(store.getState().queuedMessages).not.toContain('steered text');
  });

  it('drains queue when no pending steer exists regardless of mode', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'steer', // Mode doesn't matter for draining
      isProcessing: false,
      pendingSteerContent: null,
      queuedMessages: ['should be sent'],
    });

    await store.getState().processQueue();

    // Queue should have been drained
    expect(store.getState().queuedMessages).toEqual([]);
    expect(mockStreamMessage).toHaveBeenCalled();
  });

  it('no-op when isProcessing is true (double-send prevention)', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: true,
      queuedMessages: ['should not be sent'],
    });

    await store.getState().processQueue();

    // Queue should remain unchanged
    expect(store.getState().queuedMessages).toEqual(['should not be sent']);
    expect(mockStreamMessage).not.toHaveBeenCalled();
  });

  it('no-op when queuedMessages is empty', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: [],
    });

    await store.getState().processQueue();

    expect(store.getState().queuedMessages).toEqual([]);
    expect(mockStreamMessage).not.toHaveBeenCalled();
  });

  it('adjusts editingQueueIndex correctly when dequeuing (index > 0 decrements)', async () => {
    const store = createTestStore();
    // Mock streamMessage to keep isProcessing=true so queue doesn't fully drain
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['first', 'second', 'third'],
      editingQueueIndex: 2,
    });

    const _processPromise = store.getState().processQueue();
    await new Promise((r) => setTimeout(r, 10));

    // editingQueueIndex was 2, after removing index 0 it should be 1
    expect(store.getState().editingQueueIndex).toBe(1);
  });

  it('sets editingQueueIndex to null when it was 0 (item being edited was dequeued)', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['editing this', 'next'],
      editingQueueIndex: 0,
    });

    await store.getState().processQueue();

    // editingQueueIndex was 0, which means the item at index 0 was being edited
    // Since we dequeued index 0, editing should be cleared
    expect(store.getState().editingQueueIndex).toBeNull();
  });

  it('leaves editingQueueIndex as null when it was already null', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['msg'],
      editingQueueIndex: null,
    });

    await store.getState().processQueue();

    expect(store.getState().editingQueueIndex).toBeNull();
  });

  it('after dequeue, remaining messages stay in buffer in order', async () => {
    const store = createTestStore();
    // Mock streamMessage to keep isProcessing=true so queue doesn't fully drain
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['first', 'second', 'third', 'fourth'],
    });

    const _processPromise = store.getState().processQueue();
    await new Promise((r) => setTimeout(r, 10));

    // Only the first message should be removed; rest stay in order
    expect(store.getState().queuedMessages).toEqual([
      'second',
      'third',
      'fourth',
    ]);
  });

  it('waits for cancelInProgress before processing', async () => {
    const store = createTestStore();
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    let resolveCancelPromise: () => void;
    const cancelPromise = new Promise<void>((resolve) => {
      resolveCancelPromise = resolve;
    });

    store.setState({
      activeInterruptMode: 'queue',
      isProcessing: false,
      queuedMessages: ['waiting msg'],
      cancelInProgress: cancelPromise,
    });

    // Start processQueue — it should wait for cancelInProgress
    const processPromise = store.getState().processQueue();

    // Resolve the cancel promise
    resolveCancelPromise!();

    await processPromise;

    // After cancel resolved and isProcessing was false, it should have sent
    expect(mockStreamMessage).toHaveBeenCalled();
  });
});
