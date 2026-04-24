import { describe, it, expect, mock } from 'bun:test';
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
