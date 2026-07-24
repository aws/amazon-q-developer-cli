import { describe, it, expect, mock, jest, afterAll } from 'bun:test';
import {
  createAppStore,
  MessageRole,
  ToolUseStatus,
  NOT_READY_TOOLS,
  type HookInfo,
} from './app-store';
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

afterAll(() => {
  mock.restore();
});

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

  it('flushContentToStore appends a placeholder Model row and writes live text to streamingContent', async () => {
    const store = createStore();

    // Seed a user message and a prior model message (from an earlier turn).
    // The new turn appends ANOTHER Model row (the streaming placeholder) —
    // it does not edit the prior one in place; that one already committed.
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'hello' },
        { id: 'm1', role: MessageRole.Model, content: 'initial' },
      ],
    });

    const handler = store.getState().createStreamEventHandler();

    handler!({
      type: AgentEventType.Content,
      id: 'm-new',
      content: { type: ContentType.Text, text: 'updated response' },
    });

    // Batched flush is scheduled via setTimeout(fn, 16) — wait past it.
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Three rows now: user, prior model, streaming placeholder. The live
    // text lives in `streamingContent` AND is mirrored onto the placeholder
    // row's content field so external readers (transcript export, integ
    // tests reading the messages snapshot) stay in sync mid-stream. The
    // prior model row stays untouched (same id, same content).
    const state = store.getState();
    expect(state.messages).toHaveLength(3);
    expect(state.messages[0]?.id).toBe('u1');
    expect(state.messages[1]?.id).toBe('m1');
    expect(state.messages[1]?.content).toBe('initial');
    expect(state.messages[2]?.role).toBe(MessageRole.Model);
    expect(state.messages[2]?.content).toBe('updated response');
    expect(state.streamingContent).toBe('updated response');
  });

  it('flushContentToStore appends placeholder Model row when last is not model', async () => {
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

    // Live streaming text lands in `streamingContent` AND is mirrored onto
    // the placeholder Model row's content so external readers can see
    // streamed text mid-stream. The row is committed at turn end.
    const state = store.getState();
    expect(state.messages).toHaveLength(2);
    expect(state.messages[1]?.role).toBe(MessageRole.Model);
    expect(state.messages[1]?.content).toBe('first chunk');
    expect(state.streamingContent).toBe('first chunk');
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
    streamingBuffer?.startBuffering?.();

    // We need to set bufferedContent — simulate by sending a content event
    // while buffering is active (it won't schedule a flush, just buffers)
    const handler = store.getState().createStreamEventHandler();
    handler!({
      type: AgentEventType.Content,
      id: 'x',
      content: { type: ContentType.Text, text: 'buffered text' },
    });

    // stopBuffering calls commitBufferedContent
    store.getState().streamingBuffer?.stopBuffering?.();

    // Messages should not have been unnecessarily replaced
    const msgsAfter = store.getState().messages;
    expect(msgsAfter).toHaveLength(1);
  });
});

describe('Stream handler dispose (cancel race hardening)', () => {
  function createStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });
    return store;
  }

  it('dispose drops buffered content instead of committing it', async () => {
    const store = createStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hello' }],
    });

    const handler = store.getState().createStreamEventHandler();

    // Stream partial content
    handler({
      type: AgentEventType.Content,
      id: 'm1',
      content: { type: ContentType.Text, text: 'partial response' },
    });

    // Cancel the turn mid-stream — dispose before the batched flush timer fires
    handler.dispose();

    // Wait long enough for any stale timers to have fired
    await new Promise((resolve) => setTimeout(resolve, 50));

    // The partial content must NOT have leaked into the message list
    const msgs = store.getState().messages;
    expect(msgs).toHaveLength(1);
    expect(msgs[0]?.role).toBe(MessageRole.User);
  });

  it('events arriving after dispose are no-ops (simulates post-cancel stream chunks)', async () => {
    const store = createStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hello' }],
    });

    const handler = store.getState().createStreamEventHandler();
    handler.dispose();

    // Simulate a late event that arrives through the ACP SDK's
    // deferred-unsubscribe window. It must not commit a Model message.
    handler({
      type: AgentEventType.Content,
      id: 'late',
      content: {
        type: ContentType.Text,
        text: 'late chunk from cancelled stream',
      },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    const msgs = store.getState().messages;
    expect(msgs).toHaveLength(1);
    expect(msgs[0]?.role).toBe(MessageRole.User);
  });

  it('dispose clears streamingBuffer so old closures do not retain bufferedContent', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    // createStreamEventHandler installs non-null buffer control fns
    expect(store.getState().streamingBuffer.startBuffering).not.toBeNull();

    handler.dispose();

    // After dispose, the store must not hold closures that keep the
    // old handler's bufferedContent alive. Consumers see null.
    expect(store.getState().streamingBuffer.startBuffering).toBeNull();
    expect(store.getState().streamingBuffer.stopBuffering).toBeNull();
  });

  it('dispose is idempotent (safe to call multiple times)', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    expect(() => {
      handler.dispose();
      handler.dispose();
      handler.dispose();
    }).not.toThrow();
  });

  it('flush is a no-op after dispose (does not resurrect stale content)', async () => {
    const store = createStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hello' }],
    });

    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Content,
      id: 'm1',
      content: { type: ContentType.Text, text: 'buffered' },
    });

    handler.dispose();
    // Caller (sendMessage success path) would normally call flush after
    // streamMessage resolves. Post-dispose it must not leak content.
    handler.flush();

    await new Promise((resolve) => setTimeout(resolve, 50));

    const msgs = store.getState().messages;
    expect(msgs).toHaveLength(1);
    expect(msgs[0]?.role).toBe(MessageRole.User);
  });

  it('tool output buffers are cleared on dispose', async () => {
    const store = createStore();
    store.setState({
      messages: [
        { id: 't1', role: MessageRole.ToolUse, name: 'bash', content: '{}' },
      ],
    });

    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.ToolCallUpdate,
      id: 't1',
      content: { type: ContentType.Text, text: 'line1\n' },
    });

    handler.dispose();

    await new Promise((resolve) => setTimeout(resolve, 50));

    // Live output buffer for the abandoned tool call must not be populated
    // from the cancelled handler's batched flush.
    const liveOutput = store.getState().liveOutputs.get('t1') ?? [];
    expect(liveOutput).toEqual([]);
  });
});

