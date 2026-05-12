import {
  describe,
  it,
  expect,
  mock,
  beforeEach,
  afterEach,
  afterAll,
} from 'bun:test';
import { EventEmitter } from 'events';
import { AgentEventType, ContentType } from '../types/agent-events';

// --- Mock child_process ---
function createMockStream() {
  const emitter = new EventEmitter();
  (emitter as any).destroy = mock(() => {});
  return emitter;
}

function createMockStdin() {
  const emitter = new EventEmitter();
  (emitter as any).write = mock(
    (_chunk: any, cb?: (...args: any[]) => void) => {
      if (cb) cb();
      return true;
    }
  );
  (emitter as any).end = mock(() => {});
  (emitter as any).destroy = mock(() => {});
  (emitter as any).destroyed = false;
  (emitter as any).writableEnded = false;
  return emitter;
}

let mockProcess: any;
const mockSpawn = mock((_cmd: string, _args: string[], _opts: any) => {
  mockProcess = {
    stdin: createMockStdin(),
    stdout: createMockStream(),
    stderr: createMockStream(),
    kill: mock(() => {}),
    pid: 12345,
    on: mock(() => {}),
    once: mock(() => {}),
    off: mock(() => {}),
  };
  return mockProcess;
});

mock.module('child_process', () => ({ spawn: mockSpawn }));
mock.module('node:child_process', () => ({ spawn: mockSpawn }));

// --- Mock @kiro/client ---
let capturedSessionUpdateHandler: any = null;
let capturedPermissionHandler: any = null;

const mockKiroInitialize = mock(() =>
  Promise.resolve({
    protocolVersion: '1.0',
    agentCapabilities: {
      _meta: {
        kiro: {
          extensionMethods: [
            {
              method: '_kiro/help',
              name: '/help',
              description: 'Show available commands',
            },
            {
              method: '_kiro/clear',
              name: '/clear',
              description: 'Clear conversation',
            },
            {
              method: '_kiro/plan',
              name: '/plan',
              description: 'Switch to plan agent',
            },
            {
              method: '_kiro/session/list',
              name: '/session list',
              description: 'List sessions',
            },
            {
              method: '_kiro/session/delete',
              name: '/session delete',
              description: 'Delete a session',
            },
          ],
        },
      },
    },
  })
);
const mockKiroNewSession = mock((_req: any) =>
  Promise.resolve({
    sessionId: 'kas-session-1',
    models: {
      currentModelId: 'm1',
      availableModels: [{ modelId: 'm1', name: 'Test Model' }],
    },
    modes: null,
  })
);
const mockKiroLoadSession = mock((_req: any) =>
  Promise.resolve({
    sessionId: 'kas-loaded',
    models: null,
    modes: null,
  })
);
const mockKiroPrompt = mock((_req: any) => Promise.resolve());
const mockKiroCancel = mock((_sessionId: string) => {});
const mockKiroSetSessionConfigOption = mock((_req: any) => Promise.resolve());
const mockKiroSendExtMethod = mock((_method: string, _params: any) =>
  Promise.resolve({})
);
const mockKiroListSessions = mock(() =>
  Promise.resolve({ sessions: [] as Array<Record<string, unknown>> })
);

const mockSessionUpdateDispose = mock(() => {});
const mockPermissionRequestDispose = mock(() => {});

const MockKiroClient = class {
  initialize = mockKiroInitialize;
  newSession = mockKiroNewSession;
  loadSession = mockKiroLoadSession;
  prompt = mockKiroPrompt;
  cancel = mockKiroCancel;
  setSessionConfigOption = mockKiroSetSessionConfigOption;
  sendExtMethod = mockKiroSendExtMethod;
  listSessions = mockKiroListSessions;
  onSessionUpdate = mock((_sessionId: string, handler: any) => {
    capturedSessionUpdateHandler = handler;
    return { dispose: mockSessionUpdateDispose };
  });
  onPermissionRequest = mock((_sessionId: string, handler: any) => {
    capturedPermissionHandler = handler;
    return { dispose: mockPermissionRequestDispose };
  });
  constructor(_config: any) {}
};

mock.module('@kiro/client', () => ({
  KiroClient: MockKiroClient,
}));

