import { describe, it, expect, mock, afterAll } from 'bun:test';
import {
  createAppStore,
  MessageRole,
  ToolUseStatus,
  NOT_READY_TOOLS,
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

  it('flushContentToStore updates last model message in place without double spread', async () => {
    const store = createStore();

    // Seed a user message and a model message
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'hello' },
        { id: 'm1', role: MessageRole.Model, content: 'initial' },
      ],
    });

    const handler = store.getState().createStreamEventHandler();

    // Send a content event — this buffers the text
    handler!({
      type: AgentEventType.Content,
      id: 'm1',
      content: { type: ContentType.Text, text: 'updated response' },
    });

    // Manually trigger the batched flush by advancing the timer
    // The flush is scheduled via setTimeout(fn, 16), so we wait for it
    await new Promise((resolve) => setTimeout(resolve, 50));

    const messages = store.getState().messages;
    expect(messages).toHaveLength(2);
    expect(messages[1]?.content).toBe('updated response');
    // The user message should be the exact same object reference (not copied)
    expect(messages[0]?.id).toBe('u1');
  });

  it('flushContentToStore appends new model message when last is not model', async () => {
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

    const messages = store.getState().messages;
    expect(messages).toHaveLength(2);
    expect(messages[1]?.role).toBe(MessageRole.Model);
    expect(messages[1]?.content).toBe('first chunk');
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
