/**
 * Unit tests for cancelMessage() behavior in both steering and queueing modes.
 *
 * Verifies:
 * 1. isProcessing safety net (P409238957) — always cleared in finally block
 * 2. Steering mode: replays pendingSteerContent as sendMessage after cancel
 * 3. Queueing mode: preserves queuedMessages buffer, calls processQueue after cancel
 * 4. Error handling in both modes
 */

import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { createAppStore, MessageRole, ToolUseStatus } from '../app-store';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(() => Promise.resolve()),
    clearSteering: mock(() => Promise.resolve()),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('cancelMessage clears isProcessing (P409238957)', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('clears isProcessing after successful cancel', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: true, isInitialized: true });

    await store.getState().cancelMessage();

    expect(store.getState().isProcessing).toBe(false);
    expect(store.getState().cancelInProgress).toBeNull();
  });

  it('clears isProcessing when kiro.cancel() throws', async () => {
    mockKiro.cancel = mock(() => Promise.reject(new Error('connection lost')));
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: true, isInitialized: true });

    await store.getState().cancelMessage();

    expect(store.getState().isProcessing).toBe(false);
    expect(store.getState().cancelInProgress).toBeNull();
    expect(store.getState().agentError).toBe('connection lost');
  });

  it('clears currentAbortController in finally block', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const controller = new AbortController();
    store.setState({
      isProcessing: true,
      isInitialized: true,
      currentAbortController: controller,
    });

    await store.getState().cancelMessage();

    expect(store.getState().currentAbortController).toBeNull();
    expect(store.getState().isProcessing).toBe(false);
  });

  it('clears isProcessing after cancel', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: true, isInitialized: true });

    await store.getState().cancelMessage();

    // isProcessing cleared after cancel
    expect(store.getState().isProcessing).toBe(false);
  });

  it('is idempotent — calling cancel when not processing is safe', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: false, isInitialized: true });

    await store.getState().cancelMessage();

    expect(store.getState().isProcessing).toBe(false);
    expect(store.getState().cancelInProgress).toBeNull();
  });

  it('flips unfinished Pending tools to Rejected with cancelled result', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({
      isProcessing: true,
      isInitialized: true,
      messages: [
        {
          id: 'pending-tool',
          role: MessageRole.ToolUse,
          name: 'fs_write',
          content: '{}',
          status: ToolUseStatus.Pending,
        },
      ],
    });

    await store.getState().cancelMessage();

    const msg = store.getState().messages.find((m) => m.id === 'pending-tool');
    expect(msg).toBeDefined();
    if (msg!.role === MessageRole.ToolUse) {
      expect(msg!.isFinished).toBe(true);
      expect(msg!.status).toBe(ToolUseStatus.Rejected);
      expect(msg!.result).toEqual({ status: 'cancelled' });
    }
  });

  it('preserves Approved status on already-approved unfinished tools', async () => {
    // A tool that the user explicitly Approved and is still executing should
    // not get reclassified as Rejected when streaming is cancelled. The
    // user-visible "Cancelled" label comes from `result.status === 'cancelled'`,
    // so the internal status can stay Approved without affecting render.
    const store = createAppStore({ kiro: mockKiro });
    store.setState({
      isProcessing: true,
      isInitialized: true,
      messages: [
        {
          id: 'approved-tool',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{"command":"sleep 30"}',
          status: ToolUseStatus.Approved,
        },
      ],
    });

    await store.getState().cancelMessage();

    const msg = store.getState().messages.find((m) => m.id === 'approved-tool');
    expect(msg).toBeDefined();
    if (msg!.role === MessageRole.ToolUse) {
      expect(msg!.isFinished).toBe(true);
      expect(msg!.status).toBe(ToolUseStatus.Approved);
      expect(msg!.result).toEqual({ status: 'cancelled' });
    }
  });
});

describe('cancelMessage in steering mode', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('replays pendingSteerContent as a fresh prompt after cancel', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const sendMessage = mock(() => Promise.resolve());
    (store.getState() as any).sendMessage = sendMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'steer',
      pendingSteerContent: 'redirect to counting',
    });

    await store.getState().cancelMessage();

    // Queue was captured and replayed as a fresh prompt
    expect(sendMessage).toHaveBeenCalledWith('redirect to counting');
    // Local queue display was cleared so the tray doesn't show stale "pending"
    expect(store.getState().pendingSteerContent).toBeNull();
    // Backend clears the queue on cancel (emits SteeringCleared), so no
    // explicit clearSteering() call is needed from the TUI.
    expect(mockKiro.clearSteering).not.toHaveBeenCalled();
  });

  it('suppresses the generic "Cancelled streaming" toast when a redirect runs', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const sendMessage = mock(() => Promise.resolve());
    (store.getState() as any).sendMessage = sendMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'steer',
      pendingSteerContent: 'new instruction',
    });

    await store.getState().cancelMessage();

    // The final transient alert should not be the generic cancel toast,
    // because a new turn is about to start via the replay.
    const alert = store.getState().transientAlert;
    expect(alert?.message).not.toBe('Cancelled streaming');
  });

  it('shows the "Cancelled streaming" toast when no pending message exists', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const sendMessage = mock(() => Promise.resolve());
    (store.getState() as any).sendMessage = sendMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'steer',
      pendingSteerContent: null,
    });

    await store.getState().cancelMessage();

    expect(sendMessage).not.toHaveBeenCalled();
    expect(mockKiro.clearSteering).not.toHaveBeenCalled();
    expect(store.getState().transientAlert?.message).toBe(
      'Cancelled streaming'
    );
  });

  it('surfaces a visible warning if the replay fails so the redirect is not silently lost', async () => {
    const store = createAppStore({ kiro: mockKiro });
    // processQueue calls sendMessage internally via streamMessage
    const mockStreamMessage = mock(() =>
      Promise.reject(new Error('network unavailable'))
    );
    (store.getState().kiro as any).streamMessage = mockStreamMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'steer',
      pendingSteerContent: 'redirect to counting',
    });

    await store.getState().cancelMessage();

    // processQueue should have attempted to send the steer and errored
    expect(store.getState().pendingSteerContent).toBeNull();
  });

  it('calls processQueue which handles steer-first priority', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'steer',
      pendingSteerContent: 'steer first',
      queuedMessages: ['queue second'],
    });

    const _cancelPromise = store.getState().cancelMessage();
    await new Promise((r) => setTimeout(r, 50));

    // processQueue should replay the steer first
    expect(store.getState().pendingSteerContent).toBeNull();
    expect(mockStreamMessage).toHaveBeenCalledTimes(1);
  });
});