// --- Mock @agentclientprotocol/sdk ---
mock.module('@agentclientprotocol/sdk', () => ({
  ndJsonStream: (_writable: any, _readable: any) => ({
    readable: new ReadableStream(),
    writable: new WritableStream(),
  }),
  PROTOCOL_VERSION: '1.0',
}));

// --- Mock logger ---
mock.module('../utils/logger', () => ({
  logger: {
    debug: () => {},
    error: () => {},
    warn: () => {},
    info: () => {},
  },
}));

afterAll(() => {
  mock.restore();
});

// @ts-expect-error — bun-specific query-string import
const { KasAcpClient } = await import('../acp-client?kas-test');

function freshMocks() {
  mockSpawn.mockClear();
  mockKiroInitialize.mockClear();
  mockKiroNewSession.mockClear();
  mockKiroLoadSession.mockClear();
  mockKiroPrompt.mockClear();
  mockKiroCancel.mockClear();
  mockKiroSetSessionConfigOption.mockClear();
  mockKiroSendExtMethod.mockClear();
  mockKiroListSessions.mockClear();
  capturedSessionUpdateHandler = null;
  capturedPermissionHandler = null;
  mockSessionUpdateDispose.mockClear();
  mockPermissionRequestDispose.mockClear();
  mockSpawn.mockImplementation((_cmd: string, _args: string[], _opts: any) => {
    mockProcess = {
      stdin: createMockStdin(),
      stdout: createMockStream(),
      stderr: createMockStream(),
      kill: mock(() => {}),
      pid: 12345,
      on: mock(() => {}),
      once: mock(() => {}),
      off: mock(() => {}),
    };
    return mockProcess;
  });
}