describe('Always-on live renderer (observer turns)', () => {
  function createStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });
    return store;
  }

  it('TurnStart sets isProcessing and TurnEnd clears it (observer turn)', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({ type: AgentEventType.TurnStart });
    expect(store.getState().isProcessing).toBe(true);

    handler({ type: AgentEventType.TurnEnd });
    expect(store.getState().isProcessing).toBe(false);
  });

  it('leaves isProcessing true after a replayed turn_start with no turn_end (mid-turn resume)', () => {
    // KAS replays the in-flight turn's turn_start on load but NOT a turn_end
    // (the turn is still running). Replaying that through the live handler must
    // leave isProcessing true so the thinking indicator shows for the resumed
    // turn — the bug where a mid-turn resume rendered content but no spinner.
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    // Simulate the load replay: prior completed turn (start→end), then the
    // still-open turn's start with no matching end.
    handler({ type: AgentEventType.TurnStart });
    handler({ type: AgentEventType.TurnEnd });
    handler({ type: AgentEventType.TurnStart });

    expect(store.getState().isProcessing).toBe(true);
  });

  it('renders live content arriving without a prompt (resume-mid-turn)', async () => {
    const store = createStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'question' }],
    });
    const handler = store.getState().createStreamEventHandler();

    // A turn already running when the session was resumed streams content with
    // no local sendMessage in flight — it must land in a Model row anyway.
    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.Content,
      id: 'm-live',
      content: { type: ContentType.Text, text: 'streamed answer' },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const state = store.getState();
    const modelRow = state.messages.find((m) => m.role === MessageRole.Model);
    expect(modelRow?.content).toBe('streamed answer');
  });

  it('reset commits partial content and stays live for the next turn', async () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Content,
      id: 'm1',
      content: { type: ContentType.Text, text: 'partial' },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    handler.reset();

    // Partial content committed to a Model row (not dropped like dispose).
    expect(store.getState().messages.some((m) => m.content === 'partial')).toBe(
      true
    );

    // Still live: a subsequent turn's content renders into a fresh row.
    handler({
      type: AgentEventType.Content,
      id: 'm2',
      content: { type: ContentType.Text, text: 'next turn' },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      store.getState().messages.some((m) => m.content === 'next turn')
    ).toBe(true);
  });

  it('keeps an open replayed turn in one row through its live continuation', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler.resetSession();
    handler.setHistoryReplay(true);
    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.Content,
      id: 'history-prefix',
      content: { type: ContentType.Text, text: 'hel' },
      meta: { kiro: { messageId: 'persisted-message' } },
    } as never);
    handler.setHistoryReplay(false);
    handler({
      type: AgentEventType.Content,
      id: 'live-tail',
      content: { type: ContentType.Text, text: 'lo' },
    });
    handler({ type: AgentEventType.TurnEnd });

    const rows = store
      .getState()
      .messages.filter((message) => message.role === MessageRole.Model);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.content).toBe('hello');
  });

  it('splits local content around a steer consumed by the persistent handler', () => {
    const store = createStore();
    const localHandler = store.getState().createStreamEventHandler();
    const persistentHandler = store.getState().createStreamEventHandler();
    store.setState({ _activeStreamHandler: localHandler });

    localHandler({
      type: AgentEventType.Content,
      id: 'before-steer',
      content: { type: ContentType.Text, text: 'before' },
    });
    persistentHandler({
      type: AgentEventType.SteeringConsumed,
      content: 'change direction',
    } as never);
    localHandler({
      type: AgentEventType.Content,
      id: 'after-steer',
      content: { type: ContentType.Text, text: 'after' },
    });
    localHandler.flush();

    expect(
      store
        .getState()
        .messages.map((message) => [
          message.role,
          message.content,
          message.role === MessageRole.User ? message.steered : undefined,
        ])
    ).toEqual([
      [MessageRole.Model, 'before', undefined],
      [MessageRole.User, 'change direction', true],
      [MessageRole.Model, 'after', undefined],
    ]);

    store.setState({ _activeStreamHandler: null });
    localHandler.dispose();
    persistentHandler.dispose();
  });

  it('TurnEnd drains input queued behind an observer turn', async () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();
    const sent: string[] = [];
    store.setState({
      queuedMessages: ['next prompt'],
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });

    handler({ type: AgentEventType.TurnStart });
    handler({ type: AgentEventType.TurnEnd });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent).toEqual(['next prompt']);
  });

  it('watchdog drains input queued behind a stale observer boundary', async () => {
    jest.useFakeTimers();
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();
    const sent: string[] = [];
    store.setState({
      queuedMessages: ['next prompt'],
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });

    try {
      handler({ type: AgentEventType.TurnStart });
      jest.advanceTimersByTime(30_000);
      await Promise.resolve();

      expect(store.getState().isProcessing).toBe(false);
      expect(sent).toEqual(['next prompt']);
    } finally {
      handler.dispose();
      jest.useRealTimers();
    }
  });

  it('dispose prevents a replay watchdog from draining queued input', async () => {
    jest.useFakeTimers();
    const store = createStore();
    const handler = store.getState().createStreamEventHandler({
      fromHistory: true,
    });
    const sent: string[] = [];
    store.setState({
      queuedMessages: ['next prompt'],
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });

    try {
      handler({ type: AgentEventType.TurnStart });
      handler.flush();
      handler.dispose();
      jest.advanceTimersByTime(30_000);
      await Promise.resolve();

      expect(sent).toEqual([]);
    } finally {
      handler.dispose();
      jest.useRealTimers();
    }
  });

  it('does not drain queued input after an observer auth error', async () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();
    const sent: string[] = [];
    store.setState({
      queuedMessages: ['wait for re-auth'],
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });

    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.AuthError,
      message: 'sign in again',
    } as never);
    handler({ type: AgentEventType.TurnEnd });
    await store.getState().processQueue();

    expect(store.getState().isProcessing).toBe(false);
    expect(sent).toEqual([]);
    expect(store.getState().queuedMessages).toEqual(['wait for re-auth']);

    store.getState().setAgentError(null);
    await store.getState().processQueue();
    expect(sent).toEqual(['wait for re-auth']);
  });

  it('dedupes the backend echo of a locally-submitted user message', () => {
    const store = createStore();
    // Simulate sendMessage's optimistic append + recording.
    store.setState({
      messages: [
        { id: 'local-1', role: MessageRole.User, content: 'hi there' },
      ],
      _recentLocalUserMessages: [{ content: 'hi there', at: Date.now() }],
    });
    const handler = store.getState().createStreamEventHandler();

    // The backend echoes the same text with a different id — must be dropped.
    handler({
      type: AgentEventType.UserMessage,
      id: 'echo-1',
      content: { type: ContentType.Text, text: 'hi there' },
    });

    const userRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.User);
    expect(userRows).toHaveLength(1);
    expect(userRows[0]?.id).toBe('local-1');
  });

  it('renders a user message with no local match (web-initiated)', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.UserMessage,
      id: 'web-1',
      content: { type: ContentType.Text, text: 'from the web' },
    });

    const userRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.User);
    expect(userRows).toHaveLength(1);
    expect(userRows[0]?.content).toBe('from the web');
  });

  it('stamps steered on a user message replayed inside an open turn', () => {
    // A mid-turn steer replays as a plain user_message (the persisted
    // source:'steer' marker is dropped by the backend projection). Position
    // is authoritative: real prompts precede turn_start, so a user message
    // between turn_start and turn_end is a steer and must fold into the
    // turn instead of anchoring a "Cancelled" card.
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.UserMessage,
      id: 'p1',
      content: { type: ContentType.Text, text: 'real prompt' },
    });
    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.UserMessage,
      id: 's1',
      content: { type: ContentType.Text, text: 'say hi when you finished' },
    });
    handler({ type: AgentEventType.TurnEnd, stopReason: 'cancelled' });
    handler({
      type: AgentEventType.UserMessage,
      id: 'p2',
      content: { type: ContentType.Text, text: 'next prompt' },
    });

    const userRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.User) as Array<{
      id: string;
      steered?: boolean;
    }>;
    expect(userRows.map((m) => [m.id, m.steered ?? false])).toEqual([
      ['p1', false],
      ['s1', true],
      ['p2', false],
    ]);
  });

  it('skips empty user messages (steer-cleared artifacts) on replay', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.UserMessage,
      id: 'empty-1',
      content: { type: ContentType.Text, text: '' },
    });

    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.User)
    ).toHaveLength(0);
  });

  it('sweeps unfinished tools to cancelled when a replayed turn ends cancelled', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.ToolCall,
      id: 'stage-1',
      name: 'Sub-agent: general-task-execution',
      args: { prompt: 'research' },
    } as never);
    handler({
      type: AgentEventType.ToolCall,
      id: 'done-1',
      name: 'read_file',
      args: { path: '/tmp/x' },
    } as never);
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'done-1',
      result: { status: 'success', output: 'ok' },
    } as never);
    handler({ type: AgentEventType.TurnEnd, stopReason: 'cancelled' });

    const rows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.ToolUse) as Array<{
      id: string;
      isFinished?: boolean;
      result?: { status: string };
    }>;
    const stage = rows.find((r) => r.id === 'stage-1');
    const done = rows.find((r) => r.id === 'done-1');
    // The still-running stage closes as cancelled; the tool that genuinely
    // completed before the cancel keeps its success result.
    expect(stage?.isFinished).toBe(true);
    expect(stage?.result?.status).toBe('cancelled');
    expect(done?.result?.status).toBe('success');
  });

  it('does not sweep tools when a replayed turn ends normally', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler({ type: AgentEventType.TurnStart });
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-1',
      name: 'read_file',
      args: { path: '/tmp/x' },
    } as never);
    handler({ type: AgentEventType.TurnEnd, stopReason: 'end_turn' });

    const row = store
      .getState()
      .messages.find(
        (m) => m.role === MessageRole.ToolUse && m.id === 'tool-1'
      ) as { result?: { status: string } } | undefined;
    expect(row?.result?.status).not.toBe('cancelled');
  });

  it('does not replay a backend-held steer as a prompt after natural completion', async () => {
    // The backend keeps an unconsumed steer and injects it into the next
    // turn itself; on a relayed session the SteeringConsumed echo also lags
    // seconds behind the prompt response. Replaying pendingSteerContent as a
    // fresh prompt after a natural turn end therefore double-delivers it.
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SteeringQueued,
      message: 'say hi when you done',
    } as never);
    expect(store.getState().pendingSteerContent).toBe('say hi when you done');

    const sent: string[] = [];
    store.setState({
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });
    await store.getState().processQueue();

    expect(sent).toEqual([]);
    // Tray keeps showing the still-queued steer (backend owns it).
    expect(store.getState().pendingSteerContent).toBe('say hi when you done');
  });

  it('replays the steer as a prompt only when armed by the cancel re-seed', async () => {
    const store = createStore();
    store.setState({
      pendingSteerContent: 'redirect me',
      _steerReplayArmed: true,
    });
    const sent: string[] = [];
    store.setState({
      sendMessage: (async (content: string) => {
        sent.push(content);
      }) as never,
    });
    await store.getState().processQueue();

    expect(sent).toEqual(['redirect me']);
    expect(store.getState().pendingSteerContent).toBeNull();
    expect(store.getState()._steerReplayArmed).toBe(false);
  });

  it('drops persisted re-deliveries after replay completes (relay re-attach)', async () => {
    // A broken relay downlink re-attaches by replaying the persisted log
    // through the live channel. Persisted chunks carry meta.kiro.messageId;
    // live deltas do not. A chunk whose id was already rendered — and is not
    // the message currently streaming — is a re-delivery and must not render
    // again, while the initial replay renders in full no matter how late it
    // arrives (cloud replay streams in after session/load resolves).
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    // Initial replay (arbitrarily late): two chunks of one persisted message
    // both render (same id stays current), then a second message.
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'first-a ' },
      meta: { kiro: { messageId: 'm-1' } },
    } as never);
    handler({
      type: AgentEventType.Content,
      id: 'c1b',
      content: { type: ContentType.Text, text: 'first-b ' },
      meta: { kiro: { messageId: 'm-1' } },
    } as never);
    handler({
      type: AgentEventType.UserMessage,
      id: 'u-2',
      content: { type: ContentType.Text, text: 'next prompt' },
    });
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'second' },
      meta: { kiro: { messageId: 'm-2' } },
    } as never);
    handler.reset();

    // Re-attach burst: persisted chunks re-delivered — dropped.
    handler({
      type: AgentEventType.Content,
      id: 'c1-again',
      content: { type: ContentType.Text, text: 'first-a ' },
      meta: { kiro: { messageId: 'm-1' } },
    } as never);
    handler({
      type: AgentEventType.Content,
      id: 'c2-again',
      content: { type: ContentType.Text, text: 'second' },
      meta: { kiro: { messageId: 'm-2' } },
    } as never);
    // A genuinely live delta (no persisted id) still renders.
    handler({
      type: AgentEventType.Content,
      id: 'c3',
      content: { type: ContentType.Text, text: 'live tail' },
    } as never);
    await new Promise((r) => setTimeout(r, 50));
    handler.flush();

    const text = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.Model)
      .map((m) => m.content)
      .join('|');
    // Initial replay rendered fully, including the multi-chunk message.
    expect(text).toContain('first-a first-b');
    expect(text).toContain('second');
    expect(text).toContain('live tail');
    // Re-delivered persisted chunks did not render twice.
    expect((text.match(/first-a/g) ?? []).length).toBe(1);
    expect((text.match(/second/g) ?? []).length).toBe(1);
  });

  it('accepts a persisted message id again after resetting for another session', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();
    const persisted = (id: string, text: string) =>
      ({
        type: AgentEventType.Content,
        id,
        content: { type: ContentType.Text, text },
        meta: { kiro: { messageId: 'shared-message-id' } },
      }) as never;

    handler(persisted('first-session', 'first session'));
    handler.reset();
    handler.resetSession();
    handler(persisted('second-session', 'second session'));
    handler.reset();

    const text = store
      .getState()
      .messages.filter((message) => message.role === MessageRole.Model)
      .map((message) => message.content)
      .join('|');
    expect(text).toContain('first session');
    expect(text).toContain('second session');
  });
  it('drops an echo whose id matches a rendered row (kasMessageId identity)', () => {
    const store = createStore();
    store.setState({
      messages: [
        {
          id: 'local-1',
          role: MessageRole.User,
          content: 'same words',
          kasMessageId: 'kas-42',
        },
      ],
    });
    const handler = store.getState().createStreamEventHandler();

    // Echo carries the persisted id, not the local row id — must be dropped
    // even without any _recentLocalUserMessages entry.
    handler({
      type: AgentEventType.UserMessage,
      id: 'kas-42',
      content: { type: ContentType.Text, text: 'same words' },
    });
    // A remote message with the SAME TEXT but a new id still renders.
    handler({
      type: AgentEventType.UserMessage,
      id: 'kas-43',
      content: { type: ContentType.Text, text: 'same words' },
    });

    const userRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.User);
    expect(userRows).toHaveLength(2);
    expect(userRows.map((m) => m.id)).toEqual(['local-1', 'kas-43']);
  });

  it('dedupes an echo of the transmitted (expanded) content', () => {
    const store = createStore();
    store.setState({
      messages: [
        { id: 'local-1', role: MessageRole.User, content: '@file:notes.md' },
      ],
      _recentLocalUserMessages: [
        {
          content: '@file:notes.md',
          sentContent: 'expanded file contents here',
          at: Date.now(),
        },
      ],
    });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.UserMessage,
      id: 'echo-1',
      content: { type: ContentType.Text, text: 'expanded file contents here' },
    });

    const userRows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.User);
    expect(userRows).toHaveLength(1);
    expect(userRows[0]?.id).toBe('local-1');
  });
});

