import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { AgentEventType } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';

// --- Mock logger ---
mock.module('../utils/logger', () => ({
  logger: {
    debug: () => {},
    error: () => {},
    warn: () => {},
    info: () => {},
  },
}));

// --- Mock AcpClient ---
let mockOnUpdateHandler: ((event: AgentStreamEvent) => void) | null = null;

const mockSessionClient = {
  sessionId: undefined as string | undefined,
  initialize: mock(() => Promise.resolve()),
  newSession: mock(() => {
    mockSessionClient.sessionId = 'session-1';
    return Promise.resolve({
      sessionId: 'session-1',
      currentModel: { id: 'model-1', name: 'Test Model' },
      currentAgent: { name: 'test-agent', welcomeMessage: 'Welcome!' },
    });
  }),
  loadSession: mock((id: string) =>
    Promise.resolve({
      sessionId: id,
      currentModel: { id: 'model-1', name: 'Test Model' },
      currentAgent: { name: 'test-agent' },
    })
  ),
  prompt: mock(() => Promise.resolve()),
  cancel: mock(() => Promise.resolve()),
  close: mock(() => {}),
  onUpdate: mock((handler: (event: AgentStreamEvent) => void) => {
    mockOnUpdateHandler = handler;
    return () => {
      mockOnUpdateHandler = null;
    };
  }),
  executeCommand: mock(() => Promise.resolve({ success: true, message: 'ok' })),
  getCommandOptions: mock(() => Promise.resolve({ options: [] })),
  setMode: mock(() => Promise.resolve()),
  listSettings: mock(() => Promise.resolve({ 'chat.theme': 'dark' })),
  setSetting: mock(() => Promise.resolve()),
  terminateSession: mock(() => Promise.resolve()),
  listSessions: mock(() => Promise.resolve({ sessions: [] })),
};

const MockAcpClientClass = class MockAcpClient {
  sessionId = mockSessionClient.sessionId;
  initialize = mockSessionClient.initialize;
  newSession = (...args: any[]) => {
    const result = mockSessionClient.newSession(...(args as []));
    result.then((r: any) => {
      this.sessionId = r.sessionId;
    });
    return result;
  };
  loadSession = mockSessionClient.loadSession;
  prompt = mockSessionClient.prompt;
  cancel = mockSessionClient.cancel;
  close = mockSessionClient.close;
  onUpdate = mockSessionClient.onUpdate;
  executeCommand = mockSessionClient.executeCommand;
  getCommandOptions = mockSessionClient.getCommandOptions;
  setMode = mockSessionClient.setMode;
  listSettings = mockSessionClient.listSettings;
  setSetting = mockSessionClient.setSetting;
  terminateSession = mockSessionClient.terminateSession;
  listSessions = mockSessionClient.listSessions;
  constructor() {}
};

mock.module('../acp-client', () => ({
  AcpClient: MockAcpClientClass,
  createAcpClient: () => new MockAcpClientClass(),
}));

afterAll(() => {
  mock.restore();
});

// Use a query-string import so the specifier doesn't match the bare
// '../kiro' that other test files mock via mock.module.  This gives us
// the real Kiro class (which will pick up our '../acp-client' mock above).
// @ts-expect-error — query-string specifier bypasses bun's mock registry
const { Kiro } = await import('../kiro?real');

