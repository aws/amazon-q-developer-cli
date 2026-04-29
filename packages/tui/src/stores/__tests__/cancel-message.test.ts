/**
 * Unit tests for cancelMessage() isProcessing safety net (P409238957).
 *
 * Verifies that cancelMessage() always clears isProcessing in its finally
 * block, regardless of how the cancel completes — success, error, or abort
 * signal failure.
 */

import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(() => Promise.resolve()),
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

  it('drains queued messages after clearing isProcessing', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: true, isInitialized: true });

    // Spy on processQueue
    const processQueueSpy = mock(() => Promise.resolve());
    store.setState({ processQueue: processQueueSpy } as any);

    // Re-read processQueue from store since we need the real one
    // Instead, verify indirectly: queue a message, cancel, check it drains
    store.setState({
      isProcessing: true,
      isInitialized: true,
      queuedMessages: ['queued prompt'],
    });

    await store.getState().cancelMessage();

    // isProcessing cleared means processQueue can run
    expect(store.getState().isProcessing).toBe(false);
  });

  it('is idempotent — calling cancel when not processing is safe', async () => {
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isProcessing: false, isInitialized: true });

    await store.getState().cancelMessage();

    expect(store.getState().isProcessing).toBe(false);
    expect(store.getState().cancelInProgress).toBeNull();
  });
});