describe('Enum and constant exports', () => {
  it('MessageRole has the expected values', () => {
    expect(MessageRole.User as string).toBe('user');
    expect(MessageRole.Model as string).toBe('model');
    expect(MessageRole.ToolUse as string).toBe('tool_use');
    expect(MessageRole.System as string).toBe('system');
  });

  it('ToolUseStatus has the expected values', () => {
    expect(ToolUseStatus.Pending as string).toBe('pending');
    expect(ToolUseStatus.Approved as string).toBe('approved');
    expect(ToolUseStatus.Rejected as string).toBe('rejected');
  });

  it('NOT_READY_TOOLS is a Set', () => {
    expect(NOT_READY_TOOLS).toBeInstanceOf(Set);
  });
});

describe('Simple state setters', () => {
  function makeStore() {
    return createAppStore({ kiro: new Kiro() });
  }

  it('setProcessing(true) sets isProcessing', () => {
    const store = makeStore();
    expect(store.getState().isProcessing).toBe(false);
    store.getState().setProcessing(true);
    expect(store.getState().isProcessing).toBe(true);
  });

  it('setProcessing(false) clears isProcessing', () => {
    const store = makeStore();
    store.getState().setProcessing(true);
    store.getState().setProcessing(false);
    expect(store.getState().isProcessing).toBe(false);
  });

  it('setGoalStatus only turn-owns rows emitted during processing', () => {
    const store = makeStore();
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'completed prompt' },
        { id: 'm1', role: MessageRole.Model, content: 'completed response' },
      ],
    });

    store.getState().setGoalStatus({
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'IDLE_GOAL_STATUS_AFTER_TURN',
    });

    let statusRow = store.getState().messages.at(-1) as
      | { role: MessageRole.System; turnOwned?: boolean }
      | undefined;
    expect(statusRow?.role).toBe(MessageRole.System);
    expect(statusRow?.turnOwned).not.toBe(true);

    store.setState({ goalStatus: null, isProcessing: true });
    store.getState().setGoalStatus({
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'IN_FLIGHT_GOAL_STATUS',
    });

    statusRow = store.getState().messages.at(-1) as
      | { role: MessageRole.System; turnOwned?: boolean }
      | undefined;
    expect(statusRow?.role).toBe(MessageRole.System);
    expect(statusRow?.turnOwned).toBe(true);
  });

  it('setAgentError sets error and guidance', () => {
    const store = makeStore();
    store.getState().setAgentError('something broke', 'try again');
    expect(store.getState().agentError).toBe('something broke');
    expect(store.getState().agentErrorGuidance).toBe('try again');
  });

  it('setAgentError with null clears error and guidance', () => {
    const store = makeStore();
    store.getState().setAgentError('err', 'guide');
    store.getState().setAgentError(null);
    expect(store.getState().agentError).toBeNull();
    expect(store.getState().agentErrorGuidance).toBeNull();
  });

  it('setCurrentModel sets the model', () => {
    const store = makeStore();
    expect(store.getState().currentModel).toBeNull();
    const model = { id: 'model-1', name: 'Claude' };
    store.getState().setCurrentModel(model);
    expect(store.getState().currentModel).toEqual(model);
  });

  it('setCurrentModel(null) clears the model', () => {
    const store = makeStore();
    store.getState().setCurrentModel({ id: 'x', name: 'y' });
    store.getState().setCurrentModel(null);
    expect(store.getState().currentModel).toBeNull();
  });

  it('clearMessages keeps only the last turn', () => {
    const store = makeStore();
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'first' },
        { id: 'm1', role: MessageRole.Model, content: 'reply1' },
        { id: 'u2', role: MessageRole.User, content: 'second' },
        { id: 'm2', role: MessageRole.Model, content: 'reply2' },
      ],
    });
    store.getState().clearMessages();
    const msgs = store.getState().messages;
    expect(msgs).toHaveLength(2);
    expect(msgs[0]!.id).toBe('u2');
    expect(msgs[1]!.id).toBe('m2');
  });

  it('clearMessages does nothing when fewer than 2 messages', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'only' }],
    });
    store.getState().clearMessages();
    expect(store.getState().messages).toHaveLength(1);
  });

  it('clearMessages does nothing when no user messages exist', () => {
    const store = makeStore();
    store.setState({
      messages: [
        { id: 'm1', role: MessageRole.Model, content: 'a' },
        { id: 'm2', role: MessageRole.Model, content: 'b' },
      ],
    });
    store.getState().clearMessages();
    expect(store.getState().messages).toHaveLength(2);
  });

  it('showTransientAlert sets the alert', () => {
    const store = makeStore();
    expect(store.getState().transientAlert).toBeNull();
    const alert = { message: 'Done!', status: 'success' as const };
    store.getState().showTransientAlert(alert);
    expect(store.getState().transientAlert).toEqual(alert);
  });

  it('dismissTransientAlert clears the alert', () => {
    const store = makeStore();
    store
      .getState()
      .showTransientAlert({ message: 'hi', status: 'info' as const });
    store.getState().dismissTransientAlert();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('setShowHelpPanel toggles panel and sets commands', () => {
    const store = makeStore();
    expect(store.getState().showHelpPanel).toBe(false);
    const cmds = [{ name: '/help', description: 'Show help' }];
    store.getState().setShowHelpPanel(true, cmds as any);
    expect(store.getState().showHelpPanel).toBe(true);
    expect(store.getState().helpCommands).toEqual(cmds as any);
  });

  it('setShowHelpPanel defaults commands to empty array', () => {
    const store = makeStore();
    store.getState().setShowHelpPanel(true);
    expect(store.getState().showHelpPanel).toBe(true);
    expect(store.getState().helpCommands).toEqual([]);
  });

  it('setShowUsagePanel toggles panel and sets data', () => {
    const store = makeStore();
    const usageData = { total: 100 } as any;
    store.getState().setShowUsagePanel(true, usageData);
    expect(store.getState().showUsagePanel).toBe(true);
    expect(store.getState().usageData).toEqual(usageData);
  });

  it('setShowUsagePanel with no data sets usageData to null', () => {
    const store = makeStore();
    store.getState().setShowUsagePanel(true);
    expect(store.getState().showUsagePanel).toBe(true);
    expect(store.getState().usageData).toBeNull();
  });

  it('setShowMcpPanel sets panel state with defaults', () => {
    const store = makeStore();
    store.getState().setShowMcpPanel(true);
    expect(store.getState().showMcpPanel).toBe(true);
    expect(store.getState().mcpServers).toEqual([]);
    expect(store.getState().mcpMode).toBe('list');
    expect(store.getState().mcpRegistryServers).toEqual([]);
  });

  it('setShowMcpPanel sets panel state with all arguments', () => {
    const store = makeStore();
    const servers = [{ name: 'srv1' }] as any;
    const registry = [{ name: 'reg1' }] as any;
    store.getState().setShowMcpPanel(true, servers, 'add', registry);
    expect(store.getState().showMcpPanel).toBe(true);
    expect(store.getState().mcpServers).toEqual(servers);
    expect(store.getState().mcpMode).toBe('add');
    expect(store.getState().mcpRegistryServers).toEqual(registry);
  });

  it('setContextUsage sets contextUsagePercent', () => {
    const store = makeStore();
    expect(store.getState().contextUsagePercent).toBeNull();
    store.getState().setContextUsage(75);
    expect(store.getState().contextUsagePercent).toBe(75);
  });

  it('clearInput resets input buffer to initial state', () => {
    const store = makeStore();
    store.getState().insert('hello');
    expect(store.getState().input.lines[0]).toBe('hello');
    store.getState().clearInput();
    expect(store.getState().input.lines).toEqual(['']);
    expect(store.getState().input.cursorRow).toBe(0);
    expect(store.getState().input.cursorCol).toBe(0);
  });

  it('setHasExpandableToolOutputs sets the flag', () => {
    const store = makeStore();
    expect(store.getState().hasExpandableToolOutputs).toBe(false);
    store.getState().setHasExpandableToolOutputs(true);
    expect(store.getState().hasExpandableToolOutputs).toBe(true);
    store.getState().setHasExpandableToolOutputs(false);
    expect(store.getState().hasExpandableToolOutputs).toBe(false);
  });

  it('confirmTrustAllTools sets the confirmed flag', () => {
    const store = makeStore();
    expect(store.getState().trustAllToolsConfirmed).toBe(false);
    store.getState().confirmTrustAllTools();
    expect(store.getState().trustAllToolsConfirmed).toBe(true);
  });

  it('addPendingImage appends to pendingImages', () => {
    const store = makeStore();
    expect(store.getState().pendingImages).toEqual([]);
    const img1 = {
      base64: 'abc',
      mimeType: 'image/png',
      width: 100,
      height: 100,
      sizeBytes: 1024,
    };
    const img2 = {
      base64: 'def',
      mimeType: 'image/jpeg',
      width: 200,
      height: 200,
      sizeBytes: 2048,
    };
    store.getState().addPendingImage(img1);
    store.getState().addPendingImage(img2);
    expect(store.getState().pendingImages).toEqual([img1, img2]);
  });

  it('removePendingImage removes by index', () => {
    const store = makeStore();
    const img1 = {
      base64: 'a',
      mimeType: 'image/png',
      width: 10,
      height: 10,
      sizeBytes: 100,
    };
    const img2 = {
      base64: 'b',
      mimeType: 'image/jpeg',
      width: 20,
      height: 20,
      sizeBytes: 200,
    };
    const img3 = {
      base64: 'c',
      mimeType: 'image/gif',
      width: 30,
      height: 30,
      sizeBytes: 300,
    };
    store.getState().addPendingImage(img1);
    store.getState().addPendingImage(img2);
    store.getState().addPendingImage(img3);
    store.getState().removePendingImage(1);
    expect(store.getState().pendingImages).toEqual([img1, img3]);
  });

  it('clearPendingImages empties the array', () => {
    const store = makeStore();
    store.getState().addPendingImage({
      base64: 'x',
      mimeType: 'image/png',
      width: 50,
      height: 50,
      sizeBytes: 500,
    });
    store.getState().clearPendingImages();
    expect(store.getState().pendingImages).toEqual([]);
  });
});

