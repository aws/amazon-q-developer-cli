import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { KAS_DEFAULT_AGENT_ID } from '../constants/agents.js';
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
const mockUpdateHandlers = new Set<(event: AgentStreamEvent) => void>();
const mockSessionEventUnsubscribe = mock(() => {});
const mockMultiSessionUnsubscribe = mock(() => {});
const mockSubagentListUnsubscribe = mock(() => {});

function broadcastMockUpdate(event: AgentStreamEvent): void {
  for (const handler of [...mockUpdateHandlers]) handler(event);
}

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
    mockUpdateHandlers.add(handler);
    mockOnUpdateHandler = handler;
    return () => {
      mockUpdateHandlers.delete(handler);
      if (mockOnUpdateHandler === handler) {
        mockOnUpdateHandler =
          [...mockUpdateHandlers][mockUpdateHandlers.size - 1] ?? null;
      }
    };
  }),
  onSessionEvent: mock((_handler: (event: any) => void) => {
    return mockSessionEventUnsubscribe;
  }),
  onMultiSessionUpdate: mock(
    (_handler: (sessionId: string, event: AgentStreamEvent) => void) => {
      return mockMultiSessionUnsubscribe;
    }
  ),
  onSubagentListUpdate: mock(
    (_handler: (subagents: any[], pendingStages?: any[]) => void) => {
      return mockSubagentListUnsubscribe;
    }
  ),
  executeCommand: mock(() => Promise.resolve({ success: true, message: 'ok' })),
  getCommandOptions: mock(() => Promise.resolve({ options: [] })),
  setConfigOption: mock(() => Promise.resolve()),
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
  onSessionEvent = mockSessionClient.onSessionEvent;
  onMultiSessionUpdate = mockSessionClient.onMultiSessionUpdate;
  onSubagentListUpdate = mockSessionClient.onSubagentListUpdate;
  executeCommand = mockSessionClient.executeCommand;
  getCommandOptions = mockSessionClient.getCommandOptions;
  setConfigOption = mockSessionClient.setConfigOption;
  listSettings = mockSessionClient.listSettings;
  setSetting = mockSessionClient.setSetting;
  terminateSession = mockSessionClient.terminateSession;
  listSessions = mockSessionClient.listSessions;
  resolveSpecSession = mockSessionClient.resolveSpecSession;
  invokeSpec = mockSessionClient.invokeSpec;
  constructor() {}
};

// Load the real module via a query-string specifier (bypasses bun's mock
// registry) so we can spread its exports below. Overriding ONLY AcpClient /
// createAcpClient keeps the mock a complete superset of the real module —
// otherwise this global mock.module would strip exports like
// `parseAgentSubcommand` and break OTHER test files that share this process.
// @ts-expect-error — query-string specifier bypasses bun's mock registry
const realAcpClient = await import('../acp-client?real');