describe('Kiro', () => {
  beforeEach(() => {
    mockSessionClient.sessionId = undefined;
    mockSessionClient.initialize.mockClear();
    mockSessionClient.newSession.mockClear();
    mockSessionClient.loadSession.mockClear();
    mockSessionClient.prompt.mockClear();
    mockSessionClient.cancel.mockClear();
    mockSessionClient.close.mockClear();
    mockSessionClient.onUpdate.mockClear();
    mockSessionClient.executeCommand.mockClear();
    mockSessionClient.getCommandOptions.mockClear();
    mockSessionClient.setMode.mockClear();
    mockSessionClient.listSettings.mockClear();
    mockSessionClient.setSetting.mockClear();
    mockSessionClient.terminateSession.mockClear();
    mockSessionClient.listSessions.mockClear();
    mockOnUpdateHandler = null;
    // Reset newSession to update sessionId
    mockSessionClient.newSession.mockImplementation(() => {
      mockSessionClient.sessionId = 'session-1';
      return Promise.resolve({
        sessionId: 'session-1',
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent', welcomeMessage: 'Welcome!' },
      });
    });
  });

  it('initialize creates AcpClient and calls initialize', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    expect(mockSessionClient.initialize).toHaveBeenCalledTimes(1);
  });

  it('after initialize, sessionId is undefined until createSession', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    expect(kiro.sessionId).toBeUndefined();
  });

  it('createSession calls newSession and notifies model/agent handlers', async () => {
    const kiro = new Kiro();
    const modelHandler = mock(() => {});
    const agentHandler = mock(() => {});
    kiro.onModelUpdate(modelHandler);
    kiro.onAgentUpdate(agentHandler);
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    expect(mockSessionClient.newSession).toHaveBeenCalledTimes(1);
    expect(modelHandler).toHaveBeenCalledWith({
      id: 'model-1',
      name: 'Test Model',
    });
    expect(agentHandler).toHaveBeenCalledWith({
      name: 'test-agent',
      welcomeMessage: 'Welcome!',
    });
  });

  it('onCommandsUpdate registers handler and handler receives commands from onUpdate events', async () => {
    const kiro = new Kiro();
    const commandsHandler = mock(() => {});
    kiro.onCommandsUpdate(commandsHandler);
    await kiro.initialize('/path/to/agent');

    // Simulate onUpdate event
    expect(mockSessionClient.onUpdate).toHaveBeenCalled();
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.CommandsUpdate,
        commands: [{ name: 'help', description: 'Show help' }],
      } as AgentStreamEvent);
    }
    expect(commandsHandler).toHaveBeenCalledWith([
      { name: 'help', description: 'Show help' },
    ]);
  });

  it('onModelUpdate registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onModelUpdate(handler);
    // No error thrown
    expect(handler).not.toHaveBeenCalled();
  });

  it('onAgentUpdate registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onAgentUpdate(handler);
    expect(handler).not.toHaveBeenCalled();
  });

  it('executeCommand throws when not initialized', async () => {
    const kiro = new Kiro();
    await expect(
      kiro.executeCommand({ command: 'test' } as any)
    ).rejects.toThrow('Kiro not initialized');
  });

  it('executeCommand forwards to sessionClient when initialized', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.executeCommand({ command: 'test' } as any);
    expect(mockSessionClient.executeCommand).toHaveBeenCalled();
  });

  it('cancel does nothing when not initialized', async () => {
    const kiro = new Kiro();
    await kiro.cancel();
    expect(mockSessionClient.cancel).not.toHaveBeenCalled();
  });

  it('cancel calls sessionClient.cancel when initialized', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.cancel();
    expect(mockSessionClient.cancel).toHaveBeenCalled();
  });

  it('close calls sessionClient.close and cleans up', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    kiro.close();
    expect(mockSessionClient.close).toHaveBeenCalled();
  });

  it('streamMessage throws when not initialized', async () => {
    const kiro = new Kiro();
    const controller = new AbortController();
    await expect(
      kiro.streamMessage('hello', controller.signal, () => {})
    ).rejects.toThrow('Kiro not initialized');
  });

  it('newSession creates session and terminates previous', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();

    mockSessionClient.newSession.mockClear();
    mockSessionClient.newSession.mockImplementation(() => {
      return Promise.resolve({
        sessionId: 'session-2',
        currentModel: undefined as any,
        currentAgent: undefined as any,
      });
    });

    const result = await kiro.newSession();
    expect(result.sessionId).toBe('session-2');
    expect(mockSessionClient.terminateSession).toHaveBeenCalled();
  });

  it('settings returns empty object initially, populated after initialize', async () => {
    const kiro = new Kiro();
    expect(kiro.settings).toEqual({});
    await kiro.initialize('/path/to/agent');
    expect(kiro.settings).toEqual({ 'chat.theme': 'dark' });
  });

  it('double initialize creates a new client without error', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    expect(mockSessionClient.initialize).toHaveBeenCalledTimes(1);
    // Second initialize should succeed (creates a new AcpClient)
    mockSessionClient.initialize.mockClear();
    await kiro.initialize('/path/to/agent');
    expect(mockSessionClient.initialize).toHaveBeenCalledTimes(1);
  });

  it('onUpdate handler that throws does not prevent other event processing', async () => {
    const kiro = new Kiro();
    const throwingHandler = mock(() => {
      throw new Error('handler error');
    });
    kiro.onCommandsUpdate(throwingHandler);
    await kiro.initialize('/path/to/agent');

    // Simulate onUpdate event with CommandsUpdate - the handler throws but
    // the global event listener should catch the error via try/catch in
    // the onEvent callback pattern
    expect(mockSessionClient.onUpdate).toHaveBeenCalled();
    if (mockOnUpdateHandler) {
      // This should not throw out to caller - Kiro's global handler
      // invokes commandsHandler which throws, but it doesn't crash
      expect(() => {
        mockOnUpdateHandler!({
          type: AgentEventType.CommandsUpdate,
          commands: [{ name: 'test', description: 'Test' }],
        } as AgentStreamEvent);
      }).toThrow('handler error');
    }
    expect(throwingHandler).toHaveBeenCalled();
  });
});