describe('RetryWarning event handling', () => {
  function createStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });
    return store;
  }

  it('sets retryStatus and does not emit a transient alert', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler!({
      type: AgentEventType.RetryWarning,
      attempt: 2,
      maxAttempts: 6,
      delaySecs: 5.0,
      message: 'Retrying in 5s (attempt 2/6)',
    });

    const retry = store.getState().retryStatus;
    expect(retry).not.toBeNull();
    expect(retry?.attempt).toBe(2);
    expect(retry?.maxAttempts).toBe(6);
    expect(retry?.delaySecs).toBe(5.0);
    expect(retry?.message).toBe('Retrying in 5s (attempt 2/6)');

    // The previous UX (transient alert) is intentionally not used — the retry is
    // rendered inline with the "Thinking..." spinner instead.
    expect(store.getState().transientAlert).toBeNull();
  });

  it('clears retryStatus when a non-retry stream event arrives', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    // First, fire a retry warning to set the banner.
    handler!({
      type: AgentEventType.RetryWarning,
      attempt: 2,
      maxAttempts: 6,
      delaySecs: 5.0,
      message: 'Retrying in 5s (attempt 2/6)',
    });
    expect(store.getState().retryStatus).not.toBeNull();

    // Any non-retry event means the SDK finished its backoff — the banner is stale.
    handler!({
      type: AgentEventType.Content,
      id: 'msg-1',
      content: { type: ContentType.Text, text: 'hello' },
    });
    expect(store.getState().retryStatus).toBeNull();
  });

  it('subsequent retry overwrites previous retry status', () => {
    const store = createStore();
    const handler = store.getState().createStreamEventHandler();

    handler!({
      type: AgentEventType.RetryWarning,
      attempt: 2,
      maxAttempts: 6,
      delaySecs: 1.0,
      message: 'Retrying in 1s (attempt 2/6)',
    });
    expect(store.getState().retryStatus?.attempt).toBe(2);

    // Third attempt fires — should overwrite, not accumulate.
    handler!({
      type: AgentEventType.RetryWarning,
      attempt: 3,
      maxAttempts: 6,
      delaySecs: 2.0,
      message: 'Retrying in 2s (attempt 3/6)',
    });
    const retry = store.getState().retryStatus;
    expect(retry?.attempt).toBe(3);
    expect(retry?.delaySecs).toBe(2.0);
    expect(retry?.message).toBe('Retrying in 2s (attempt 3/6)');
  });
});