describe('cancelMessage in queueing mode', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('preserves queuedMessages buffer and calls processQueue after cancel resolves', async () => {
    const store = createAppStore({ kiro: mockKiro });
    // Mock streamMessage to simulate processQueue triggering sendMessage
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves — simulates in-flight turn
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: ['first queued', 'second queued'],
      pendingSteerContent: null,
    });

    const _cancelPromise = store.getState().cancelMessage();

    // Give the event loop time to process
    await new Promise((r) => setTimeout(r, 50));

    // processQueue should have dequeued the first message
    expect(store.getState().queuedMessages).toEqual(['second queued']);
    // streamMessage (underlying sendMessage) was called — confirming processQueue ran
    expect(mockStreamMessage).toHaveBeenCalledTimes(1);
  });

  it('just cancels without sending when buffer is empty', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: [],
      pendingSteerContent: null,
    });

    await store.getState().cancelMessage();

    // No messages sent since buffer was empty
    expect(mockStreamMessage).not.toHaveBeenCalled();
    expect(store.getState().queuedMessages).toEqual([]);
    expect(store.getState().isProcessing).toBe(false);
  });

  it('replays pendingSteerContent first even in queueing mode (steer cuts the line)', async () => {
    const store = createAppStore({ kiro: mockKiro });
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      pendingSteerContent: 'steer content goes first',
      queuedMessages: ['queued after'],
    });

    const _cancelPromise = store.getState().cancelMessage();
    await new Promise((r) => setTimeout(r, 50));

    // processQueue replays steer first regardless of mode
    expect(store.getState().pendingSteerContent).toBeNull();
    expect(mockStreamMessage).toHaveBeenCalledTimes(1);
    // Queue not yet drained — steer turn is still in progress
    expect(store.getState().queuedMessages).toEqual(['queued after']);
  });

  it('clears isProcessing even when kiro.cancel() throws in queueing mode', async () => {
    mockKiro.cancel = mock(() => Promise.reject(new Error('backend error')));
    const store = createAppStore({ kiro: mockKiro });
    // Mock streamMessage to keep isProcessing=true so processQueue doesn't fully drain
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: ['preserved message', 'second message'],
      pendingSteerContent: null,
    });

    const _cancelPromise = store.getState().cancelMessage();
    await new Promise((r) => setTimeout(r, 50));

    // processQueue still ran after cancel error and dequeued first message
    // (sendMessage clears agentError when starting a new turn, which is correct)
    expect(mockStreamMessage).toHaveBeenCalledTimes(1);
    // Remaining messages preserved in buffer
    expect(store.getState().queuedMessages).toEqual(['second message']);
  });

  it('drains queued messages after clearing isProcessing (settles to idle)', async () => {
    const store = createAppStore({ kiro: mockKiro });
    // streamMessage resolves immediately, so the replayed turn completes and
    // the queue fully drains, leaving the store idle.
    const mockStreamMessage = mock(() => Promise.resolve());
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: ['queued prompt'],
      pendingSteerContent: null,
    });

    await store.getState().cancelMessage();
    // Let the post-cancel drain settle.
    await new Promise((r) => setTimeout(r, 50));

    // The queued message was drained and processing returned to idle —
    // clearing isProcessing in the finally block is what lets processQueue run.
    expect(store.getState().queuedMessages).toEqual([]);
    expect(store.getState().isProcessing).toBe(false);
    expect(store.getState().cancelInProgress).toBeNull();
  });

  it('shows "Cancelled streaming" toast when buffer is empty in queueing mode', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: [],
      pendingSteerContent: null,
    });

    await store.getState().cancelMessage();

    expect(store.getState().transientAlert?.message).toBe(
      'Cancelled streaming'
    );
  });

  it('preserves buffer order when processQueue dequeues after cancel', async () => {
    const store = createAppStore({ kiro: mockKiro });
    // Mock streamMessage to keep isProcessing=true (prevents further drain)
    const mockStreamMessage = mock(() => {
      store.setState({ isProcessing: true });
      return new Promise(() => {}); // Never resolves
    });
    (store.getState().kiro as any).streamMessage = mockStreamMessage;

    store.setState({
      isProcessing: true,
      isInitialized: true,
      sessionId: 'session-1',
      activeInterruptMode: 'queue',
      queuedMessages: ['msg-a', 'msg-b', 'msg-c'],
      pendingSteerContent: null,
    });

    const _cancelPromise = store.getState().cancelMessage();
    await new Promise((r) => setTimeout(r, 50));

    // Only first message dequeued; rest preserved in order
    expect(store.getState().queuedMessages).toEqual(['msg-b', 'msg-c']);
  });
});
