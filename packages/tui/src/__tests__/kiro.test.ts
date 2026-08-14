import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { KAS_DEFAULT_AGENT_ID } from '../constants/agents.js';
import { AgentEventType } from '../types/agent-events';
import type {
  AgentStreamEvent,
  WorkflowProgressStreamEvent,
} from '../types/agent-events';
import type { WorkflowProgressSource } from '../kiro';
import type { WorkflowNodeSessionTarget } from '../types/workflow.js';
import type {
  WorkflowCancelResponse,
  WorkflowInspectResponse,
  WorkflowPauseResponse,
  WorkflowResumeResponse,
  WorkflowRunSummary,
} from '../types/workflow-history.js';
import type {
  WorkflowCreateRequest,
  WorkflowCreateResponse,
  WorkflowInvokeResponse,
  WorkflowRecipeDescriptor,
} from '../types/workflow-launch.js';
import { releaseSessionLock } from '../utils/session-lock.js';

// --- Mock logger ---
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../utils/logger']);

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
const mockWorkflowConversation = {
  sendMessage: mock(
    (_target: WorkflowNodeSessionTarget, _content: string): Promise<void> =>
      Promise.resolve()
  ),
};
const mockWorkflowControl = {
  listRecipes: mock(
    (_workspacePaths: readonly string[]): Promise<WorkflowRecipeDescriptor[]> =>
      Promise.resolve([])
  ),
  createRun: mock(
    async (
      _request: WorkflowCreateRequest
    ): Promise<WorkflowCreateResponse> => {
      throw new Error('createRun response not configured');
    }
  ),
  invokeRun: mock(
    (workflowId: string): Promise<WorkflowInvokeResponse> =>
      Promise.resolve({ workflowId, status: 'running' })
  ),
  listRuns: mock(
    (_workspacePaths: readonly string[]): Promise<WorkflowRunSummary[]> =>
      Promise.resolve([])
  ),
  inspectRun: mock(
    async (_workflowId: string): Promise<WorkflowInspectResponse> => {
      throw new Error('inspectRun response not configured');
    }
  ),
  pauseRun: mock(
    (_workflowId: string): Promise<WorkflowPauseResponse> =>
      Promise.resolve({ paused: true })
  ),
  resumeRun: mock(
    (workflowId: string): Promise<WorkflowResumeResponse> =>
      Promise.resolve({ workflowId, status: 'running' as const })
  ),
  cancelRun: mock(
    (
      _workflowId: string,
      _targetStatus?: 'aborted' | 'completed'
    ): Promise<WorkflowCancelResponse> =>
      Promise.resolve({ ok: true, previousStatus: 'running' as const })
  ),
};

function broadcastMockUpdate(event: AgentStreamEvent): void {
  for (const handler of [...mockUpdateHandlers]) handler(event);
}

const mockSessionClient = {
  sessionId: undefined as string | undefined,
  abandonPendingLoad: undefined as (() => void) | undefined,
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
  workflowConversation: mockWorkflowConversation,
  workflowControl: mockWorkflowControl,
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
  loadSession = (...args: Parameters<typeof mockSessionClient.loadSession>) => {
    const result = mockSessionClient.loadSession(...args);
    void result.then(
      (response) => {
        this.sessionId = response.sessionId;
      },
      () => {}
    );
    return result;
  };
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
  // Lazy delegate: tests install the spy after this instance is constructed.
  abandonPendingLoad = () => mockSessionClient.abandonPendingLoad?.();
  listSessions = mockSessionClient.listSessions;
  resolveSpecSession = mockSessionClient.resolveSpecSession;
  invokeSpec = mockSessionClient.invokeSpec;
  workflowConversation = mockSessionClient.workflowConversation;
  workflowControl = mockSessionClient.workflowControl;
  constructor() {}
};

// Load the real module via a query-string specifier (bypasses bun's mock
// registry) so we can spread its exports below. Overriding ONLY AcpClient /
// createAcpClient keeps the mock a complete superset of the real module —
// otherwise this global mock.module would strip exports like
// `parseAgentSubcommand` and break OTHER test files that share this process.
// @ts-expect-error — query-string specifier bypasses bun's mock registry
const realAcpClient = await import('../acp-client?real');

// Captured from the query-suffixed import above: this module's graph contains
// top-level await, so it cannot be required at module scope when this file
// runs alone.
restoreRealModulesAfterAll(import.meta.dir, [['../acp-client', realAcpClient]]);

mock.module('../acp-client', () => ({
  ...realAcpClient,
  AcpClient: MockAcpClientClass,
  createAcpClient: () => new MockAcpClientClass(),
}));

const recordTuiWorkflowControl = mock(
  (_action: string, _result: string, _version: string): void => {}
);