describe('reopenSettingsMenu', () => {
  // This action is called by ESC handlers when a /settings-derived overlay
  // is dismissed — going back one level to the top-level /settings overlay
  // instead of closing entirely. After the SettingsPanel rewrite this is
  // a single store flag flip; the panel itself owns row rendering and
  // navigation.
  it('opens the SettingsPanel by setting showSettingsPanel=true', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    expect(store.getState().showSettingsPanel).toBe(false);

    store.getState().reopenSettingsMenu();

    expect(store.getState().showSettingsPanel).toBe(true);
  });

  it('does not touch activeCommand', () => {
    // Old behaviour set activeCommand to a synthetic /settings menu —
    // that path is gone, replaced by the SettingsPanel overlay. Make
    // sure we don't accidentally re-introduce activeCommand coupling.
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    store.getState().reopenSettingsMenu();

    // tui mode: reopenSettingsMenu opens main's SettingsPanel overlay, not
    // the lite command-menu, so activeCommand stays null.
    expect(store.getState().activeCommand).toBeNull();
  });

  it('opens the same SettingsPanel in lite mode (1:1 with TUI)', () => {
    // Lite now renders the shared SettingsPanel (via <BackendPanels>) rather
    // than a bespoke command-menu, so reopenSettingsMenu flips the same flag
    // in both modes. The lite-only verbosity row is gated inside the panel's
    // model (settings-panel-model.ts), not via activeCommand.
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro, uiMode: 'lite' });

    store.getState().reopenSettingsMenu();

    expect(store.getState().showSettingsPanel).toBe(true);
    // No command-menu coupling in either mode.
    expect(store.getState().activeCommand).toBeNull();
  });
});