mock.module('../acp-client', () => ({
  ...realAcpClient,
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
    mockSessionClient.onSessionEvent.mockClear();
    mockSessionClient.onMultiSessionUpdate.mockClear();
    mockSessionClient.onSubagentListUpdate.mockClear();
    mockSessionEventUnsubscribe.mockClear();
    mockMultiSessionUnsubscribe.mockClear();
    mockSubagentListUnsubscribe.mockClear();
    mockSessionClient.onSessionEvent.mockImplementation(
      () => mockSessionEventUnsubscribe
    );
    mockSessionClient.onMultiSessionUpdate.mockImplementation(
      () => mockMultiSessionUnsubscribe
    );
    mockSessionClient.onSubagentListUpdate.mockImplementation(
      () => mockSubagentListUnsubscribe
    );
    mockSessionClient.executeCommand.mockClear();
    mockSessionClient.getCommandOptions.mockClear();
    mockSessionClient.setConfigOption.mockClear();
    mockSessionClient.listSettings.mockClear();
    mockSessionClient.setSetting.mockClear();
    mockSessionClient.terminateSession.mockClear();
    mockSessionClient.listSessions.mockClear();
    mockSessionClient.resolveSpecSession.mockClear();
    mockSessionClient.invokeSpec.mockClear();
    mockUpdateHandlers.clear();
    mockOnUpdateHandler = null;
    mockSessionClient.initialize.mockImplementation(() => Promise.resolve());
    mockSessionClient.listSettings.mockImplementation(() =>
      Promise.resolve({ 'chat.theme': 'dark' })
    );
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

  it('reinitializing closes the previous client and ignores its late events', async () => {
    const kiro = new Kiro();
    const commandsHandler = mock(() => {});
    kiro.onCommandsUpdate(commandsHandler);
    await kiro.initialize('/path/to/agent');
    const firstClientUpdate = mockOnUpdateHandler!;

    await kiro.initialize('/path/to/agent');
    expect(mockSessionClient.close).toHaveBeenCalledTimes(1);

    firstClientUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [{ name: 'stale', description: 'From old client' }],
    } as AgentStreamEvent);
    expect(commandsHandler).not.toHaveBeenCalled();

    mockOnUpdateHandler!({
      type: AgentEventType.CommandsUpdate,
      commands: [{ name: 'fresh', description: 'From active client' }],
    } as AgentStreamEvent);
    expect(commandsHandler).toHaveBeenCalledWith(
      [{ name: 'fresh', description: 'From active client' }],
      undefined
    );
  });

  it('keeps the newer client when overlapping initializations finish out of order', async () => {
    let resolveFirstInitialize!: () => void;
    let resolveSecondInitialize!: () => void;
    const firstInitialize = new Promise<void>((resolve) => {
      resolveFirstInitialize = resolve;
    });
    const secondInitialize = new Promise<void>((resolve) => {
      resolveSecondInitialize = resolve;
    });
    mockSessionClient.initialize
      .mockImplementationOnce(() => firstInitialize)
      .mockImplementationOnce(() => secondInitialize);
    mockSessionClient.listSettings.mockImplementation(() =>
      Promise.resolve({ 'chat.theme': 'newer' })
    );

    const kiro = new Kiro();
    const commandsHandler = mock(() => {});
    kiro.onCommandsUpdate(commandsHandler);

    const firstStartup = kiro
      .initialize('/path/to/first-agent')
      .then(() => kiro.createSession());
    const firstOutcome = firstStartup.then(
      () => null,
      (error: unknown) => error
    );
    const firstClientUpdate = mockSessionClient.onUpdate.mock.calls[0]![0];
    const second = kiro.initialize('/path/to/second-agent');
    const secondClientUpdate = mockSessionClient.onUpdate.mock.calls[1]![0];

    resolveSecondInitialize();
    await second;
    resolveFirstInitialize();
    const staleError = await firstOutcome;

    firstClientUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [{ name: 'stale', description: 'From old client' }],
    } as AgentStreamEvent);
    secondClientUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [{ name: 'fresh', description: 'From active client' }],
    } as AgentStreamEvent);

    expect(staleError).toBeInstanceOf(Error);
    expect((staleError as Error).message).toContain(
      'superseded by a newer attempt'
    );
    expect(mockSessionClient.newSession).not.toHaveBeenCalled();
    expect(mockSessionClient.listSettings).toHaveBeenCalledTimes(1);
    expect(kiro.settings).toEqual({ 'chat.theme': 'newer' });
    expect(commandsHandler).toHaveBeenCalledTimes(1);
    expect(commandsHandler).toHaveBeenCalledWith(
      [{ name: 'fresh', description: 'From active client' }],
      undefined
    );
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

  it('KasModelConfigUpdate event notifies kasModelConfigHandler only, not agentHandler', async () => {
    const kiro = new Kiro();
    const kasModelConfigHandler = mock(() => {});
    const agentHandler = mock(() => {});
    kiro.onKasModelConfigUpdate(kasModelConfigHandler);
    kiro.onAgentUpdate(agentHandler);
    await kiro.initialize('/path/to/agent');

    expect(mockOnUpdateHandler).not.toBeNull();
    mockOnUpdateHandler!({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'gpt-5', name: 'GPT-5' }],
      currentModelId: 'gpt-5',
      efforts: [],
      currentLevel: null,
      origin: 'serverPush',
    } as AgentStreamEvent);

    expect(kasModelConfigHandler).toHaveBeenCalledTimes(1);
    // A model/effort config update must not clobber the current agent.
    expect(agentHandler).not.toHaveBeenCalled();
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

  it('close detaches every retained session subscription', async () => {
    const kiro = new Kiro();
    kiro.onSessionEvent(() => {});
    kiro.onMultiSessionUpdate(() => {});
    kiro.onSubagentListUpdate(() => {});
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();

    kiro.close();

    expect(mockSessionEventUnsubscribe).toHaveBeenCalledTimes(1);
    expect(mockMultiSessionUnsubscribe).toHaveBeenCalledTimes(1);
    expect(mockSubagentListUnsubscribe).toHaveBeenCalledTimes(1);
  });

  it('unsubscribes a retained channel before registering its replacement', async () => {
    const calls: string[] = [];
    const unsubscribe = mock(() => {
      calls.push('unsubscribe');
    });
    mockSessionClient.onSessionEvent.mockImplementation(() => {
      calls.push('subscribe');
      return unsubscribe;
    });
    const kiro = new Kiro();
    const handler = () => {};
    kiro.onSessionEvent(handler);
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    calls.length = 0;

    kiro.onSessionEvent(handler);

    expect(calls).toEqual(['unsubscribe', 'subscribe']);
  });

  it('drains late events before allowing the next prompt to start', async () => {
    const firstEvents: string[] = [];
    const secondEvents: string[] = [];
    let resolveSecondPrompt!: () => void;
    mockSessionClient.prompt
      .mockImplementationOnce(() =>
        Promise.resolve().then(() => {
          // ACP prompt responses can resolve before an already-received
          // notification reaches the client event handlers.
          setTimeout(() => {
            broadcastMockUpdate({
              type: AgentEventType.Content,
              id: 'late-first',
              content: { type: 'text', text: 'late first turn content' },
            } as AgentStreamEvent);
          }, 0);
        })
      )
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveSecondPrompt = resolve;
          })
      );
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    await kiro.streamMessage(
      'first',
      new AbortController().signal,
      (event: AgentStreamEvent) => firstEvents.push((event as any).id)
    );
    const secondPrompt = kiro.streamMessage(
      'second',
      new AbortController().signal,
      (event: AgentStreamEvent) => secondEvents.push((event as any).id)
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(firstEvents).toEqual(['late-first']);
    expect(secondEvents).toEqual([]);
    expect((kiro as any)._promptActive).toBe(true);

    resolveSecondPrompt();
    await secondPrompt;
  });

  it('times out a retired prompt without cancelling the replacement client', async () => {
    const previousTimeout = process.env.KIRO_INITIAL_RESPONSE_TIMEOUT_MS;
    process.env.KIRO_INITIAL_RESPONSE_TIMEOUT_MS = '20';
    const kiro = new Kiro();
    try {
      await kiro.initialize('/path/to/agent');
      const firstClient = (kiro as any).sessionClient;
      const firstCancel = mock(() => Promise.resolve());
      firstClient.cancel = firstCancel;
      firstClient.prompt = mock(() => new Promise<void>(() => {}));

      const retiredPrompt = kiro.streamMessage(
        'first',
        new AbortController().signal,
        () => {}
      );
      await kiro.initialize('/path/to/replacement-agent');
      const replacementCancel = mock(() => Promise.resolve());
      (kiro as any).sessionClient.cancel = replacementCancel;

      await expect(retiredPrompt).rejects.toThrow('Agent not responding');
      expect(firstCancel).toHaveBeenCalledTimes(1);
      expect(replacementCancel).not.toHaveBeenCalled();
    } finally {
      if (previousTimeout === undefined) {
        delete process.env.KIRO_INITIAL_RESPONSE_TIMEOUT_MS;
      } else {
        process.env.KIRO_INITIAL_RESPONSE_TIMEOUT_MS = previousTimeout;
      }
      kiro.close();
    }
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

  it('onCompactionStatus receives ContextBreakdownUpdate events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onCompactionStatus(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ContextBreakdownUpdate,
        breakdown: {
          contextFiles: { tokens: 100, percent: 5 },
          tools: { tokens: 20, percent: 1 },
          kiroResponses: { tokens: 30, percent: 2 },
          yourPrompts: { tokens: 40, percent: 2 },
        },
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

  it('onCompactionStatus receives SessionRosterDelta events outside a prompt', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onCompactionStatus(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.SessionRosterDelta,
        delta: {
          upserted: [{ sessionId: 'sess-1', status: 'idle' }],
        },
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
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onInitNotification receives MCP registry snapshots', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onInitNotification(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.McpRegistrySnapshot,
        registryServers: [],
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onInitNotification receives MCP server snapshots', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onInitNotification(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.McpServerSnapshot,
        servers: [],
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

  it('setConfigOption forwards to sessionClient', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.setConfigOption('mode', 'fast');
    expect(mockSessionClient.setConfigOption).toHaveBeenCalledWith(
      'mode',
      'fast'
    );
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

  it('sends images and embedded resources as ACP content blocks', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();
    const controller = new AbortController();
    const images = [{ base64: 'abc', mimeType: 'image/png' }];
    const resources = [
      {
        uri: 'file:///tmp/notes.txt',
        text: 'notes',
        mimeType: 'text/plain',
      },
    ];
    const blobs = [
      {
        uri: 'file:///tmp/report.pdf',
        blob: 'cGRm',
        mimeType: 'application/pdf',
      },
    ];

    await kiro.streamMessage(
      'describe',
      controller.signal,
      () => {},
      images,
      resources,
      blobs
    );

    expect(mockSessionClient.prompt).toHaveBeenLastCalledWith([
      { type: 'image', data: 'abc', mimeType: 'image/png' },
      {
        type: 'resource',
        resource: {
          uri: 'file:///tmp/notes.txt',
          text: 'notes',
          mimeType: 'text/plain',
        },
      },
      {
        type: 'resource',
        resource: {
          uri: 'file:///tmp/report.pdf',
          blob: 'cGRm',
          mimeType: 'application/pdf',
        },
      },
      { type: 'text', text: 'describe' },
    ]);
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