const initialAgentEngine = process.env.KIRO_AGENT_ENGINE;
afterAll(() => {
  if (initialAgentEngine === undefined) {
    delete process.env.KIRO_AGENT_ENGINE;
  } else {
    process.env.KIRO_AGENT_ENGINE = initialAgentEngine;
  }
  mock.restore();
});

// Use a query-string import so the specifier doesn't match the bare
// '../kiro' that other test files mock via mock.module.  This gives us
// the real Kiro class (which will pick up our '../acp-client' mock above).
// @ts-expect-error — query-string specifier bypasses bun's mock registry
const { Kiro } = await import('../kiro?real');

describe('Kiro', () => {
  beforeEach(() => {
    process.env.KIRO_AGENT_ENGINE = 'v2';
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
    mockWorkflowConversation.sendMessage.mockClear();
    mockWorkflowControl.listRecipes.mockClear();
    mockWorkflowControl.createRun.mockClear();
    mockWorkflowControl.invokeRun.mockClear();
    mockWorkflowControl.listRuns.mockClear();
    mockWorkflowControl.inspectRun.mockClear();
    mockWorkflowControl.pauseRun.mockClear();
    mockWorkflowControl.resumeRun.mockClear();
    mockWorkflowControl.cancelRun.mockClear();
    recordTuiWorkflowControl.mockClear();
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
    mockSessionClient.loadSession.mockImplementation((id: string) =>
      Promise.resolve({
        sessionId: id,
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      })
    );
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

  it('rejects createSession when the returned session cannot be locked', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-lock-failure-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    const dir = join(root, 'hash', 'sess_session-1');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({ id: 'sess_session-1' })
    );
    writeFileSync(join(dir, '.lock'), '{"pid":');

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');

      await expect(kiro.createSession()).rejects.toThrow(
        'malformed or unreadable'
      );
      expect(mockSessionClient.terminateSession).toHaveBeenCalledWith(
        'session-1'
      );
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('subscribes before newSession can replay workflow child events', async () => {
    const received: string[] = [];
    let sessionEventHandler: ((event: any) => void) | undefined;
    let multiSessionHandler:
      | ((sessionId: string, event: AgentStreamEvent) => void)
      | undefined;
    mockSessionClient.onSessionEvent.mockImplementation((handler) => {
      sessionEventHandler = handler;
      return mockSessionEventUnsubscribe;
    });
    mockSessionClient.onMultiSessionUpdate.mockImplementation((handler) => {
      multiSessionHandler = handler;
      return mockMultiSessionUnsubscribe;
    });
    mockSessionClient.newSession.mockImplementation(() => {
      sessionEventHandler!({
        type: 'session_created',
        session: { id: 'workflow-child' },
      });
      multiSessionHandler!('workflow-child', {
        type: AgentEventType.Content,
        content: { type: 'text', text: 'replayed child output' },
      } as AgentStreamEvent);
      return Promise.resolve({
        sessionId: 'session-1',
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: {
          name: 'test-agent',
          welcomeMessage: 'Welcome!',
        },
      });
    });

    const kiro = new Kiro();
    kiro.onSessionEvent(() => received.push('session'));
    kiro.onMultiSessionUpdate(() => received.push('message'));
    kiro.onSubagentListUpdate(() => {});

    await kiro.initialize('/path/to/agent');
    await kiro.createSession();

    expect(received).toEqual(['session', 'message']);
    expect(mockSessionClient.onSessionEvent).toHaveBeenCalledTimes(1);
    expect(mockSessionClient.onMultiSessionUpdate).toHaveBeenCalledTimes(1);
    expect(mockSessionClient.onSubagentListUpdate).toHaveBeenCalledTimes(1);
  });

  it('keeps the active lock when a remote same-id local copy is locked and load fails', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-lock-transfer-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    const previousEngine = process.env.KIRO_AGENT_ENGINE;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    process.env.KIRO_AGENT_ENGINE = 'v2';
    const sessionDir = join(root, 'hash', 'sess_session-1');
    const collisionDir = join(root, 'hash', 'sess_remote-only');
    mkdirSync(sessionDir, { recursive: true });
    mkdirSync(collisionDir, { recursive: true });
    writeFileSync(
      join(sessionDir, 'session.json'),
      JSON.stringify({ id: 'sess_session-1' })
    );
    writeFileSync(
      join(collisionDir, 'session.json'),
      JSON.stringify({ id: 'sess_remote-only' })
    );
    writeFileSync(join(collisionDir, '.lock'), '{"pid":');

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      await kiro.createSession();
      const lockPath = join(sessionDir, '.lock');
      expect(existsSync(lockPath)).toBe(true);

      mockSessionClient.loadSession.mockRejectedValueOnce(
        new Error('load failed')
      );
      await expect(
        kiro.loadSession('remote-only', undefined, { source: 'remote' })
      ).rejects.toThrow('load failed');
      expect(existsSync(lockPath)).toBe(true);
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      if (previousEngine === undefined) {
        delete process.env.KIRO_AGENT_ENGINE;
      } else {
        process.env.KIRO_AGENT_ENGINE = previousEngine;
      }
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('releases a failed createSession resume target lock', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-failed-resume-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    const sessionDir = join(root, 'hash', 'sess_target');
    mkdirSync(sessionDir, { recursive: true });
    writeFileSync(
      join(sessionDir, 'session.json'),
      JSON.stringify({ id: 'sess_target' })
    );

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      mockSessionClient.loadSession.mockRejectedValueOnce(
        new Error('resume failed')
      );

      await expect(kiro.createSession('target')).rejects.toThrow(
        'resume failed'
      );
      expect(existsSync(join(sessionDir, '.lock'))).toBe(false);
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('serializes overlapping loads and leaves only the final lock held', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-overlap-load-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    for (const id of ['first', 'second']) {
      const dir = join(root, 'hash', `sess_${id}`);
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, 'session.json'),
        JSON.stringify({ id: `sess_${id}` })
      );
    }

    let resolveFirst!: (value: {
      sessionId: string;
      currentModel: { id: string; name: string };
      currentAgent: { name: string };
    }) => void;
    let resolveSecond!: typeof resolveFirst;
    const firstResult = new Promise<{
      sessionId: string;
      currentModel: { id: string; name: string };
      currentAgent: { name: string };
    }>((resolve) => {
      resolveFirst = resolve;
    });
    const secondResult = new Promise<{
      sessionId: string;
      currentModel: { id: string; name: string };
      currentAgent: { name: string };
    }>((resolve) => {
      resolveSecond = resolve;
    });
    mockSessionClient.loadSession.mockImplementation((id: string) =>
      id === 'first' ? firstResult : secondResult
    );

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      const firstLoad = kiro.loadSession('first');
      const secondLoad = kiro.loadSession('second');
      await new Promise((resolve) => setImmediate(resolve));

      expect(mockSessionClient.loadSession).toHaveBeenCalledTimes(1);
      expect(existsSync(join(root, 'hash', 'sess_first', '.lock'))).toBe(true);
      expect(existsSync(join(root, 'hash', 'sess_second', '.lock'))).toBe(
        false
      );

      resolveFirst({
        sessionId: 'first',
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      });
      await firstLoad;
      await new Promise((resolve) => setImmediate(resolve));
      expect(mockSessionClient.loadSession).toHaveBeenCalledTimes(2);
      expect(existsSync(join(root, 'hash', 'sess_first', '.lock'))).toBe(true);
      expect(existsSync(join(root, 'hash', 'sess_second', '.lock'))).toBe(true);

      resolveSecond({
        sessionId: 'second',
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      });
      await secondLoad;
      expect(existsSync(join(root, 'hash', 'sess_first', '.lock'))).toBe(false);
      expect(existsSync(join(root, 'hash', 'sess_second', '.lock'))).toBe(true);
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('never terminates the target when same-target loads are queued', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');
    await kiro.createSession('outgoing');
    mockSessionClient.loadSession.mockClear();
    mockSessionClient.terminateSession.mockClear();

    const first = kiro.loadSession('target');
    const second = kiro.loadSession('target');
    await Promise.all([first, second]);

    expect(mockSessionClient.loadSession).toHaveBeenCalledTimes(2);
    expect(mockSessionClient.terminateSession).toHaveBeenCalledTimes(1);
    expect(mockSessionClient.terminateSession).toHaveBeenCalledWith('outgoing');
    expect(mockSessionClient.terminateSession).not.toHaveBeenCalledWith(
      'target'
    );
  });

  it('subscribes before loadSession can replay workflow child events', async () => {
    const received: string[] = [];
    let sessionEventHandler: ((event: any) => void) | undefined;
    let multiSessionHandler:
      | ((sessionId: string, event: AgentStreamEvent) => void)
      | undefined;
    mockSessionClient.onSessionEvent.mockImplementation((handler) => {
      sessionEventHandler = handler;
      return mockSessionEventUnsubscribe;
    });
    mockSessionClient.onMultiSessionUpdate.mockImplementation((handler) => {
      multiSessionHandler = handler;
      return mockMultiSessionUnsubscribe;
    });
    mockSessionClient.loadSession.mockImplementation((sessionId: string) => {
      sessionEventHandler!({
        type: 'session_created',
        session: { id: 'workflow-child' },
      });
      multiSessionHandler!('workflow-child', {
        type: AgentEventType.Content,
        content: { type: 'text', text: 'loaded child output' },
      } as AgentStreamEvent);
      return Promise.resolve({
        sessionId,
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      });
    });

    const kiro = new Kiro();
    kiro.onSessionEvent(() => received.push('session'));
    kiro.onMultiSessionUpdate(() => received.push('message'));
    kiro.onSubagentListUpdate(() => {});

    await kiro.initialize('/path/to/agent');
    await kiro.createSession('existing-session');

    expect(received).toEqual(['session', 'message']);
    expect(mockSessionClient.onSessionEvent).toHaveBeenCalledTimes(1);
    expect(mockSessionClient.onMultiSessionUpdate).toHaveBeenCalledTimes(1);
    expect(mockSessionClient.onSubagentListUpdate).toHaveBeenCalledTimes(1);
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

  it('forwards workflow launch, history, and controls through the typed capability', async () => {
    const kiro = new Kiro({ recordWorkflowControl: recordTuiWorkflowControl });
    const nodeTarget = {
      workflowId: 'workflow-1',
      parentSessionId: 'parent-session',
      nodeId: 'build',
      nodePath: ['root', 'build'],
      sessionId: 'workflow-node-session',
    } as const satisfies WorkflowNodeSessionTarget;
    const state = {
      workflowId: 'workflow-1',
      workflowName: 'Test workflow',
      status: 'completed' as const,
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      root: {
        nodeId: 'root',
        type: 'sequence' as const,
        status: 'completed' as const,
      },
    };
    const summary = {
      workflowId: 'workflow-1',
      name: 'Test workflow',
      status: 'completed' as const,
      createdAt: '2026-07-19T10:00:00.000Z',
      updatedAt: '2026-07-19T10:01:00.000Z',
    };
    mockWorkflowControl.listRuns.mockResolvedValueOnce([summary]);
    mockWorkflowControl.listRecipes.mockResolvedValueOnce([
      {
        name: 'release',
        source: 'bundled://release',
        builtIn: true,
      },
    ]);
    mockWorkflowControl.createRun.mockResolvedValueOnce({
      workflowId: 'workflow-1',
      initialState: state,
    });
    mockWorkflowControl.inspectRun.mockResolvedValueOnce({
      workflowId: 'workflow-1',
      state,
    });
    await kiro.initialize('/path/to/agent');

    await expect(kiro.listWorkflows()).resolves.toEqual([summary]);
    await expect(kiro.listWorkflowRecipes()).resolves.toEqual([
      {
        name: 'release',
        source: 'bundled://release',
        builtIn: true,
      },
    ]);
    await expect(
      kiro.createWorkflow({
        source: { type: 'path', workflowPath: 'bundled://release' },
        inputs: {},
        parentSessionId: 'parent-session',
      })
    ).resolves.toEqual({
      workflowId: 'workflow-1',
      initialState: state,
    });
    await expect(kiro.invokeWorkflow('workflow-1')).resolves.toEqual({
      workflowId: 'workflow-1',
      status: 'running',
    });
    await expect(kiro.inspectWorkflow('workflow-1')).resolves.toEqual({
      workflowId: 'workflow-1',
      state,
    });
    await expect(
      kiro.messageWorkflowNode(nodeTarget, 'private message')
    ).resolves.toBeUndefined();
    await expect(kiro.pauseWorkflow('workflow-1')).resolves.toEqual({
      paused: true,
    });
    await expect(kiro.resumeWorkflow('workflow-1')).resolves.toEqual({
      workflowId: 'workflow-1',
      status: 'running',
    });
    await expect(
      kiro.cancelWorkflow('workflow-1', 'completed')
    ).resolves.toEqual({
      ok: true,
      previousStatus: 'running',
    });

    expect(mockWorkflowControl.listRuns).toHaveBeenCalledWith([process.cwd()]);
    expect(mockWorkflowControl.listRecipes).toHaveBeenCalledWith([
      process.cwd(),
    ]);
    expect(mockWorkflowControl.createRun).toHaveBeenCalledWith({
      source: { type: 'path', workflowPath: 'bundled://release' },
      inputs: {},
      parentSessionId: 'parent-session',
    });
    expect(mockWorkflowControl.invokeRun).toHaveBeenCalledWith('workflow-1');
    expect(mockWorkflowControl.inspectRun).toHaveBeenCalledWith('workflow-1');
    expect(mockWorkflowConversation.sendMessage).toHaveBeenCalledWith(
      nodeTarget,
      'private message'
    );
    // Every control path out of the TUI is a deliberate human act, so each one
    // attributes itself — otherwise KAS nudges the parent session about an
    // "aborted" run the user stopped on purpose.
    expect(mockWorkflowControl.pauseRun).toHaveBeenCalledWith('workflow-1', {
      initiator: 'user',
    });
    expect(mockWorkflowControl.resumeRun).toHaveBeenCalledWith('workflow-1', {
      initiator: 'user',
    });
    expect(mockWorkflowControl.cancelRun).toHaveBeenCalledWith(
      'workflow-1',
      'completed',
      { initiator: 'user' }
    );
    expect(recordTuiWorkflowControl.mock.calls).toEqual([
      ['message', 'success', expect.any(String)],
      ['pause', 'success', expect.any(String)],
      ['resume', 'success', expect.any(String)],
      ['cancel', 'success', expect.any(String)],
    ]);
  });

  it('forwards an explanation for a stop the user gave a reason for', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    await kiro.cancelWorkflow('workflow-1', 'aborted', 'wrong branch');

    expect(mockWorkflowControl.cancelRun).toHaveBeenCalledWith(
      'workflow-1',
      'aborted',
      { initiator: 'user', reason: 'wrong branch' }
    );
  });

  it('records every rejected workflow control and preserves its error', async () => {
    const error = new Error('workflow transport unavailable');
    const target = {
      workflowId: 'workflow-1',
      parentSessionId: 'parent-session',
      nodeId: 'build',
      nodePath: ['root', 'build'],
      sessionId: 'workflow-node-session',
    } as const satisfies WorkflowNodeSessionTarget;
    const kiro = new Kiro({ recordWorkflowControl: recordTuiWorkflowControl });
    await kiro.initialize('/path/to/agent');

    const cases = [
      {
        action: 'message',
        reject: () =>
          mockWorkflowConversation.sendMessage.mockRejectedValueOnce(error),
        invoke: () => kiro.messageWorkflowNode(target, 'private message'),
      },
      {
        action: 'pause',
        reject: () => mockWorkflowControl.pauseRun.mockRejectedValueOnce(error),
        invoke: () => kiro.pauseWorkflow('workflow-1'),
      },
      {
        action: 'resume',
        reject: () =>
          mockWorkflowControl.resumeRun.mockRejectedValueOnce(error),
        invoke: () => kiro.resumeWorkflow('workflow-1'),
      },
      {
        action: 'cancel',
        reject: () =>
          mockWorkflowControl.cancelRun.mockRejectedValueOnce(error),
        invoke: () => kiro.cancelWorkflow('workflow-1'),
      },
    ] as const;

    for (const testCase of cases) {
      recordTuiWorkflowControl.mockClear();
      testCase.reject();
      await expect(testCase.invoke()).rejects.toBe(error);
      expect(recordTuiWorkflowControl).toHaveBeenCalledWith(
        testCase.action,
        'failed',
        expect.any(String)
      );
    }
  });

  it('treats resolved workflow control rejections as failures', async () => {
    const kiro = new Kiro({ recordWorkflowControl: recordTuiWorkflowControl });
    await kiro.initialize('/path/to/agent');

    mockWorkflowControl.pauseRun.mockResolvedValueOnce({ paused: false });
    await expect(kiro.pauseWorkflow('workflow-1')).rejects.toThrow(
      'Workflow pause was rejected'
    );

    mockWorkflowControl.cancelRun.mockResolvedValueOnce({
      ok: false,
      previousStatus: 'running',
    });
    await expect(kiro.cancelWorkflow('workflow-1')).rejects.toThrow(
      'Workflow cancel was rejected'
    );

    expect(recordTuiWorkflowControl.mock.calls).toEqual([
      ['pause', 'failed', expect.any(String)],
      ['cancel', 'failed', expect.any(String)],
    ]);
  });

  it('preserves workflow control errors for the command layer', async () => {
    const error = new Error('workflow transport unavailable');
    mockWorkflowControl.listRuns.mockRejectedValueOnce(error);
    mockWorkflowControl.inspectRun.mockRejectedValueOnce(error);
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    await expect(kiro.listWorkflows()).rejects.toBe(error);
    await expect(kiro.inspectWorkflow('workflow-1')).rejects.toBe(error);
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

  it('newSession serializes the transition and moves the held lock', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-new-session-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    for (const id of ['session-1', 'session-2']) {
      const dir = join(root, 'hash', `sess_${id}`);
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, 'session.json'),
        JSON.stringify({ id: `sess_${id}` })
      );
    }

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      await kiro.createSession();
      expect(existsSync(join(root, 'hash', 'sess_session-1', '.lock'))).toBe(
        true
      );

      mockSessionClient.newSession.mockClear();
      mockSessionClient.newSession.mockImplementation(() =>
        Promise.resolve({
          sessionId: 'session-2',
          currentModel: undefined as any,
          currentAgent: undefined as any,
        })
      );

      const result = await kiro.newSession();
      expect(result.sessionId).toBe('session-2');
      expect(mockSessionClient.terminateSession).toHaveBeenCalledWith(
        'session-1'
      );
      expect(existsSync(join(root, 'hash', 'sess_session-1', '.lock'))).toBe(
        false
      );
      expect(existsSync(join(root, 'hash', 'sess_session-2', '.lock'))).toBe(
        true
      );
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('retires a new session after lock adoption rolls back successfully', async () => {
    const root = mkdtempSync(join(tmpdir(), 'kiro-new-session-rollback-'));
    const previousRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
    process.env.KIRO_TEST_SESSIONS_ROOT = root;
    const previousDir = join(root, 'hash', 'sess_session-1');
    const newDir = join(root, 'hash', 'sess_session-2');
    mkdirSync(previousDir, { recursive: true });
    mkdirSync(newDir, { recursive: true });
    writeFileSync(
      join(previousDir, 'session.json'),
      JSON.stringify({ id: 'sess_session-1' })
    );
    writeFileSync(
      join(newDir, 'session.json'),
      JSON.stringify({ id: 'sess_session-2' })
    );
    writeFileSync(join(newDir, '.lock'), '{"pid":');

    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      await kiro.createSession();
      mockSessionClient.newSession.mockImplementationOnce(() =>
        Promise.resolve({
          sessionId: 'session-2',
          currentModel: undefined as any,
          currentAgent: undefined as any,
        })
      );

      await expect(kiro.newSession()).rejects.toThrow(
        'malformed or unreadable'
      );

      expect(mockSessionClient.loadSession).toHaveBeenLastCalledWith(
        'session-1'
      );
      expect(mockSessionClient.terminateSession).toHaveBeenCalledWith(
        'session-2'
      );
      expect(existsSync(join(previousDir, '.lock'))).toBe(true);
    } finally {
      releaseSessionLock();
      if (previousRoot === undefined) {
        delete process.env.KIRO_TEST_SESSIONS_ROOT;
      } else {
        process.env.KIRO_TEST_SESSIONS_ROOT = previousRoot;
      }
      rmSync(root, { recursive: true, force: true });
    }
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

  describe('fork()', () => {
    it('reports "not supported" when not initialized (no session client)', async () => {
      const kiro = new Kiro();
      const result = await kiro.fork({ createdReason: 'tangent', title: 'x' });
      expect(result.success).toBe(false);
      expect(result.message).toBe('Fork is not supported by this engine');
    });

    it('reports "not supported" when the session client lacks fork', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      // The mock AcpClient exposes no `fork` (simulates a non-KAS engine).
      const result = await kiro.fork({ createdReason: 'tangent', title: 'x' });
      expect(result.success).toBe(false);
      expect(result.message).toBe('Fork is not supported by this engine');
    });

    it('delegates to sessionClient.fork when supported', async () => {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      const sc = (kiro as any).sessionClient;
      const fork = mock((_opts: { createdReason: string; title?: string }) =>
        Promise.resolve({
          success: true,
          message: '',
          data: { sessionId: 'forked-1' },
        })
      );
      sc.fork = fork;
      try {
        const result = await kiro.fork({
          createdReason: 'tangent',
          title: 't',
        });
        expect(fork).toHaveBeenCalledTimes(1);
        expect(fork.mock.calls[0]![0]!.createdReason).toBe('tangent');
        expect(result.data).toEqual({ sessionId: 'forked-1' });
      } finally {
        delete sc.fork;
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
    expect(handler).toHaveBeenCalledWith(tools, undefined);
  });

  it('onToolsUpdate forwards the session-tagged flag when the push carries it', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onToolsUpdate(handler);
    await kiro.initialize('/path/to/agent');
    const tools = [
      { name: 'read', source: 'builtin', description: 'read tools' },
    ];
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ToolsUpdate,
        tools,
        sessionTagged: true,
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledWith(tools, true);
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

  it('onLiveContent receives content when no local prompt owns the turn', async () => {
    // The always-on renderer must get live content for a turn this client did
    // not start (web-initiated, or resumed mid-turn) — the bug where a resumed
    // session showed the transcript but no live deltas until the user prompted.
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onLiveContent(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.Content,
        id: 'msg-1',
        content: { type: 'text', text: 'live text' },
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalled();
  });

  it('onLiveContent receives turn boundaries when no local prompt owns the turn', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onLiveContent(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.TurnStart,
      } as AgentStreamEvent);
      mockOnUpdateHandler({
        type: AgentEventType.TurnEnd,
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledTimes(2);
  });

  it('routes mid-session replay only to its history subscriber', async () => {
    const replayedEvent = {
      type: AgentEventType.Content,
      id: 'replayed-content',
      content: { type: 'text', text: 'loaded once' },
    } as AgentStreamEvent;
    mockSessionClient.loadSession.mockImplementationOnce(async (sessionId) => {
      broadcastMockUpdate(replayedEvent);
      return {
        sessionId,
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      };
    });

    const kiro = new Kiro();
    const resetSession = mock(() => {});
    const setHistoryReplay = mock((_value: boolean) => {});
    const liveHandler = Object.assign(
      mock(() => {}),
      {
        resetSession,
        setHistoryReplay,
      }
    );
    const historyHandler = mock(() => {});
    kiro.onLiveContent(liveHandler);
    await kiro.initialize('/path/to/agent');

    await kiro.loadSession('loaded-session', historyHandler);

    expect(historyHandler).toHaveBeenCalledTimes(1);
    expect(liveHandler).not.toHaveBeenCalled();
    expect(kiro.replayHistory([replayedEvent])).toBe(true);
    expect(resetSession).toHaveBeenCalledTimes(1);
    expect(setHistoryReplay.mock.calls).toEqual([[true], [false]]);
    expect(liveHandler).toHaveBeenCalledTimes(1);

    broadcastMockUpdate({
      ...replayedEvent,
      id: 'live-content',
      content: { type: 'text', text: 'live once' },
    } as AgentStreamEvent);

    expect(historyHandler).toHaveBeenCalledTimes(1);
    expect(liveHandler).toHaveBeenCalledTimes(2);
  });

  it('keeps switched-session workflow lifecycle in buffered history order', async () => {
    const incomingUser = {
      type: AgentEventType.UserMessage,
      id: 'incoming-user',
      content: { type: 'text', text: 'run release' },
    } as AgentStreamEvent;
    const replayedWorkflow = {
      type: AgentEventType.WorkflowProgress,
      id: 'replayed-workflow',
      event: {
        type: 'run_start',
        workflowId: 'wf-replayed',
        workflowName: 'release',
        inputs: {},
        nodeTree: [],
      },
    } as AgentStreamEvent;
    const liveWorkflow = {
      type: AgentEventType.WorkflowProgress,
      id: 'live-workflow',
      event: {
        type: 'run_start',
        workflowId: 'wf-live',
        workflowName: 'live',
        inputs: {},
        nodeTree: [],
      },
    } as AgentStreamEvent;
    mockSessionClient.loadSession.mockImplementationOnce(async (sessionId) => {
      broadcastMockUpdate(incomingUser);
      broadcastMockUpdate(replayedWorkflow);
      return {
        sessionId,
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      };
    });

    const renderedIds = ['outgoing-user'];
    const kiro = new Kiro();
    const liveHandler = Object.assign(
      (event: AgentStreamEvent) =>
        renderedIds.push('id' in event ? event.id : event.type),
      {
        resetSession: mock(() => {}),
        setHistoryReplay: mock((_value: boolean) => {}),
      }
    );
    kiro.onLiveContent(liveHandler);
    kiro.onWorkflowProgress(
      (event: WorkflowProgressStreamEvent, source: WorkflowProgressSource) => {
        if (source === 'live') renderedIds.push(event.id);
      }
    );
    await kiro.initialize('/path/to/agent');

    const buffered: AgentStreamEvent[] = [];
    await kiro.loadSession('incoming-session', (event: AgentStreamEvent) =>
      buffered.push(event)
    );

    expect(renderedIds).toEqual(['outgoing-user']);
    expect(
      buffered.map((event) => ('id' in event ? event.id : event.type))
    ).toEqual(['incoming-user', 'replayed-workflow']);

    expect(kiro.replayHistory(buffered)).toBe(true);
    expect(renderedIds).toEqual([
      'outgoing-user',
      'incoming-user',
      'replayed-workflow',
    ]);

    broadcastMockUpdate(liveWorkflow);
    expect(renderedIds).toEqual([
      'outgoing-user',
      'incoming-user',
      'replayed-workflow',
      'live-workflow',
    ]);
  });

  it('routes steering lifecycle through the persistent handler during a local prompt', async () => {
    let resolvePrompt!: () => void;
    mockSessionClient.prompt.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolvePrompt = resolve;
        })
    );
    const kiro = new Kiro();
    const liveHandler = mock(() => {});
    const promptHandler = mock(() => {});
    kiro.onLiveContent(liveHandler);
    await kiro.initialize('/path/to/agent');
    await kiro.createSession();

    const prompt = kiro.streamMessage(
      'hello',
      new AbortController().signal,
      promptHandler
    );
    broadcastMockUpdate({
      type: AgentEventType.SteeringConsumed,
      content: 'finish with a summary',
    } as AgentStreamEvent);

    expect(liveHandler).toHaveBeenCalledTimes(1);
    expect(promptHandler).not.toHaveBeenCalled();

    resolvePrompt();
    await prompt;
  });

  it('onLiveContent receives refusal/retry/error events when no local prompt owns the turn', async () => {
    // An observer turn must end with an explanation, not just a stopped
    // spinner: refusal, retry, auth, and session errors ride the same route
    // as content.
    const kiro = new Kiro();
    const handler = mock(() => {});
    kiro.onLiveContent(handler);
    await kiro.initialize('/path/to/agent');
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler({
        type: AgentEventType.ModelRefusal,
      } as AgentStreamEvent);
      mockOnUpdateHandler({
        type: AgentEventType.RetryWarning,
        attempt: 1,
        maxAttempts: 3,
        delaySecs: 1,
        message: 'retrying',
      } as AgentStreamEvent);
      mockOnUpdateHandler({
        type: AgentEventType.AuthError,
        message: 'auth failed',
      } as AgentStreamEvent);
      mockOnUpdateHandler({
        type: AgentEventType.SessionError,
        message: 'session lost',
      } as AgentStreamEvent);
    }
    expect(handler).toHaveBeenCalledTimes(4);
  });

  it('onTurnSummary receives TurnSummary events', async () => {
    const kiro = new Kiro();
    const handler = mock(() => {});
    const historyHandler = mock(() => {});
    const liveHandler = mock(() => {});
    kiro.onTurnSummary(handler);
    kiro.onHistoryEvent(historyHandler);
    kiro.onLiveContent(liveHandler);
    await kiro.initialize('/path/to/agent');
    const event = {
      type: AgentEventType.TurnSummary,
      meteringUsage: [],
    } as AgentStreamEvent;
    if (mockOnUpdateHandler) {
      mockOnUpdateHandler(event);
    }
    expect(historyHandler).toHaveBeenCalledWith(event);
    expect(handler).toHaveBeenCalledWith(event);
    expect(liveHandler).toHaveBeenCalledWith(event);
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

  it('setSessionMode throws when not initialized', async () => {
    const kiro = new Kiro();
    await expect(kiro.setSessionMode('autonomous')).rejects.toThrow(
      'Kiro not initialized'
    );
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

  it('loadSession rejects after the deadline instead of wedging transitions', async () => {
    const previousTimeout = process.env.KIRO_SESSION_LOAD_TIMEOUT_MS;
    process.env.KIRO_SESSION_LOAD_TIMEOUT_MS = '25';
    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      await kiro.createSession();
      mockSessionClient.loadSession.mockImplementation(
        () => new Promise(() => {}) // never settles
      );
      await expect(kiro.loadSession('session-hung')).rejects.toThrow(
        'timed out'
      );
      // The transition chain must be free again: a subsequent load works.
      mockSessionClient.loadSession.mockImplementation((id: string) =>
        Promise.resolve({
          sessionId: id,
          currentModel: { id: 'model-1', name: 'Test Model' },
          currentAgent: { name: 'test-agent' },
        })
      );
      const result = await kiro.loadSession('session-after');
      expect(result.sessionId).toBe('session-after');
    } finally {
      if (previousTimeout === undefined) {
        delete process.env.KIRO_SESSION_LOAD_TIMEOUT_MS;
      } else {
        process.env.KIRO_SESSION_LOAD_TIMEOUT_MS = previousTimeout;
      }
    }
  });

  it('tells the client to abandon a load that missed its deadline', async () => {
    const previousTimeout = process.env.KIRO_SESSION_LOAD_TIMEOUT_MS;
    process.env.KIRO_SESSION_LOAD_TIMEOUT_MS = '25';
    const abandoned = mock(() => {});
    try {
      const kiro = new Kiro();
      await kiro.initialize('/path/to/agent');
      await kiro.createSession();

      // A load that misses the deadline: the client must be told before the
      // caller rolls back, so a late RPC response cannot seize ownership.
      let releaseLoad!: (value: {
        sessionId: string;
        currentModel: { id: string; name: string };
        currentAgent: { name: string };
      }) => void;
      mockSessionClient.abandonPendingLoad = abandoned;
      mockSessionClient.loadSession.mockImplementation(
        () =>
          new Promise((resolve) => {
            releaseLoad = resolve;
          })
      );
      await expect(kiro.loadSession('session-late')).rejects.toThrow(
        'timed out'
      );
      expect(abandoned).toHaveBeenCalledTimes(1);

      // Settle the abandoned RPC so nothing dangles into the next test.
      releaseLoad({
        sessionId: 'session-late',
        currentModel: { id: 'model-1', name: 'Test Model' },
        currentAgent: { name: 'test-agent' },
      });
      await new Promise((r) => setTimeout(r, 5));
    } finally {
      mockSessionClient.abandonPendingLoad = undefined;
      mockSessionClient.loadSession.mockImplementation((id: string) =>
        Promise.resolve({
          sessionId: id,
          currentModel: { id: 'model-1', name: 'Test Model' },
          currentAgent: { name: 'test-agent' },
        })
      );
      if (previousTimeout === undefined) {
        delete process.env.KIRO_SESSION_LOAD_TIMEOUT_MS;
      } else {
        process.env.KIRO_SESSION_LOAD_TIMEOUT_MS = previousTimeout;
      }
    }
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