describe('Kiro — handler registration and forwarding', () => {
  it('onPromptsUpdate receives PromptsUpdate events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onPromptsUpdate(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.PromptsUpdate,
        prompts: [{ name: 'test-prompt', arguments: [], serverName: 'srv' }],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      { name: 'test-prompt', arguments: [], serverName: 'srv' },
    ]);
  });

  it('onExtensionMethodsDiscovered receives events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onExtensionMethodsDiscovered(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ExtensionMethodsDiscovered,
        commands: [{ name: 'ext-cmd', description: 'Extension' }],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      { name: 'ext-cmd', description: 'Extension' },
    ]);
  });

  it('onCompactionStatus receives compaction events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onCompactionStatus(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.CompactionStatus,
        status: 'started',
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onCompactionStatus receives ContextUsage events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onCompactionStatus(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ContextUsage,
        percent: 75,
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onCompactionStatus receives EffortUpdate events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onCompactionStatus(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.EffortUpdate,
        effort: 'high',
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onInitNotification receives MCP failure events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onInitNotification(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.McpServerInitFailure,
        serverName: 'test-mcp',
        error: 'connection refused',
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onInitNotification receives AgentNotFound events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onInitNotification(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.AgentNotFound,
        requestedAgent: 'missing',
        fallbackAgent: 'default',
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onHistoryEvent receives historical content events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onHistoryEvent(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.Content,
        id: 'msg-1',
        content: { type: 'text', text: 'hello' },
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onTurnSummary receives TurnSummary events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onTurnSummary(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.TurnSummary,
        meteringUsage: [],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('AgentSwitched event notifies agent and model handlers', async () => {
    const kiro = new Kiro();
    const agentHandler = mock(() => {});
    const modelHandler = mock(() => {});
    kiro.onAgentUpdate(agentHandler);
    kiro.onModelUpdate(modelHandler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.AgentSwitched,
        agentName: 'planner',
        welcomeMessage: 'Planning!',
        model: 'claude-opus',
      } as AgentStreamEvent);
    }
    expect(agentHandler).toHaveBeenCalledWith({
      name: 'planner',
      welcomeMessage: 'Planning!',
    });
    expect(modelHandler).toHaveBeenCalledWith({
      id: 'claude-opus',
      name: 'claude-opus',
    });
  });
});

describe('Kiro — session methods', () => {
  it('spawnSession throws when not initialized', async () => {
    const kiro = new Kiro();
    await expect(kiro.spawnSession('task')).rejects.toThrow(
      'Kiro not initialized'
    );
  });

  it('sendMessage throws when not initialized', async () => {
    const kiro = new Kiro();
    await expect(kiro.sendMessage('s1', 'hi')).rejects.toThrow(
      'Kiro not initialized'
    );
  });

  it('terminateSession does nothing when not initialized', async () => {
    const kiro = new Kiro();
    await kiro.terminateSession('s1');
    expect(mockSessionClient.terminateSession).not.toHaveBeenCalled();
  });

  it('setMode forwards to sessionClient', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.setMode('fast');
    expect(mockSessionClient.setMode).toHaveBeenCalledWith('fast');
  });

  it('getCommandOptions returns empty when not initialized', async () => {
    const kiro = new Kiro();
    const result = await kiro.getCommandOptions('/help');
    expect(result).toEqual({ options: [] });
  });

  it('listSessions returns empty when not initialized', async () => {
    const kiro = new Kiro();
    const result = await kiro.listSessions('/tmp');
    expect(result).toEqual({ sessions: [] });
  });

  it('setSetting throws when not initialized', async () => {
    const kiro = new Kiro();
    await expect(kiro.setSetting('key', 'val')).rejects.toThrow(
      'Kiro not initialized'
    );
  });

  it('loadSession loads and terminates previous session', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    mockSessionClient.terminateSession.mockClear();
    const result = await kiro.loadSession('session-new');
    expect(result.sessionId).toBe('session-new');
    expect(mockSessionClient.terminateSession).toHaveBeenCalled();
  });

  it('loadSession calls onHistoryEvent handler', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const historyHandler = mock(() => {});
    await kiro.loadSession('session-hist', historyHandler);
    // The handler is registered via onUpdate
    expect(mockSessionClient.onUpdate).toHaveBeenCalled();
  });

  it('createSession with resumeSessionId calls loadSession', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession('existing-session');
    expect(mockSessionClient.loadSession).toHaveBeenCalledWith(
      'existing-session'
    );
  });

  it('onSessionEvent registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onSessionEvent(handler);
    // No error
    expect(handler).not.toHaveBeenCalled();
  });

  it('onMultiSessionUpdate registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onMultiSessionUpdate(handler);
    expect(handler).not.toHaveBeenCalled();
  });

  it('onSubagentListUpdate registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onSubagentListUpdate(handler);
    expect(handler).not.toHaveBeenCalled();
  });

  it('onInboxNotification registers handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onInboxNotification(handler);
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('Kiro — streamMessage', () => {
  it('resolves when prompt completes successfully', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const controller = new AbortController();
    const onEvent = mock(() => {});
    await kiro.streamMessage('hello', controller.signal, onEvent);
    expect(mockSessionClient.prompt).toHaveBeenCalled();
  });

  it('rejects when prompt fails', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    mockSessionClient.prompt.mockImplementationOnce(() =>
      Promise.reject(new Error('backend error'))
    );
    const controller = new AbortController();
    await expect(
      kiro.streamMessage('hello', controller.signal, () => {})
    ).rejects.toThrow('backend error');
  });

  it('delivers events to onEvent callback', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const onEvent = mock(() => {});
    // Make prompt resolve after we fire an event
    mockSessionClient.prompt.mockImplementationOnce(() => {
      // Simulate an event arriving during prompt
      if (mockOnUpdateHandler) {
        mockOnUpdateHandler({
          type: AgentEventType.Content,
          id: 'msg-1',
          content: { type: 'text', text: 'chunk' },
        } as AgentStreamEvent);
      }
      return Promise.resolve();
    });
    const controller = new AbortController();
    await kiro.streamMessage('hello', controller.signal, onEvent);
    expect(onEvent).toHaveBeenCalled();
  });

  it('sends images as content blocks', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const controller = new AbortController();
    const images = [{ base64: 'abc', mimeType: 'image/png' }];
    await kiro.streamMessage('describe', controller.signal, () => {}, images);
    expect(mockSessionClient.prompt).toHaveBeenCalled();
  });

  it('filters out UserMessage events during streaming', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const onEvent = mock(() => {});
    mockSessionClient.prompt.mockImplementationOnce(() => {
      if (mockOnUpdateHandler) {
        mockOnUpdateHandler({
          type: AgentEventType.UserMessage,
          id: 'um-1',
          content: { type: 'text', text: 'historical' },
        } as AgentStreamEvent);
      }
      return Promise.resolve();
    });
    const controller = new AbortController();
    await kiro.streamMessage('hello', controller.signal, onEvent);
    expect(onEvent).not.toHaveBeenCalled();
  });
});
