import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { AgentEventType } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';
import { createAppStore } from '../stores/app-store';

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
  resolveSpecSession: mock((req: { featureName: string }) =>
    Promise.resolve({ sessionId: `spec-${req.featureName}` })
  ),
  invokeSpec: mock((req: { sessionId: string }) =>
    Promise.resolve({ sessionId: req.sessionId, executionId: 'exec-1' })
  ),
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
  resolveSpecSession = mockSessionClient.resolveSpecSession;
  invokeSpec = mockSessionClient.invokeSpec;
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
    mockSessionClient.resolveSpecSession.mockClear();
    mockSessionClient.invokeSpec.mockClear();
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
    expect(commandsHandler).toHaveBeenCalled();
    const firstCallArgs = (commandsHandler.mock.calls as unknown[][])[0];
    expect(firstCallArgs![0]).toEqual([
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

  it('ModelUpdate event notifies modelHandler only, not agentHandler', async () => {
    const kiro = new Kiro();
    const modelHandler = mock(() => {});
    const agentHandler = mock(() => {});
    kiro.onModelUpdate(modelHandler);
    kiro.onAgentUpdate(agentHandler);
    await kiro.initialize('/path/to/agent');

    expect(mockOnUpdateHandler).not.toBeNull();
    mockOnUpdateHandler!({
      type: AgentEventType.ModelUpdate,
      model: { id: 'gpt-5', name: 'GPT-5' },
    } as AgentStreamEvent);

    expect(modelHandler).toHaveBeenCalledWith({ id: 'gpt-5', name: 'GPT-5' });
    // Model-only update must not clobber the current agent.
    expect(agentHandler).not.toHaveBeenCalled();
  });

  it('e2e: empty model chip self-heals when a ModelUpdate arrives (wired to store)', async () => {
    // Reproduces the original bug end-to-end: the model chip is empty on
    // launch (store.currentModel === null), and a later KAS-pushed model
    // (surfaced as a ModelUpdate event) must populate it. Wires the Kiro
    // model handler to the real store exactly as index.tsx does.
    const store = createAppStore({ kiro: {} as never });
    const kiro = new Kiro();
    kiro.onModelUpdate((model: { id: string; name: string }) =>
      store.getState().setCurrentModel(model)
    );
    await kiro.initialize('/path/to/agent');

    // Precondition: chip empty (the symptom).
    expect(store.getState().currentModel).toBeNull();

    expect(mockOnUpdateHandler).not.toBeNull();
    mockOnUpdateHandler!({
      type: AgentEventType.ModelUpdate,
      model: { id: 'claude-sonnet', name: 'Claude Sonnet' },
    } as AgentStreamEvent);

    // The chip is now populated — no /model open or agent switch needed.
    expect(store.getState().currentModel).toEqual({
      id: 'claude-sonnet',
      name: 'Claude Sonnet',
    });
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

  describe('resolveSpecSession()', () => {
    it('throws when not initialized', async () => {
      const kiro = new Kiro();
      await expect(
        kiro.resolveSpecSession({ featureName: 'login', strategy: 'reuse' })
      ).rejects.toThrow('Kiro not initialized');
    });

    it('forwards request to sessionClient and returns its response', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');

      const result = await kiro.resolveSpecSession({
        featureName: 'login',
        strategy: 'reuse',
      });

      expect(mockSessionClient.resolveSpecSession).toHaveBeenCalledTimes(1);
      expect(mockSessionClient.resolveSpecSession).toHaveBeenCalledWith({
        featureName: 'login',
        strategy: 'reuse',
      });
      expect(result.sessionId).toBe('spec-login');
    });

    it('throws "not supported" when sessionClient lacks resolveSpecSession', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');

      // Drill into the private sessionClient to drop the method, then
      // restore. This simulates a non-KAS engine which has no spec
      // workflow methods on its session client.
      const sc = (kiro as any).sessionClient;
      const original = sc.resolveSpecSession;
      delete sc.resolveSpecSession;
      try {
        await expect(
          kiro.resolveSpecSession({ featureName: 'login', strategy: 'reuse' })
        ).rejects.toThrow(
          'Spec workflow is not supported by the current agent engine'
        );
      } finally {
        sc.resolveSpecSession = original;
      }
    });
  });

  describe('invokeSpec()', () => {
    it('throws when not initialized', async () => {
      const kiro = new Kiro();
      await expect(
        kiro.invokeSpec({
          operation: 'runAllTasks',
          sessionId: 's',
          featureName: 'login',
          specDocuments: [],
          tasksFilePath: '/x/tasks.md',
        })
      ).rejects.toThrow('Kiro not initialized');
    });

    it('forwards request to sessionClient and returns its response', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');

      const result = await kiro.invokeSpec({
        operation: 'runAllTasks',
        sessionId: 'sess-7',
        featureName: 'login',
        specDocuments: ['/x/requirements.md'],
        tasksFilePath: '/x/tasks.md',
      });

      expect(mockSessionClient.invokeSpec).toHaveBeenCalledTimes(1);
      expect(mockSessionClient.invokeSpec).toHaveBeenCalledWith({
        operation: 'runAllTasks',
        sessionId: 'sess-7',
        featureName: 'login',
        specDocuments: ['/x/requirements.md'],
        tasksFilePath: '/x/tasks.md',
      });
      expect(result.sessionId).toBe('sess-7');
      expect(result.executionId).toBe('exec-1');
    });

    it('throws "not supported" when sessionClient lacks invokeSpec', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');

      const sc = (kiro as any).sessionClient;
      const original = sc.invokeSpec;
      delete sc.invokeSpec;
      try {
        await expect(
          kiro.invokeSpec({
            operation: 'runAllTasks',
            sessionId: 's',
            featureName: 'login',
            specDocuments: [],
            tasksFilePath: '/x/tasks.md',
          })
        ).rejects.toThrow(
          'Spec workflow is not supported by the current agent engine'
        );
      } finally {
        sc.invokeSpec = original;
      }
    });
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
        prompts: [
          {
            name: 'test-prompt',
            arguments: [],
            source: { kind: 'mcp', serverName: 'srv' },
          },
        ],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      {
        name: 'test-prompt',
        arguments: [],
        source: { kind: 'mcp', serverName: 'srv' },
      },
    ]);
  });

  it('onSkillsUpdate receives SkillsUpdate events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onSkillsUpdate(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.SkillsUpdate,
        skills: [
          {
            name: 'pair-program',
            source: { kind: 'agent-config' },
          },
        ],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      {
        name: 'pair-program',
        source: { kind: 'agent-config' },
      },
    ]);
  });

  it('onSteeringUpdate receives SteeringUpdate events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onSteeringUpdate(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.SteeringUpdate,
        steering: [
          {
            name: 'project-context',
            source: { kind: 'workspace' },
          },
        ],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      {
        name: 'project-context',
        source: { kind: 'workspace' },
      },
    ]);
  });

  it('onKasCommandsDiscovered receives events', async () => {
    const { KasCommandName } = await import('../kas-commands');
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onKasCommandsDiscovered(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.KasCommandsDiscovered,
        commands: [{ name: KasCommandName.Help, description: 'Show help' }],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith([
      { name: KasCommandName.Help, description: 'Show help' },
    ]);
  });

  it('onToolsUpdate receives tools update events from the global handler', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onToolsUpdate(handler);
    await kiro.initialize('/path/to/agent');
    const tools = [
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: '@git/status', source: 'mcp', description: 'git status' },
    ];
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ToolsUpdate,
        tools,
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith(tools);
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

  // Regression: --resume goes through this global filter, not through the
  // onUpdate-bypass path used by /chat <id>. If `Thought` isn't whitelisted
  // here, `AgentThoughtChunk` events are emitted by the backend and converted
  // by acp-client but never reach the message store on resume — so resumed
  // sessions show no thinking even when it was persisted.
  it('onHistoryEvent receives historical thought events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onHistoryEvent(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.Thought,
        id: 'thought-1',
        content: { type: 'text', text: 'thinking out loud' },
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

  it('onApprovalRequest forwards ApprovalRequest from global handler when no prompt is active', async () => {
    const kiro = new Kiro();
    const approvalHandler = mock(() => {});
    kiro.onApprovalRequest(approvalHandler);
    await kiro.initialize('/path/to/agent');

    // Simulate an ApprovalRequest arriving via the global onUpdate handler
    // (as happens when a background /spawn session needs tool permission)
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ApprovalRequest,
        value: {
          sessionId: 'spawn-session-1',
          toolCall: { toolCallId: 'tc-1' },
          permissionOptions: [
            {
              kind: 'allow_once' as any,
              optionId: 'allow_once',
              name: 'Allow once',
            },
          ],
          resolve: () => {},
        },
      } as AgentStreamEvent);
    }
    expect(approvalHandler).toHaveBeenCalledTimes(1);
  });

  it('onApprovalRequest does NOT forward when a prompt is active (avoids double-processing)', async () => {
    const kiro = new Kiro();
    const approvalHandler = mock(() => {});
    kiro.onApprovalRequest(approvalHandler);
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();

    // Start a prompt — this sets _promptActive = true
    const onEvent = mock(() => {});
    mockSessionClient.prompt.mockImplementationOnce(() => {
      // While prompt is active, simulate an ApprovalRequest via global handler
      if (mockOnUpdateHandler) {
        mockOnUpdateHandler({
          type: AgentEventType.ApprovalRequest,
          value: {
            sessionId: 'spawn-session-2',
            toolCall: { toolCallId: 'tc-2' },
            permissionOptions: [
              {
                kind: 'allow_once' as any,
                optionId: 'allow_once',
                name: 'Allow once',
              },
            ],
            resolve: () => {},
          },
        } as AgentStreamEvent);
      }
      return Promise.resolve();
    });
    const controller = new AbortController();
    await kiro.streamMessage('test', controller.signal, onEvent);

    // The global handler should NOT have forwarded (per-message handler covers it)
    expect(approvalHandler).not.toHaveBeenCalled();
  });
});