describe('isSubagentTool flag survives ToolCall create → ToolCall update → ToolCallFinished', () => {
  function createStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });
    return store;
  }

  it('preserves isSubagentTool through update and finished rebuilds', async () => {
    const store = createStore();
    // Pre-populate a subagent session so sessionId resolves
    store.setState({
      sessions: new Map([
        [
          'subagent-session-1',
          { name: 'worker', type: 'ephemeral', status: 'running' } as any,
        ],
      ]),
    });

    const handler = store.getState().createStreamEventHandler();

    // Create: subagent tool call (has sessionId → isSubagentTool:true)
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-1',
      name: 'bash',
      args: { command: 'echo hi' },
      sessionId: 'subagent-session-1',
    });

    const afterCreate = store
      .getState()
      .messages.find((m) => m.id === 'tool-1');
    expect(afterCreate?.role).toBe(MessageRole.ToolUse);
    expect((afterCreate as any).isSubagentTool).toBe(true);

    // Update: re-emit with new args (existingIndex path)
    handler({
      type: AgentEventType.ToolCall,
      id: 'tool-1',
      name: 'bash',
      args: { command: 'echo hi', extra: true },
      sessionId: 'subagent-session-1',
    });

    const afterUpdate = store
      .getState()
      .messages.find((m) => m.id === 'tool-1');
    expect((afterUpdate as any).isSubagentTool).toBe(true);

    // Finished: must not drop isSubagentTool
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-1',
      result: { status: 'success', output: '' },
    });

    const afterFinished = store
      .getState()
      .messages.find((m) => m.id === 'tool-1');
    expect((afterFinished as any).isSubagentTool).toBe(true);
    expect((afterFinished as any).isFinished).toBe(true);
  });
});