describe('KasAcpClient', () => {
  let origKasPath: string | undefined;

  beforeEach(() => {
    origKasPath = process.env.KIRO_KAS_SERVER_PATH;
    process.env.KIRO_KAS_SERVER_PATH = '/fake/acp-server.js';
    freshMocks();
  });

  afterEach(() => {
    if (origKasPath === undefined) delete process.env.KIRO_KAS_SERVER_PATH;
    else process.env.KIRO_KAS_SERVER_PATH = origKasPath;
  });

  it('constructor spawns process with KAS server args', () => {
    const _client = new KasAcpClient();
    expect(mockSpawn).toHaveBeenCalled();
    const [_cmd, args] = mockSpawn.mock.calls[0]!;
    expect(args).toContain('--experimental-wasm-modules');
    expect(args).toContain('--transport=stdio');
  });

  it('close() calls kill("SIGTERM") on the agent process', () => {
    const client = new KasAcpClient();
    client.close();
    expect(mockProcess.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('initialize() calls kiroClient.initialize', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    expect(mockKiroInitialize).toHaveBeenCalledTimes(1);
  });

  it('newSession() creates session and sets autopilot config', async () => {
    const client = new KasAcpClient();
    const result = await client.newSession();

    expect(mockKiroNewSession).toHaveBeenCalledTimes(1);
    expect(result.sessionId).toBe('kas-session-1');
    expect(result.currentModel).toEqual({ id: 'm1', name: 'Test Model' });

    // Should set autopilot config
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'autopilot', value: 'on' })
    );
  });

  it('newSession() wires session update and permission listeners', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // onSessionUpdate and onPermissionRequest should be registered
    expect(capturedSessionUpdateHandler).not.toBeNull();
    expect(capturedPermissionHandler).not.toBeNull();
  });

  it('loadSession() loads session and wires listeners', async () => {
    const client = new KasAcpClient();
    const result = await client.loadSession('existing-session');

    expect(mockKiroLoadSession).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: 'existing-session' })
    );
    expect(result.sessionId).toBe('existing-session');
    expect(capturedSessionUpdateHandler).not.toBeNull();
  });

  it('loadSession() disposes previous session listeners', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // First session wired — dispose not yet called
    expect(mockSessionUpdateDispose).not.toHaveBeenCalled();
    expect(mockPermissionRequestDispose).not.toHaveBeenCalled();

    // Switch session — old listeners should be disposed
    await client.loadSession('second-session');
    expect(mockSessionUpdateDispose).toHaveBeenCalledTimes(1);
    expect(mockPermissionRequestDispose).toHaveBeenCalledTimes(1);
  });

  it('prompt() throws when no session is active', async () => {
    const client = new KasAcpClient();
    expect(
      client.prompt([{ type: 'text', text: 'hello' } as any])
    ).rejects.toThrow('cannot send prompt without an active session');
  });

  it('prompt() calls kiroClient.prompt with session', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    await client.prompt([{ type: 'text', text: 'hello' } as any]);

    expect(mockKiroPrompt).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'kas-session-1',
        prompt: [{ type: 'text', text: 'hello' }],
      })
    );
  });

  it('cancel() calls kiroClient.cancel with sessionId', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    await client.cancel();
    expect(mockKiroCancel).toHaveBeenCalledWith('kas-session-1');
  });

  it('cancel() does nothing when no session', async () => {
    const client = new KasAcpClient();
    await client.cancel();
    expect(mockKiroCancel).not.toHaveBeenCalled();
  });

  // ── executeCommand routing ──

  it('executeCommand("quit") returns success without forwarding', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({ command: 'quit' } as any);
    expect(result.success).toBe(true);
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
  });

  it('executeCommand("help") forwards to agent via _kiro/help', async () => {
    mockKiroSendExtMethod.mockResolvedValue({
      commands: [{ name: '/help', description: 'Show help' }],
    });
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    await client.executeCommand({ command: 'help' } as any);
    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/help',
      expect.objectContaining({ sessionId: 'kas-session-1' })
    );
  });

  it('executeCommand("plan") forwards to agent via _kiro/plan', async () => {
    mockKiroSendExtMethod.mockResolvedValue({});
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    await client.executeCommand({ command: 'plan' } as any);
    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/plan',
      expect.objectContaining({ sessionId: 'kas-session-1' })
    );
  });

  it('executeCommand("agent") (no args) derives agent list from cached modes, not _kiro/agent/list', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Vibe',
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'research',
            name: 'Research',
            description: 'Deep investigation',
            _meta: { kiro: { source: 'user' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    const result = await client.executeCommand({ command: 'agent' } as any);

    // Must NOT call the removed extension method.
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/agent/list',
      expect.anything()
    );
    expect(result.success).toBe(true);
    const data = result.data as {
      agents: Array<{ name: string; description: string }>;
      current: string;
    };
    expect(data.current).toBe('vibe');
    expect(data.agents.map((a) => a.name)).toEqual(['vibe', 'research']);
  });

  it('executeCommand("clear") creates a new session via session/new primitive', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession(); // seed an initial session
    mockKiroNewSession.mockClear();
    mockKiroSendExtMethod.mockClear();

    // Second newSession returns a different sessionId so we can tell clear worked
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-2',
      models: {
        currentModelId: 'm1',
        availableModels: [{ modelId: 'm1', name: 'Test Model' }],
      },
      modes: null,
    } as any);

    const result = await client.executeCommand({ command: 'clear' } as any);

    // Should have called newSession on the ACP client (not _kiro/clear ext method)
    expect(mockKiroNewSession).toHaveBeenCalledTimes(1);
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/clear',
      expect.anything()
    );

    // Result surfaces the new session so the effect can update the TUI
    expect(result.success).toBe(true);
    expect((result.data as any)?.sessionId).toBe('kas-session-2');
    expect((result.data as any)?.currentModel).toEqual({
      id: 'm1',
      name: 'Test Model',
    });
  });

  it('executeCommand("clear") returns error when newSession rejects', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroNewSession.mockRejectedValueOnce(new Error('kas is down'));

    const result = await client.executeCommand({ command: 'clear' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('kas is down');
  });

  it('executeCommand("agent") with agentName swaps via setSessionConfigOption', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();
    const result = await client.executeCommand({
      command: 'agent',
      args: { agentName: 'research' },
    } as any);
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      configId: 'mode',
      value: 'research',
    });
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ agent: { name: 'research' } });
  });

  it('executeCommand("agent") with value arg strips swap prefix', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();
    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'swap docs' },
    } as any);
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      configId: 'mode',
      value: 'docs',
    });
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ agent: { name: 'docs' } });
  });

  it('executeCommand("agent create") never calls setSessionConfigOption (does not interpret "create" as a mode name)', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();

    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'create' },
    } as any);

    expect(mockKiroSetSessionConfigOption).not.toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'mode', value: 'create' })
    );
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/\/agent create.*not yet implemented/);
  });

  it('executeCommand("agent edit foo") never calls setSessionConfigOption (does not interpret "edit foo" as a mode name)', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();

    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'edit foo' },
    } as any);

    expect(mockKiroSetSessionConfigOption).not.toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'mode', value: 'edit foo' })
    );
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/\/agent edit.*not yet implemented/);
  });

  it('executeCommand("agent swap") (no name) surfaces a usage error instead of swapping', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();

    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'swap' },
    } as any);

    expect(mockKiroSetSessionConfigOption).not.toHaveBeenCalled();
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/\/agent swap <name>/);
  });

  it('executeCommand("chat delete") forwards to _kiro/session/delete', async () => {
    mockKiroSendExtMethod.mockResolvedValue({ success: true });
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    await client.executeCommand({
      command: 'chat',
      args: { value: 'delete abc123' },
    } as any);
    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/session/delete',
      expect.objectContaining({ sessionId: 'abc123' })
    );
  });

  it('executeCommand("chat save") returns unsupported in KAS mode', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'chat',
      args: { value: 'save my-session' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not yet supported in KAS mode');
  });

  it('executeCommand("reply") returns success without forwarding', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({ command: 'reply' } as any);
    expect(result.success).toBe(true);
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
  });

  it('executeCommand with unknown command returns unsupported', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'unknown_cmd',
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not yet supported in KAS mode');
  });

  // ── /paste command ──
  //
  // /paste composes entirely client-side in the TUI: it reads the system
  // clipboard and returns the image in a CommandResult. This test just
  // checks the dispatch wiring — it doesn't exercise the clipboard read
  // itself, which requires the real OS clipboard.

  it('executeCommand("paste") returns a CommandResult without forwarding to KAS', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    const result = await client.executeCommand({ command: 'paste' } as any);

    // Result shape: CommandResult has { success: boolean }.
    // The actual value depends on the real clipboard at test time — in CI
    // it's empty and we get `success: false` — but we never want to see
    // the generic "not yet supported in KAS mode" message.
    expect(typeof result.success).toBe('boolean');
    expect(result.message ?? '').not.toContain('not yet supported');

    // Client-side composition — /paste must never reach the agent.
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
  });

  // ── getCommandOptions ──

  it('getCommandOptions("feedback") returns static options', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/feedback', '');
    expect(result.options.length).toBe(3);
    expect(result.options.map((o: any) => o.value)).toEqual([
      'general',
      'feature',
      'issue',
    ]);
  });

  it('getCommandOptions returns empty when no session', async () => {
    const client = new KasAcpClient();
    const result = await client.getCommandOptions('/help', '');
    expect(result.options).toEqual([]);
  });

  it('getCommandOptions("/agent") derives options from cached session modes, grouped by _meta.kiro.source', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Vibe',
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'spec',
            name: 'Spec',
            description: 'Structured feature development',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'reviewer',
            name: 'Reviewer',
            description: 'Reviews code changes',
            _meta: { kiro: { source: 'user' } },
          },
          {
            id: 'legacy',
            name: 'Legacy',
            description: 'No source metadata',
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    const result = await client.getCommandOptions('/agent', '');

    // Must NOT fall back to the removed extension method.
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/agent/list',
      expect.anything()
    );
    expect(result.options).toEqual([
      {
        value: 'vibe',
        label: 'Vibe',
        description: '[active] General coding assistance',
        group: 'Bundled',
      },
      {
        value: 'spec',
        label: 'Spec',
        description: 'Structured feature development',
        group: 'Bundled',
      },
      {
        value: 'reviewer',
        label: 'Reviewer',
        description: 'Reviews code changes',
        group: 'User',
      },
      {
        value: 'legacy',
        label: 'Legacy',
        description: 'No source metadata',
      },
    ]);
  });

  it('getCommandOptions("/agent") returns empty options when the agent advertised no modes', async () => {
    // Default mockKiroNewSession returns `modes: null`.
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/agent', '');
    expect(result.options).toEqual([]);
  });

  it('current_mode_update notification updates the cached currentModeId used by /agent', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Vibe',
            description: '',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'spec',
            name: 'Spec',
            description: '',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();

    // Simulate the agent switching modes mid-session.
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'current_mode_update',
        currentModeId: 'spec',
      },
    });

    const result = await client.getCommandOptions('/agent', '');
    const active = result.options.find((o: { description?: string }) =>
      (o.description ?? '').startsWith('[active]')
    );
    expect(active?.value).toBe('spec');
  });

  // ── Session update broadcasting ──

  it('session update broadcasts Content event via onUpdate handler', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    // Simulate agent sending a session update
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'Hello from KAS' },
      },
    });

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.Content);
    expect(event.content.type).toBe(ContentType.Text);
    expect(event.content.text).toBe('Hello from KAS');
  });

  it('session update broadcasts ToolCall event', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-1',
        title: 'fs_write',
        kind: 'edit',
        rawInput: { path: '/test' },
        content: [],
        locations: [],
      },
    });

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.ToolCall);
    expect(event.id).toBe('tc-1');
    expect(event.name).toBe('fs_write');
  });

  // ── Stub methods ──

  it('listSettings returns empty object', async () => {
    const client = new KasAcpClient();
    const result = await client.listSettings();
    expect(result).toEqual({});
  });

  it('spawnSession returns empty sessionId', async () => {
    const client = new KasAcpClient();
    const result = await client.spawnSession('do something', 'my-task');
    expect(result).toEqual({ sessionId: '', name: 'my-task' });
  });

  it('listSessions returns sessions from kiroClient', async () => {
    mockKiroListSessions.mockResolvedValue({
      sessions: [
        {
          sessionId: 's1',
          cwd: '/tmp',
          title: 'Test',
          updatedAt: '2026-01-01',
        },
      ],
    });
    const client = new KasAcpClient();
    const result = await client.listSessions('/tmp');
    expect(result.sessions.length).toBe(1);
    expect(result.sessions[0]!.sessionId).toBe('s1');
  });

  // ── /model command ──

  /**
   * Seed a newSession response that mirrors what KAS actually returns
   * for an agent with a ModelConfigProvider: a SessionConfigOption list
   * containing a `category: 'model'` entry with currentValue + options.
   */
  function seedSessionWithModels(opts: {
    currentValue: string;
    models: Array<{ value: string; name: string; description?: string }>;
  }): void {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-models',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'model',
          name: 'Model',
          category: 'model',
          currentValue: opts.currentValue,
          options: opts.models,
        },
      ],
    } as any);
  }

  it('newSession extracts currentModel from configOptions (KAS shape)', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    const result = await client.newSession();
    expect(result.currentModel).toEqual({ id: 'claude-4', name: 'Claude 4' });
  });

  it('getCommandOptions("/model") returns cached options from configOptions', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4', description: 'Best overall' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/model', '');
    expect(result.options.length).toBe(2);
    expect(result.options[0]).toEqual({
      value: 'claude-4',
      label: 'Claude 4',
      description: '[active] Best overall',
    });
    expect(result.options[1]).toEqual({
      value: 'gpt-5',
      label: 'GPT-5',
      description: '',
    });
  });

  it('getCommandOptions("/model") returns empty when no models configured', async () => {
    // Default mock returns no configOptions → no model cache
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/model', '');
    expect(result.options).toEqual([]);
  });

  it('executeCommand("model") switches via setSessionConfigOption and refreshes cache', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // KAS returns the full configOptions state reflecting the switch
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'model',
          name: 'Model',
          category: 'model',
          currentValue: 'gpt-5',
          options: [
            { value: 'claude-4', name: 'Claude 4' },
            { value: 'gpt-5', name: 'GPT-5' },
          ],
        },
      ],
    } as any);

    mockKiroSetSessionConfigOption.mockClear();
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'gpt-5' },
    } as any);

    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-models',
      configId: 'model',
      value: 'gpt-5',
    });
    expect(result.success).toBe(true);
    expect(result.message).toBe('Switched to GPT-5');
    expect(result.data).toEqual({ model: { id: 'gpt-5', name: 'GPT-5' } });

    // Cache should now mark gpt-5 as active
    const options = await client.getCommandOptions('/model', '');
    const activeEntry = options.options.find((o: any) =>
      o.description?.startsWith('[active]')
    );
    expect(activeEntry?.value).toBe('gpt-5');
  });

  it('executeCommand("model") without a value returns usage error', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [{ value: 'claude-4', name: 'Claude 4' }],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.executeCommand({ command: 'model' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('Usage');
  });

  it('executeCommand("model") without a value and no models returns "No models available"', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({ command: 'model' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toBe('No models available');
  });

  it('executeCommand("model") returns error when KAS rejects to a different value', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // Simulate KAS ignoring an unknown id and leaving the selection unchanged
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'model',
          name: 'Model',
          category: 'model',
          currentValue: 'claude-4',
          options: [
            { value: 'claude-4', name: 'Claude 4' },
            { value: 'gpt-5', name: 'GPT-5' },
          ],
        },
      ],
    } as any);

    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'nonexistent' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain("'nonexistent' not available");
  });

  it('config_option_update session notification refreshes the model cache', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // KAS autonomously switches models (e.g. rate-limit fallback)
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-models',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'model',
            name: 'Model',
            category: 'model',
            currentValue: 'gpt-5',
            options: [
              { value: 'claude-4', name: 'Claude 4' },
              { value: 'gpt-5', name: 'GPT-5' },
            ],
          },
        ],
      },
    });

    const options = await client.getCommandOptions('/model', '');
    const activeEntry = options.options.find((o: any) =>
      o.description?.startsWith('[active]')
    );
    expect(activeEntry?.value).toBe('gpt-5');
  });

  // ── /prompts command ──

  it('executeCommand("prompts") with no args returns success message', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({ command: 'prompts' } as any);
    expect(result.success).toBe(true);
    expect(result.message).toBe('Use the selection menu to pick a prompt.');
  });

  it('executeCommand("prompts") with a name returns executePrompt data', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    (client as any).promptsCache = [
      {
        name: 'research',
        description: 'Deep research',
        arguments: [],
        serverName: 'builder-mcp',
      },
    ];
    const result = await client.executeCommand({
      command: 'prompts',
      args: { value: 'research' },
    } as any);
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ executePrompt: '/research' });
  });

  it('executeCommand("prompts") with any name returns executePrompt data (no validation)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'prompts',
      args: { value: 'anything' },
    } as any);
    expect(result.success).toBe(true);
    expect(result.data).toEqual({ executePrompt: '/anything' });
  });

  it('getCommandOptions("/prompts") computes hint from multiple arguments', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    (client as any).promptsCache = [
      {
        name: 'search',
        description: 'Search docs',
        arguments: [
          { name: 'query', required: true },
          { name: 'context', required: false },
        ],
        serverName: 'mcp-server',
      },
    ];
    const result = await client.getCommandOptions('/prompts', '');
    expect(result.options[0]!.hint).toBe('<query> [context]');
  });

  it('getCommandOptions("/prompts") returns formatted options from cache', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    (client as any).promptsCache = [
      {
        name: 'research',
        description: 'Deep research',
        arguments: [{ name: 'topic', required: true }],
        serverName: 'builder-mcp',
      },
      {
        name: 'summarize',
        description: '',
        arguments: [{ name: 'length', required: false }],
        serverName: 'builder-mcp',
      },
    ];

    const result = await client.getCommandOptions('/prompts', '');
    expect(result.options).toEqual([
      {
        value: 'research',
        label: '/research',
        description: 'Deep research',
        group: 'builder-mcp',
        hint: '<topic>',
      },
      {
        value: 'summarize',
        label: '/summarize',
        description: '',
        group: 'builder-mcp',
        hint: '[length]',
      },
    ]);
  });

  it('getCommandOptions("/prompts") returns empty when no prompts cached', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/prompts', '');
    expect(result.options).toEqual([]);
  });

  it('getCommandOptions("/prompts") sorts by group then label', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    (client as any).promptsCache = [
      {
        name: 'zebra',
        description: '',
        arguments: [],
        serverName: 'workspace',
      },
      {
        name: 'alpha',
        description: '',
        arguments: [],
        serverName: 'builder-mcp',
      },
      {
        name: 'beta',
        description: '',
        arguments: [],
        serverName: 'builder-mcp',
      },
    ];

    const result = await client.getCommandOptions('/prompts', '');
    expect(result.options.map((o: any) => o.value)).toEqual([
      'alpha',
      'beta',
      'zebra',
    ]);
  });

  it('getCommandOptions("/prompts") handles prompts with no arguments (hint is undefined)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    (client as any).promptsCache = [
      {
        name: 'simple',
        description: 'No args',
        arguments: [],
        serverName: 'test',
      },
    ];

    const result = await client.getCommandOptions('/prompts', '');
    expect(result.options[0]!.hint).toBeUndefined();
  });

  // ── available_commands_update populates promptsCache ──

  it('available_commands_update session notification populates promptsCache from prompt-type commands', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // Simulate the KAS session update — prompts arrive as commands with _meta.kiro.type
    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
          {
            name: 'research',
            description: 'Deep research',
            _meta: { kiro: { type: 'skill' } },
          },
          {
            name: 'plan',
            description: 'Create a plan',
            _meta: { kiro: { type: 'steering' } },
          },
        ],
      },
    });

    // Verify promptsCache was populated from prompt-type commands only
    expect((client as any).promptsCache).toHaveLength(2);
    const names = (client as any).promptsCache.map((p: any) => p.name).sort();
    expect(names).toEqual(['plan', 'research']);

    // Verify getCommandOptions now returns the prompts
    const options = await client.getCommandOptions('/prompts', '');
    expect(options.options).toHaveLength(2);
    // Verify serverName mapping: steering → workspace, skill → skill
    const groups = options.options.map((o: any) => o.group);
    expect(groups.sort()).toEqual(['skill', 'workspace']);
  });

  it('available_commands_update with no prompt-type commands clears promptsCache', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // Pre-populate cache
    (client as any).promptsCache = [
      { name: 'old', description: '', arguments: [], serverName: 'workspace' },
    ];

    // Send update with only non-prompt commands
    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
        ],
      },
    });

    expect((client as any).promptsCache).toHaveLength(0);

  // ── /knowledge command ──

  describe('knowledge command', () => {
    function seedInitializeWithKnowledge() {
      mockKiroInitialize.mockResolvedValueOnce({
        protocolVersion: '1.0',
        agentCapabilities: {
          _meta: {
            kiro: {
              extensionMethods: [
                { method: '_kiro/knowledge', name: '/knowledge', description: 'Manage knowledge' },
              ],
            },
          },
        },
      });
    }

    it('executeCommand knowledge show calls ext method with { subcommand: "show" }', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ entries: [] });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      const result = await client.executeCommand({
        command: 'knowledge',
        args: { value: 'show' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'show' })
      );
      expect(result.success).toBe(true);
      expect((result.data as any).entries).toEqual([]);
    });

    it('executeCommand knowledge add parses name and path correctly', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ message: 'Started indexing' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: 'add my-kb /path/to/dir' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({
          subcommand: 'add',
          name: 'my-kb',
          path: '/path/to/dir',
        })
      );
    });

    it('executeCommand knowledge remove parses target correctly', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ message: 'Removed' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: 'remove my-kb' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'remove', target: 'my-kb' })
      );
    });

    it('executeCommand knowledge update parses path correctly', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ message: 'Re-indexing' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: 'update /path/to/dir' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'update', path: '/path/to/dir' })
      );
    });

    it('executeCommand knowledge cancel parses operationId correctly', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ message: 'Cancelled' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: 'cancel abc123' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'cancel', operationId: 'abc123' })
      );
    });

    it('executeCommand knowledge clear sends correct subcommand', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ message: 'Cleared 2 entries' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: 'clear' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'clear' })
      );
    });

    it('propagates errors from ext method', async () => {
      seedInitializeWithKnowledge();
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockRejectedValueOnce(new Error('Store error'));

      const result = await client.executeCommand({
        command: 'knowledge',
        args: { value: 'show' },
      } as any);

      expect(result.success).toBe(false);
      expect(result.message).toBe('Store error');
    });

    it('defaults to show when no args', async () => {
      seedInitializeWithKnowledge();
      mockKiroSendExtMethod.mockResolvedValue({ entries: [] });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      mockKiroSendExtMethod.mockClear();

      await client.executeCommand({
        command: 'knowledge',
        args: { value: '' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/knowledge',
        expect.objectContaining({ subcommand: 'show' })
      );
    });
  });
});
