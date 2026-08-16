import { describe, it, expect, mock } from 'bun:test';
import { createAppStore, MessageRole } from '../app-store';
import { AgentEventType, ContentType } from '../../types/agent-events';
import type { Kiro } from '../../kiro';

// A stub instance instead of mock.module('../../kiro', ...): the module mock
// is process-global and would leak a method-less Kiro into every later test
// file in the shared-process run (bug-class of the /voice and dashboard test
// stalls). The store only touches these three members here.
const stubKiro = () =>
  ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  }) as unknown as Kiro;

const flushDelay = () => new Promise((resolve) => setTimeout(resolve, 40));

describe('stream_discarded handling', () => {
  it('drops the rendered partial so the retried stream replaces it', async () => {
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    // A partial response streams and flushes to the store.
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Truncated partial resp' },
    });
    await flushDelay();
    expect(store.getState().streamingContent).toContain('Truncated');
    const partialRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.Model);
    expect(partialRows.length).toBe(1);

    // The backend abandons the stream for a transient retry and says so.
    handler({ type: AgentEventType.StreamDiscarded });
    expect(store.getState().streamingContent).toBe('');
    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.Model)
        .length
    ).toBe(0);

    // The regenerated response must replace the partial, not concatenate.
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'Complete response.' },
    });
    await flushDelay();
    expect(store.getState().streamingContent).toBe('Complete response.');
    expect(store.getState().streamingContent).not.toContain('Truncated');
    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.Model)
        .length
    ).toBe(1);
  });

  it('drops the committed model row and never-executed tool row too', async () => {
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    // Text streams, then a tool-use start commits the model row and appends a
    // tool row; the stream drops before the tool executes.
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Let me check that file.' },
    });
    await flushDelay();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-1',
      name: 'read',
      args: {},
    });
    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.ToolUse)
        .length
    ).toBe(1);

    handler({ type: AgentEventType.StreamDiscarded });
    const afterDiscard = store.getState().messages;
    expect(
      afterDiscard.filter((m) => m.role === MessageRole.Model).length
    ).toBe(0);
    expect(
      afterDiscard.filter((m) => m.role === MessageRole.ToolUse).length
    ).toBe(0);

    // The regenerated stream renders exactly one response and one tool row.
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'Let me check that file.' },
    });
    await flushDelay();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-2',
      name: 'read',
      args: {},
    });
    const finalMessages = store.getState().messages;
    expect(
      finalMessages.filter((m) => m.role === MessageRole.Model).length
    ).toBe(1);
    expect(
      finalMessages.filter((m) => m.role === MessageRole.ToolUse).length
    ).toBe(1);
  });

  it('drops a thinking-only partial (no visible text yet)', async () => {
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    // Only reasoning tokens have streamed when the backend abandons the
    // stream — the discard must clear the thinking slot and its committed
    // row, or the retried response renders under stale reasoning.
    handler({
      type: AgentEventType.Thought,
      id: 't1',
      content: { type: ContentType.Text, text: 'Considering the options...' },
    });
    await flushDelay();
    expect(store.getState().thinkingContent).toContain('Considering');

    handler({ type: AgentEventType.StreamDiscarded });
    expect(store.getState().thinkingContent).toBe('');
    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.Model)
        .length
    ).toBe(0);

    // The regenerated stream starts clean.
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Fresh answer.' },
    });
    await flushDelay();
    const rows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.Model);
    expect(rows.length).toBe(1);
    expect(rows[0]?.thinking).toBeUndefined();
  });

  it('keeps rows from a completed request cycle (finished tool)', async () => {
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    // Cycle 1 completes: text, tool call, tool result.
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Running the tool.' },
    });
    await flushDelay();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-1',
      name: 'read',
      args: {},
    });
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-1',
      result: { status: 'success', output: 'ok' },
    });

    // Cycle 2 streams text and is then discarded.
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'Partial follow-up' },
    });
    await flushDelay();
    handler({ type: AgentEventType.StreamDiscarded });

    // Cycle 1's rows survive; only cycle 2's partial is gone.
    const messages = store.getState().messages;
    expect(messages.filter((m) => m.role === MessageRole.ToolUse).length).toBe(
      1
    );
    expect(messages.filter((m) => m.role === MessageRole.Model).length).toBe(1);
    expect(store.getState().streamingContent).toBe('');
  });

  it('a stall continuation leaves a visible reason after the discard', async () => {
    // The hard-stall tier sends discard-then-notice; the discard clears any
    // soft-tier banner, so only this ordering leaves the user an explanation
    // for the output that just vanished.
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Stalled partial' },
    });
    await flushDelay();
    // Soft tier warned first; its banner is up when the hard tier fires.
    handler({
      type: AgentEventType.StallNotice,
      message: 'Still working, model is thinking...',
    });
    expect(store.getState().retryStatus?.message).toContain('Still working');

    handler({ type: AgentEventType.StreamDiscarded });
    handler({
      type: AgentEventType.StallNotice,
      message:
        'Response timed out - discarding the stalled response and retrying',
    });

    expect(store.getState().streamingContent).toBe('');
    expect(store.getState().retryStatus?.message).toContain(
      'Response timed out'
    );
  });

  it('is a no-op when nothing has streamed yet', () => {
    const store = createAppStore({ kiro: stubKiro() });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    handler({ type: AgentEventType.StreamDiscarded });
    expect(store.getState().streamingContent).toBe('');
    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.Model)
        .length
    ).toBe(0);
  });
});