describe('setShowToolsPanel — cache preservation', () => {
  it('preserves toolsList when closing the panel (no tools arg)', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const tools = [
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: 'write', source: 'builtin', description: 'write tools' },
    ];
    // Notification populates the cache.
    store.getState().setToolsList(tools);
    // Open with the cached snapshot, then close (no tools arg).
    store.getState().setShowToolsPanel(true, tools);
    store.getState().setShowToolsPanel(false);
    // Cache must survive close so a later open (without a new push) still shows it.
    expect(store.getState().showToolsPanel).toBe(false);
    expect(store.getState().toolsList).toEqual(tools);
  });

  it('replaces toolsList when tools are explicitly provided', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store
      .getState()
      .setToolsList([{ name: 'read', source: 'builtin', description: 'r' }]);
    store
      .getState()
      .setShowToolsPanel(true, [
        { name: 'shell', source: 'builtin', description: 's' },
      ]);
    expect(store.getState().toolsList).toEqual([
      { name: 'shell', source: 'builtin', description: 's' },
    ]);
  });
});

describe('setShowHooksPanel — cache preservation', () => {
  it('preserves hooksList when closing the panel without hook data', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const hooks: HookInfo[] = [
      { trigger: 'preToolUse', matcher: 'write', command: 'validate.sh' },
    ];

    store.getState().setShowHooksPanel(true, hooks);
    store.getState().setShowHooksPanel(false);

    expect(store.getState().showHooksPanel).toBe(false);
    expect(store.getState().hooksList).toEqual(hooks);
  });

  it('replaces hooksList when hooks are explicitly provided', () => {
    const store = createAppStore({ kiro: new Kiro() });

    store
      .getState()
      .setShowHooksPanel(true, [
        { trigger: 'agentSpawn', command: 'git status' },
      ]);

    expect(store.getState().hooksList).toEqual([
      { trigger: 'agentSpawn', command: 'git status' },
    ]);
  });
});

describe('resetClientDisplayCaches', () => {
  it('clears every display snapshot derived from the transport client', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.setState({
      contextBreakdownCache: {
        contextFiles: { percent: 1, tokens: 1 },
        tools: { percent: 1, tokens: 1 },
        kiroResponses: { percent: 1, tokens: 1 },
        yourPrompts: { percent: 1, tokens: 1 },
      },
      mcpServerCache: [{ name: 'configured', status: 'running', toolCount: 1 }],
      mcpRegistryCache: [
        { name: 'registry', status: 'disabled', toolCount: 0 },
      ],
      toolsList: [{ name: 'tool', source: 'builtin', description: 'A tool' }],
      hooksList: [{ name: 'hook', trigger: 'promptSubmit', command: 'run' }],
    });

    store.getState().resetClientDisplayCaches();

    expect(store.getState()).toMatchObject({
      contextBreakdownCache: null,
      mcpServerCache: [],
      mcpRegistryCache: [],
      toolsList: [],
      hooksList: [],
    });
  });
});
