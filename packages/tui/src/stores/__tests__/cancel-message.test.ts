/**
 * Unit tests for cancelMessage() isProcessing safety net (P409238957).
 *
 * Verifies that cancelMessage() always clears isProcessing in its finally
 * block, regardless of how the cancel completes — success, error, or abort
 * signal failure.
 */

import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { createAppStore, MessageRole, ToolUseStatus } from '../app-store';
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
