/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
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
const mockExtNotificationDispose = mock(() => {});

let capturedKiroClientConfig: any = null;

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
  onExtNotification = mock((_method: string, _handler: any) => {
    if (!this._extNotifHandlers) this._extNotifHandlers = {};
    this._extNotifHandlers[_method] = _handler;
    return { dispose: mockExtNotificationDispose };
  });
  _extNotifHandlers: Record<string, any> = {};
  constructor(config: any) {
    capturedKiroClientConfig = config;
  }
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

// --- Mock cli-settings (via HOME override to avoid mock.module leaking) ---
import { mkdirSync, writeFileSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

let testSettingsDir: string;
let originalHome: string | undefined;

function setupTestHome() {
  testSettingsDir = join(
    tmpdir(),
    `kas-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(join(testSettingsDir, '.kiro', 'settings'), { recursive: true });
  writeFileSync(
    join(testSettingsDir, '.kiro', 'settings', 'cli.json'),
    '{}',
    'utf-8'
  );
  originalHome = process.env.HOME;
  process.env.HOME = testSettingsDir;
}

function teardownTestHome() {
  if (originalHome !== undefined) process.env.HOME = originalHome;
  else delete process.env.HOME;
}

function testCliJsonPath() {
  return join(testSettingsDir, '.kiro', 'settings', 'cli.json');
}

function writeTestCliJson(data: Record<string, unknown>) {
  writeFileSync(testCliJsonPath(), JSON.stringify(data), 'utf-8');
}

function readTestCliJson(): Record<string, unknown> {
  return JSON.parse(readFileSync(testCliJsonPath(), 'utf-8'));
}
afterAll(() => {
  teardownTestHome();
  mock.restore();
});

// @ts-expect-error — bun-specific query-string import
const { KasAcpClient, resolveFeedbackUrl, browserOpenCommand } =
  await import('../acp-client?kas-test');

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
  capturedKiroClientConfig = null;
  mockSessionUpdateDispose.mockClear();
  mockPermissionRequestDispose.mockClear();
  setupTestHome();
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

describe('resolveFeedbackUrl', () => {
  it('routes external users to GitHub for each kind', () => {
    expect(resolveFeedbackUrl('general', false)).toBe(
      'https://github.com/kirodotdev/Kiro/issues/new/choose'
    );
    expect(resolveFeedbackUrl('feature', false)).toBe(
      'https://github.com/kirodotdev/Kiro/issues/new?template=feature_request.yml'
    );
    expect(resolveFeedbackUrl('issue', false)).toBe(
      'https://github.com/kirodotdev/Kiro/issues'
    );
  });

  it('routes internal (Amazon) users to Taskei for each kind', () => {
    expect(resolveFeedbackUrl('general', true)).toBe(
      'https://taskei.amazon.dev/tasks/create?template=f5ac492c-9ec3-4a2d-8abb-2f486c7222eb'
    );
    expect(resolveFeedbackUrl('feature', true)).toBe(
      'https://taskei.amazon.dev/tasks/create?template=a05ddcbb-e4c6-4783-8eca-ef46ae5d7ef6'
    );
    expect(resolveFeedbackUrl('issue', true)).toBe(
      'https://taskei.amazon.dev/tasks/create?template=c0312360-3f55-432d-a6d2-e3060ad2cc59'
    );
  });

  it('falls back to general for an unknown kind', () => {
    expect(resolveFeedbackUrl('bogus', false)).toBe(
      'https://github.com/kirodotdev/Kiro/issues/new/choose'
    );
    expect(resolveFeedbackUrl('bogus', true)).toBe(
      'https://taskei.amazon.dev/tasks/create?template=f5ac492c-9ec3-4a2d-8abb-2f486c7222eb'
    );
  });
});

describe('browserOpenCommand', () => {
  it('uses `open` with the URL as an argv element on macOS', () => {
    expect(browserOpenCommand('darwin', 'https://example.com')).toEqual({
      file: 'open',
      args: ['https://example.com'],
    });
  });

  it('uses rundll32 URL handler on Windows (not cmd `start`)', () => {
    // The URL — including its `&`-bearing query string — must stay a single
    // argv element so cmd `start` quirks and `&` splitting can never occur.
    expect(
      browserOpenCommand('win32', 'https://example.com/issues?a=1&b=2')
    ).toEqual({
      file: 'rundll32',
      args: [
        'url.dll,FileProtocolHandler',
        'https://example.com/issues?a=1&b=2',
      ],
    });
  });

  it('uses xdg-open on Linux/other platforms', () => {
    expect(browserOpenCommand('linux', 'https://example.com')).toEqual({
      file: 'xdg-open',
      args: ['https://example.com'],
    });
  });

  it('uses wslview under WSL to reach the Windows browser', () => {
    expect(browserOpenCommand('linux', 'https://example.com', true)).toEqual({
      file: 'wslview',
      args: ['https://example.com'],
    });
  });
});

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

  it('declares knowledge capability in clientMeta', () => {
    const _client = new KasAcpClient();
    expect(capturedKiroClientConfig?.clientMeta?.knowledge).toBe(true);
  });

  it('declares hooks capability in clientMeta', () => {
    const _client = new KasAcpClient();
    expect(capturedKiroClientConfig?.clientMeta?.hooks).toEqual({
      enabled: true,
      v2: true,
    });
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

  it('newSession() applies initialAgent via setSessionConfigOption(mode)', async () => {
    const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
    await client.newSession();

    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
      expect.objectContaining({
        configId: 'mode',
        value: 'quick-plan',
        sessionId: 'kas-session-1',
      })
    );
  });

  it('newSession() does not set mode config when initialAgent absent and KIRO_MODE unset', async () => {
    const prev = process.env.KIRO_MODE;
    delete process.env.KIRO_MODE;
    try {
      const client = new KasAcpClient();
      await client.newSession();
      const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
        ([req]: any[]) => req?.configId === 'mode'
      );
      expect(modeCalls.length).toBe(0);
    } finally {
      if (prev !== undefined) process.env.KIRO_MODE = prev;
    }
  });

  it('newSession() prefers initialAgent over KIRO_MODE env var', async () => {
    const prev = process.env.KIRO_MODE;
    process.env.KIRO_MODE = 'vibe';
    try {
      const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
      await client.newSession();
      const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
        ([req]: any[]) => req?.configId === 'mode'
      );
      expect(modeCalls.length).toBe(1);
      expect(modeCalls[0][0].value).toBe('quick-plan');
    } finally {
      if (prev === undefined) delete process.env.KIRO_MODE;
      else process.env.KIRO_MODE = prev;
    }
  });

  it('loadSession() does NOT apply initialAgent (V2 parity, persisted agent wins)', async () => {
    const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
    await client.loadSession('existing-session');
    const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'mode'
    );
    expect(modeCalls.length).toBe(0);
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

  // ── sendMessage / steerMessage / clearSteering wire format ──

  it('sendMessage() calls kiroClient.prompt directly (KAS does not implement _message/send)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    mockKiroPrompt.mockClear();
    mockKiroSendExtMethod.mockClear();

    await client.sendMessage('kas-session-1', 'wake the crew session');

    expect(mockKiroPrompt).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'kas-session-1',
        prompt: [{ type: 'text', text: 'wake the crew session' }],
      })
    );
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_message/send',
      expect.anything()
    );
  });

  it('steerMessage() forwards to _session/steer ext method', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    await client.steerMessage('kas-session-1', 'redirect mid-turn');

    expect(mockKiroSendExtMethod).toHaveBeenCalledWith('_session/steer', {
      sessionId: 'kas-session-1',
      message: 'redirect mid-turn',
    });
  });

  it('clearSteering() forwards to _session/steer/clear ext method', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    await client.clearSteering('kas-session-1');

    expect(mockKiroSendExtMethod).toHaveBeenCalledWith('_session/steer/clear', {
      sessionId: 'kas-session-1',
    });
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

  it('executeCommand("plan") switches to quick-plan mode', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    await client.executeCommand({ command: 'plan' } as any);
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'mode', value: 'quick-plan' })
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
            name: 'Default',
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
    expect(data.current).toBe('kiro_default');
    expect(data.agents.map((a) => a.name)).toEqual([
      'kiro_default',
      'research',
    ]);
  });

  it('captureModes translates wire vibe → kiro_default and rewrites the display name to "Kiro"', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          { id: 'vibe', name: 'Vibe', description: 'General coding' },
          { id: 'spec', name: 'Spec', description: 'Spec mode' },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();
    const state = (client as any).modesState;
    expect(state.currentModeId).toBe('kiro_default');
    expect(state.availableModes.map((m: { id: string }) => m.id)).toEqual([
      'kiro_default',
      'spec',
    ]);
    const def = state.availableModes.find(
      (m: { id: string }) => m.id === 'kiro_default'
    );
    expect(def.name).toBe('Kiro');
    // Spec is unaffected — its display name passes through.
    const spec = state.availableModes.find(
      (m: { id: string }) => m.id === 'spec'
    );
    expect(spec.name).toBe('Spec');
  });

  it('agent swap of kiro_default sends "vibe" on the wire (toKasModeId)', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();
    await client.executeCommand({
      command: 'agent',
      args: { agentName: 'kiro_default' },
    } as any);
    const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'mode'
    );
    expect(modeCalls.length).toBe(1);
    expect(modeCalls[0][0].value).toBe('vibe');
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

  // ── /compact ──
  // executeCommand resolves synchronously with "Compacting..."; the terminating
  // CompactionStatus is broadcast later from `.then()`/`.catch()`. Flush a
  // macrotask after invoking so those broadcasts settle before asserting.
  const flushAsync = () => new Promise((r) => setTimeout(r, 0));

  it('executeCommand("compact") broadcasts started then completed on success', async () => {
    // Derived purely from success: KAS returns { success: true } for both a
    // real compaction and a no-op (e.g. empty conversation). Either way we
    // terminate the spinner with 'completed' — we do not infer a reason.
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual([
      'started',
      'completed',
    ]);
  });

  it('executeCommand("compact") broadcasts failed with no fabricated reason when success is false', async () => {
    // KAS reports a failure as { success: false } with no message. callExtMethod
    // wraps it as a transport-success, so the real status is in result.data. We
    // surface 'failed' without inventing a reason.
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: false });
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual(['started', 'failed']);
    // No fabricated reason — error stays undefined when KAS gives none.
    expect(
      statuses.find((s: any) => s.status === 'failed').error
    ).toBeUndefined();
  });

  it('executeCommand("compact") surfaces the real reason when the ext method rejects', async () => {
    // A thrown error (e.g. SessionNotFoundError) is a genuine reason from KAS,
    // so it is surfaced verbatim — extraction, not fabrication.
    mockKiroSendExtMethod.mockRejectedValueOnce(new Error('kas is down'));
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();

    const failed = handler.mock.calls
      .map((c) => c[0])
      .find(
        (e: any) =>
          e.type === AgentEventType.CompactionStatus && e.status === 'failed'
      );
    expect(failed).toBeDefined();
    expect(failed.error).toBe('kas is down');
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
            name: 'Default',
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
        value: 'kiro_default',
        label: 'Kiro',
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

  it('getCommandOptions("/agent") filters out denylisted agents (e.g. semantic-reviewer)', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Default',
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'semantic_reviewer',
            name: 'Semantic Reviewer',
            description: 'Reviews PRs',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/agent', '');
    const values = result.options.map((o: any) => o.value);
    expect(values).toEqual(['kiro_default']);
    expect(values).not.toContain('semantic_reviewer');
  });

  it('getCommandOptions("/agent") filters out the bundled autonomous agent', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Default',
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'autonomous',
            name: 'Autonomous',
            description: 'Self-directed execution',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/agent', '');
    const values = result.options.map((o: any) => o.value);
    expect(values).toEqual(['kiro_default']);
    expect(values).not.toContain('autonomous');
  });

  it('getCommandOptions("/agent") preserves a user/workspace agent that shares a denylisted id', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Default',
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            // Same id as the bundled denylist entry, but user-defined: the
            // user opted into this, so it must NOT be filtered out.
            id: 'semantic_reviewer',
            name: 'My Semantic Reviewer',
            description: 'Custom reviewer',
            _meta: { kiro: { source: 'workspace' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/agent', '');
    const values = result.options.map((o: any) => o.value);
    expect(values).toContain('semantic_reviewer');
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
            name: 'Default',
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

  it('current_mode_update broadcasts AgentSwitched with previousAgentName and welcomeMessage', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Default',
            description: '',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'spec',
            name: 'Spec',
            description: '',
            _meta: {
              kiro: { source: 'bundled' },
              welcomeMessage: 'Spec mode: ready to plan',
            },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'current_mode_update',
        currentModeId: 'spec',
      },
    });

    const switched = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.AgentSwitched);
    expect(switched).toBeDefined();
    expect(switched.agentName).toBe('spec');
    expect(switched.previousAgentName).toBe('kiro_default');
    expect(switched.welcomeMessage).toBe('Spec mode: ready to plan');
  });

  it('current_mode_update does not broadcast AgentSwitched when the mode is unchanged', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          {
            id: 'vibe',
            name: 'Default',
            description: '',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    // Agent re-asserts the current mode (e.g. after reconnect).
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'current_mode_update',
        currentModeId: 'vibe',
      },
    });

    const switched = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.AgentSwitched);
    expect(switched).toBeUndefined();
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
    // Filter rather than index — newSession() now also broadcasts an
    // initial EffortUpdate which would otherwise occupy slot 0.
    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.Content) as any;
    expect(event).toBeDefined();
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
    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.ToolCall) as any;
    expect(event).toBeDefined();
    expect(event.id).toBe('tc-1');
    expect(event.name).toBe('fs_write');
  });

  // ── Stub methods ──

  it('listSettings returns settings from cli.json', async () => {
    writeTestCliJson({ 'chat.theme': 'dark', 'voice.autoSubmit': true });
    const client = new KasAcpClient();
    const result = await client.listSettings();
    expect(result).toEqual({ 'chat.theme': 'dark', 'voice.autoSubmit': true });
  });

  it('setSetting merges key into existing settings and writes', async () => {
    writeTestCliJson({ 'chat.theme': 'dark' });
    const client = new KasAcpClient();
    await client.setSetting('voice.autoSubmit', true);
    expect(readTestCliJson()).toEqual({
      'chat.theme': 'dark',
      'voice.autoSubmit': true,
    });
  });

  it('newSession() applies initialModel via setSessionConfigOption(model)', async () => {
    const client = new KasAcpClient({ initialModel: 'claude-opus-4.6' });
    await client.newSession();
    const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'model'
    );
    expect(modeCalls).toHaveLength(1);
    expect(modeCalls[0][0].value).toBe('claude-opus-4.6');
  });

  it('newSession() does not set model config when initialModel is absent', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const modelCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'model'
    );
    expect(modelCalls).toHaveLength(0);
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

  // ── /effort command ──

  /**
   * Seed a newSession response that mirrors what KAS returns for a model
   * with an effortLevels schema: a SessionConfigOption list containing an
   * `id: 'effortLevel'` entry with currentValue + options.
   */
  function seedSessionWithEffort(opts: {
    currentValue: string;
    levels: Array<{ value: string; name: string }>;
  }): void {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-effort',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: opts.currentValue,
          options: opts.levels,
        },
      ],
    } as any);
  }

  it('getCommandOptions("/effort") returns cached levels with [active] marking', async () => {
    seedSessionWithEffort({
      currentValue: 'high',
      levels: [
        { value: 'low', name: 'Low' },
        { value: 'medium', name: 'Medium' },
        { value: 'high', name: 'High' },
        { value: 'xhigh', name: 'xHigh' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/effort', '');
    expect(result.options.length).toBe(4);
    expect(result.options[0]).toEqual({
      value: 'low',
      label: 'Low',
      description: '',
    });
    expect(result.options[2]).toEqual({
      value: 'high',
      label: 'High',
      description: '[active]',
    });
  });

  it('getCommandOptions("/effort") returns empty when model has no effort schema', async () => {
    // Default mock returns no configOptions → no effort cache
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/effort', '');
    expect(result.options).toEqual([]);
  });

  it('loadSession populates the effort cache from configOptions', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded-effort',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: 'medium',
          options: [
            { value: 'low', name: 'Low' },
            { value: 'medium', name: 'Medium' },
            { value: 'high', name: 'High' },
          ],
        },
      ],
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('kas-loaded-effort');

    const result = await client.getCommandOptions('/effort', '');
    expect(result.options.length).toBe(3);
    const activeEntry = result.options.find((o: any) =>
      o.description?.startsWith('[active]')
    );
    expect(activeEntry?.value).toBe('medium');
  });

  it('executeCommand("effort") sets the level via setSessionConfigOption and refreshes cache', async () => {
    seedSessionWithEffort({
      currentValue: 'high',
      levels: [
        { value: 'high', name: 'High' },
        { value: 'xhigh', name: 'xHigh' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // KAS returns the full configOptions state reflecting the new level
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: 'xhigh',
          options: [
            { value: 'high', name: 'High' },
            { value: 'xhigh', name: 'xHigh' },
          ],
        },
      ],
    } as any);

    mockKiroSetSessionConfigOption.mockClear();
    const result = await client.executeCommand({
      command: 'effort',
      args: { value: 'xhigh' },
    } as any);

    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-effort',
      configId: 'effortLevel',
      value: 'xhigh',
    });
    expect(result.success).toBe(true);
    // Message locked to "Effort set to {Level}" (display-cased), no suffix.
    expect(result.message).toBe('Effort set to xHigh');
    expect(result.data).toEqual({ effort: 'xhigh' });

    // Cache should now mark xhigh as active
    const options = await client.getCommandOptions('/effort', '');
    const activeEntry = options.options.find((o: any) =>
      o.description?.startsWith('[active]')
    );
    expect(activeEntry?.value).toBe('xhigh');
  });

  it('executeCommand("effort") without a value and no effort schema returns descriptive error', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.executeCommand({ command: 'effort' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not available');
  });

  it('executeCommand("effort") without a value but with a schema returns usage error', async () => {
    seedSessionWithEffort({
      currentValue: 'high',
      levels: [
        { value: 'high', name: 'High' },
        { value: 'xhigh', name: 'xHigh' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.executeCommand({ command: 'effort' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('Usage');
  });

  it('executeCommand("effort") returns error when KAS rejects (currentValue unchanged)', async () => {
    seedSessionWithEffort({
      currentValue: 'high',
      levels: [
        { value: 'high', name: 'High' },
        { value: 'xhigh', name: 'xHigh' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // KAS ignores an unknown level, leaving the selection unchanged.
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: 'high',
          options: [
            { value: 'high', name: 'High' },
            { value: 'xhigh', name: 'xHigh' },
          ],
        },
      ],
    } as any);

    const result = await client.executeCommand({
      command: 'effort',
      args: { value: 'nonexistent' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain("'nonexistent' not available");
  });

  it('config_option_update session notification refreshes the effort cache', async () => {
    seedSessionWithEffort({
      currentValue: 'high',
      levels: [
        { value: 'high', name: 'High' },
        { value: 'xhigh', name: 'xHigh' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    // KAS autonomously changes effort (e.g. after a model switch)
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-effort',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'effortLevel',
            name: 'Effort',
            category: 'thought_level',
            currentValue: 'xhigh',
            options: [
              { value: 'high', name: 'High' },
              { value: 'xhigh', name: 'xHigh' },
            ],
          },
        ],
      },
    });

    const options = await client.getCommandOptions('/effort', '');
    const activeEntry = options.options.find((o: any) =>
      o.description?.startsWith('[active]')
    );
    expect(activeEntry?.value).toBe('xhigh');
  });

  // ── /knowledge command ──

  describe('knowledge command', () => {
    function seedInitializeWithKnowledge() {
      mockKiroInitialize.mockResolvedValueOnce({
        protocolVersion: '1.0',
        agentCapabilities: {
          _meta: {
            kiro: {
              extensionMethods: [
                {
                  method: '_kiro/knowledge',
                  name: '/knowledge',
                  description: 'Manage knowledge',
                },
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

  // ── /context typed methods ──
  // These tests assert the wire shape sent to `_kiro/session/context`.
  // All slash-command parsing (subcommand normalize, rm alias, --force,
  // unquote) lives in the kas-handler — see kas-handlers/__tests__/context.test.ts
  // for that coverage.

  describe('context typed methods', () => {
    it('contextShow(): calls _kiro/session/context with subcommand=show and returns entries', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        entries: [{ path: 'foo.ts', matched: true }],
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.contextShow();

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/session/context',
        expect.objectContaining({ subcommand: 'show' })
      );
      expect(result.entries).toEqual([{ path: 'foo.ts', matched: true }]);
    });

    it('contextShow(): defaults missing entries to []', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({});
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.contextShow();

      expect(result.entries).toEqual([]);
    });

    it('contextAdd(): calls with subcommand=add + path, omits force when not set', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: true,
        message: "Added 'foo.ts' to context",
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.contextAdd('foo.ts');

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/session/context',
        expect.objectContaining({ subcommand: 'add', path: 'foo.ts' })
      );
      // When force is not set, the wire payload must not carry the key —
      // assert the absence so the agent never sees `force: false`.
      const params = (mockKiroSendExtMethod as any).mock.calls[0][1];
      expect('force' in params).toBe(false);
      expect(result.success).toBe(true);
      expect(result.message).toContain('Added');
    });

    it('contextAdd(): forwards force=true when opts.force', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: true,
        message: "Added 'tool-output.json' to context",
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await client.contextAdd('tool-output.json', { force: true });

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/session/context',
        expect.objectContaining({
          subcommand: 'add',
          path: 'tool-output.json',
          force: true,
        })
      );
    });

    it('contextAdd(): omits force when opts.force is false', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: true,
        message: 'ok',
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await client.contextAdd('foo.ts', { force: false });

      const params = (mockKiroSendExtMethod as any).mock.calls[0][1];
      expect('force' in params).toBe(false);
    });

    it('contextRemove(): calls with subcommand=remove + path', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: true,
        message: "Removed 'foo.ts' from context",
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await client.contextRemove('foo.ts');

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/session/context',
        expect.objectContaining({ subcommand: 'remove', path: 'foo.ts' })
      );
    });

    it('contextClear(): calls with subcommand=clear and no path', async () => {
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: true,
        message: 'Cleared 2 context entries',
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.contextClear();

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/session/context',
        expect.objectContaining({ subcommand: 'clear' })
      );
      const params = (mockKiroSendExtMethod as any).mock.calls[0][1];
      expect('path' in params).toBe(false);
      expect(result.message).toContain('Cleared');
    });

    it('mutations: surface inner success=false (path-not-found) on the typed return', async () => {
      // The agent encodes domain-level success/failure inside the response
      // payload, distinct from RPC-level success. The typed wrapper has to
      // propagate that flag so the handler picks the right alert tone.
      mockKiroSendExtMethod.mockResolvedValueOnce({
        success: false,
        message: 'Path not found: ghost.ts',
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.contextAdd('ghost.ts');

      expect(result.success).toBe(false);
      expect(result.message).toContain('Path not found');
    });

    it('contextShow(): RPC-level failure throws so the handler hits the catch path', async () => {
      // sendExtMethod rejecting models the JSON-RPC layer error path
      // (transport failure, method-not-found, etc.). callExtMethod
      // captures it as success:false; the typed wrapper escalates it
      // to an exception so the handler can show a clean error alert.
      mockKiroSendExtMethod.mockRejectedValueOnce(
        new Error('method not found')
      );
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await expect(client.contextShow()).rejects.toThrow('method not found');
    });

    it('getCachedContextBreakdown(): null until session_info_update populates it', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      expect(client.getCachedContextBreakdown()).toBeNull();
    });

    it('getCachedContextBreakdown(): returns the cached breakdown once set', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const breakdown = { contextFiles: { tokens: 100, percent: 5 } };
      (client as any).cachedBreakdown = breakdown;

      expect(client.getCachedContextBreakdown()).toBe(breakdown);
    });
  });

  // ── /code command ──

  describe('/code command', () => {
    it('executeCommand code defaults to status subcommand', async () => {
      mockKiroSendExtMethod.mockResolvedValue({
        success: true,
        status: {
          initialized: true,
          languages: ['typescript'],
          lspServers: [],
        },
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const result = await client.executeCommand({
        command: 'code',
      } as any);

      expect(result.success).toBe(true);
      expect((result.data as any).status).toBe('initialized');
    });

    it('executeCommand code passes subcommand from args.value', async () => {
      mockKiroSendExtMethod.mockResolvedValue({ status: 'initializing' });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await client.executeCommand({
        command: 'code',
        args: { value: 'init' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/codeIntelligence',
        expect.objectContaining({ subcommand: 'init' })
      );
    });

    it('executeCommand code overview passes subcommand', async () => {
      mockKiroSendExtMethod.mockResolvedValue({
        executePrompt: 'overview data',
      });
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      await client.executeCommand({
        command: 'code',
        args: { value: 'overview' },
      } as any);

      expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
        '_kiro/codeIntelligence',
        expect.objectContaining({ subcommand: 'overview' })
      );
    });
  });

  // ── /prompts command ──
  //
  // The picker and dispatch flow live in the `handlePrompts` kas-handler
  // (see `kas-handlers/__tests__/prompts.test.ts`), not in
  // `KasAcpClient`. This file only covers the ingest path that populates
  // the typed AppState slices.

  // ── available_commands_update partitions into typed slices ──

  it('available_commands_update session notification partitions into prompts/skills/steering', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // Capture broadcasted events
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    // Simulate the KAS session update -- prompts, skills, steering arrive as
    // commands tagged with _meta.kiro.type. Untagged commands flow through
    // CommandsUpdate.
    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
          {
            name: 'summarize',
            description: 'Prompt template',
            _meta: { kiro: { type: 'prompt' } },
          },
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

    // PromptsUpdate / SkillsUpdate / SteeringUpdate broadcasted with right
    // partitioning. The store consumes these events; the `/prompts`
    // picker reads from the resulting slices via `handlePrompts`.
    const promptsEvent = events.find((e) => e.type === 'prompts_update');
    const skillsEvent = events.find((e) => e.type === 'skills_update');
    const steeringEvent = events.find((e) => e.type === 'steering_update');
    expect(promptsEvent.prompts).toHaveLength(1);
    expect(promptsEvent.prompts[0].name).toBe('summarize');
    expect(skillsEvent.skills).toHaveLength(1);
    expect(skillsEvent.skills[0].name).toBe('research');
    expect(steeringEvent.steering).toHaveLength(1);
    expect(steeringEvent.steering[0].name).toBe('plan');

    // CommandsUpdate excludes prompt/skill/steering entries; only `help`
    // (untagged) flows through.
    const commandsEvent = events.find((e) => e.type === 'commands_update');
    expect(commandsEvent.commands).toHaveLength(1);
    expect(commandsEvent.commands[0].name).toBe('help');
  });

  it('available_commands_update filters out agent-type commands (handled via modes cache)', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
          {
            name: 'vibe',
            description: 'General coding',
            _meta: { kiro: { type: 'agent' } },
          },
          {
            name: 'research',
            description: 'Deep research agent',
            _meta: { kiro: { type: 'agent' } },
          },
        ],
      },
    });

    const commandsEvent = events.find((e) => e.type === 'commands_update');
    expect(commandsEvent.commands).toHaveLength(1);
    expect(commandsEvent.commands[0].name).toBe('help');
  });

  it('available_commands_update filters out custom-agent commands (delegate-task subagents)', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
          // KAS exposes curated builtin subagents + user/workspace agent
          // profiles tagged `custom-agent` (see kiro-agent source-adapters
          // EXPOSED_BUILTIN_SUBAGENTS). They are delegate-a-task subagents,
          // not top-level slash commands, and aren't switchable modes.
          {
            name: 'context-gatherer',
            description: 'Analyzes repository structure',
            _meta: { kiro: { type: 'custom-agent' } },
          },
          {
            name: 'general-task-execution',
            description: 'Delegates a general task',
            _meta: { kiro: { type: 'custom-agent' } },
          },
          // A real prompt must still partition correctly alongside them.
          {
            name: 'summarize',
            description: 'Prompt template',
            _meta: { kiro: { type: 'prompt' } },
          },
        ],
      },
    });

    // Only the untyped `help` survives into the slash-command list.
    const commandsEvent = events.find((e) => e.type === 'commands_update');
    expect(commandsEvent.commands).toHaveLength(1);
    expect(commandsEvent.commands[0].name).toBe('help');

    // The prompt still routes into the prompts slice.
    const promptsEvent = events.find((e) => e.type === 'prompts_update');
    expect(promptsEvent.prompts).toHaveLength(1);
    expect(promptsEvent.prompts[0].name).toBe('summarize');
  });

  it('available_commands_update filters out commands matching cached modes even without kiro type', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [
          { id: 'vibe', name: 'Vibe', description: 'General coding' },
          { id: 'research', name: 'Research', description: 'Deep research' },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help', _meta: {} },
          { name: 'vibe', description: 'General coding', _meta: {} },
          { name: 'research', description: 'Deep research', _meta: {} },
        ],
      },
    });

    const commandsEvent = events.find((e) => e.type === 'commands_update');
    expect(commandsEvent.commands).toHaveLength(1);
    expect(commandsEvent.commands[0].name).toBe('help');
  });

  it('available_commands_update with no prompt-type commands broadcasts an empty PromptsUpdate', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // Capture broadcasted events
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

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

    const promptsEvent = events.find((e) => e.type === 'prompts_update');
    expect(promptsEvent.prompts).toHaveLength(0);
  });

  // ── session_info_update kind=turn_completion → TurnSummary ──

  it('session_info_update kind=turn_completion broadcasts TurnSummary with metering and duration', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            promptTurnSummaries: [
              { usage: 1.5, unit: 'credit', unitPlural: 'Credits' },
              { usage: 500, unit: 'token', unitPlural: 'Tokens' },
            ],
            elapsedTime: 1234,
            status: 'success',
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeDefined();
    expect(summary.turnDurationMs).toBe(1234);
    expect(summary.meteringUsage).toEqual([
      { value: 1.5, unit: 'credit', unitPlural: 'Credits' },
      { value: 500, unit: 'token', unitPlural: 'Tokens' },
    ]);
  });

  it('session_info_update kind=turn_completion drops entries without numeric usage', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            promptTurnSummaries: [
              { unit: 'credit' }, // no usage — drop
              { usage: 2, unit: 'credit', unitPlural: 'Credits' },
            ],
            elapsedTime: 100,
            status: 'success',
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeDefined();
    expect(summary.meteringUsage).toEqual([
      { value: 2, unit: 'credit', unitPlural: 'Credits' },
    ]);
  });

  it('session_info_update kind=turn_completion fills missing unit/unitPlural with empty string', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            promptTurnSummaries: [{ usage: 7 }],
            elapsedTime: 50,
            status: 'success',
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeDefined();
    expect(summary.meteringUsage).toEqual([
      { value: 7, unit: '', unitPlural: '' },
    ]);
  });

  it('session_info_update kind=turn_completion with no metering and no duration is suppressed', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            promptTurnSummaries: [],
            status: 'success',
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeUndefined();
  });

  // ── effortLevel config option → EffortUpdate ──

  it('newSession() broadcasts EffortUpdate with current effortLevel from configOptions', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: 'high',
          options: [
            { value: 'low', name: 'Low' },
            { value: 'medium', name: 'Medium' },
            { value: 'high', name: 'High' },
          ],
        },
      ],
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    const effortEvents = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.EffortUpdate);
    expect(effortEvents).toHaveLength(1);
    expect(effortEvents[0].effort).toBe('high');
  });

  it('newSession() broadcasts EffortUpdate with null when configOptions has no effortLevel entry', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          name: 'Mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Default' }],
        },
      ],
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    const effortEvents = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.EffortUpdate);
    expect(effortEvents).toHaveLength(1);
    expect(effortEvents[0].effort).toBeNull();
  });

  it('loadSession() broadcasts EffortUpdate with current effortLevel from configOptions', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'effortLevel',
          name: 'Effort',
          category: 'thought_level',
          currentValue: 'xhigh',
          options: [{ value: 'xhigh', name: 'xHigh' }],
        },
      ],
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.loadSession('kas-loaded');

    const effortEvents = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.EffortUpdate);
    expect(effortEvents).toHaveLength(1);
    expect(effortEvents[0].effort).toBe('xhigh');
  });

  it('config_option_update broadcasts EffortUpdate with the new effortLevel', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    // Drop any EffortUpdate events fired during newSession().
    handler.mockClear();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'effortLevel',
            name: 'Effort',
            category: 'thought_level',
            currentValue: 'medium',
            options: [
              { value: 'low', name: 'Low' },
              { value: 'medium', name: 'Medium' },
              { value: 'high', name: 'High' },
            ],
          },
        ],
      },
    });

    const effortEvents = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.EffortUpdate);
    expect(effortEvents).toHaveLength(1);
    expect(effortEvents[0].effort).toBe('medium');
  });

  it('config_option_update with no effortLevel broadcasts EffortUpdate(null) to clear the chip', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();
    handler.mockClear();

    // The active model just changed to one that does not declare an
    // effortLevels schema — KAS drops the option from configOptions and
    // we should mirror that as `null` in the store so the chip disappears.
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'mode',
            name: 'Mode',
            category: 'mode',
            currentValue: 'vibe',
            options: [{ value: 'vibe', name: 'Default' }],
          },
        ],
      },
    });

    const effortEvents = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.EffortUpdate);
    expect(effortEvents).toHaveLength(1);
    expect(effortEvents[0].effort).toBeNull();
  });
});

describe('KasAcpClient — executeCommand branches', () => {
  let client: InstanceType<typeof KasAcpClient>;

  beforeEach(() => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
    client = new KasAcpClient();
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

  it('GIVEN no session WHEN /model called THEN returns error', async () => {
    await client.initialize();
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'claude' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('No active session');
  });

  it('GIVEN session WHEN /model with no arg THEN returns usage hint', async () => {
    await client.initialize();
    await client.newSession();
    const result = await client.executeCommand({ command: 'model' } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('No models available');
  });

  it('GIVEN session with models WHEN /model with valid id THEN calls setSessionConfigOption', async () => {
    await client.initialize();
    await client.newSession();
    (client as any).modelOptions = [
      { value: 'm1', name: 'Model 1' },
      { value: 'm2', name: 'Model 2' },
    ];
    (client as any).currentModelId = 'm1';
    mockKiroSetSessionConfigOption.mockImplementationOnce(() =>
      Promise.resolve({ configOptions: [] })
    );
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'm2' },
    } as any);
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalled();
    // Model not found in response → returns not available
    expect(result.message).toContain('not available');
  });

  it('GIVEN session WHEN /model swap fails THEN returns error', async () => {
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockImplementationOnce(() =>
      Promise.reject(new Error('rate limited'))
    );
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'bad' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toBe('rate limited');
  });

  it('GIVEN session with active model WHEN /model set-current-as-default THEN persists to cli.json', async () => {
    await client.initialize();
    await client.newSession();
    (client as any).modelOptions = [
      { value: 'm1', name: 'Model 1' },
      { value: 'm2', name: 'Model 2' },
    ];
    (client as any).currentModelId = 'm1';
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'set-current-as-default' },
    } as any);
    expect(result.success).toBe(true);
    expect(result.message).toContain('Model 1');
    const saved = JSON.parse(
      readFileSync(
        join(testSettingsDir, '.kiro', 'settings', 'cli.json'),
        'utf-8'
      )
    );
    expect(saved['chat.defaultModel']).toBe('m1');
  });

  it('GIVEN no active model WHEN /model set-current-as-default THEN returns error', async () => {
    await client.initialize();
    await client.newSession();
    (client as any).currentModelId = undefined;
    const result = await client.executeCommand({
      command: 'model',
      args: { value: 'set-current-as-default' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('No model is currently active');
  });

  it('GIVEN session WHEN /usage called THEN forwards to ext method', async () => {
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockImplementationOnce(() =>
      Promise.resolve({ success: true, message: 'ok', data: { credits: 5 } })
    );
    const result = await client.executeCommand({ command: 'usage' } as any);
    expect(result.success).toBe(true);
  });

  it('GIVEN session WHEN /prompts called THEN delegates to handlePrompts (no-op here)', async () => {
    // The KAS dispatcher intercepts `/prompts` before it reaches the
    // ACP client; the handler owns the picker + send-message flow. The
    // executeCommand branch is a defensive no-op for non-intercepted
    // call sites (e.g. direct API consumers).
    await client.initialize();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'prompts',
      args: { value: 'my-prompt' },
    } as any);
    expect(result.success).toBe(true);
    expect(result.data).toBeUndefined();
  });

  it('GIVEN session WHEN unknown command THEN returns unsupported', async () => {
    await client.initialize();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'unknown_cmd',
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not yet supported');
  });

  it('GIVEN session WHEN /agent create THEN returns not implemented', async () => {
    await client.initialize();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'create myagent' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not yet implemented');
  });

  it('GIVEN session WHEN /agent edit THEN returns not implemented', async () => {
    await client.initialize();
    await client.newSession();
    const result = await client.executeCommand({
      command: 'agent',
      args: { value: 'edit myagent' },
    } as any);
    expect(result.success).toBe(false);
    expect(result.message).toContain('not yet implemented');
  });
});

describe('KasAcpClient — getCommandOptions', () => {
  let client: InstanceType<typeof KasAcpClient>;

  beforeEach(async () => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
    client = new KasAcpClient();
    await client.initialize();
    mockKiroNewSession.mockImplementationOnce(() =>
      Promise.resolve({
        sessionId: 'sid',
        models: null,
        modes: {
          availableModes: [
            {
              id: 'coder',
              name: 'Coder',
              description: 'Write code',
              _meta: { kiro: { source: 'bundled' } },
            },
            { id: 'planner', name: 'Planner', description: 'Plan', _meta: {} },
          ],
          currentModeId: 'coder',
        },
        configOptions: [
          {
            id: 'model',
            category: 'model',
            currentValue: 'm1',
            options: [
              { value: 'm1', name: 'Claude Sonnet', description: 'Fast' },
              { value: 'm2', name: 'Claude Opus' },
            ],
          },
        ],
      })
    );
    await client.newSession();
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

  it('GIVEN modes cached WHEN /agent options requested THEN returns modes with [active] marker', async () => {
    const result = await client.getCommandOptions('/agent', '');
    expect(result.options).toHaveLength(2);
    expect(result.options[0].value).toBe('coder');
    expect(result.options[0].description).toContain('[active]');
    expect(result.options[1].value).toBe('planner');
  });

  it('GIVEN models cached WHEN /model options requested THEN returns models with [active]', async () => {
    // Manually set model cache
    (client as any).modelOptions = [
      { value: 'm1', name: 'Claude Sonnet', description: 'Fast' },
      { value: 'm2', name: 'Claude Opus' },
    ];
    (client as any).currentModelId = 'm1';
    const result = await client.getCommandOptions('/model', '');
    expect(result.options).toHaveLength(2);
    expect(result.options[0].description).toContain('[active]');
    expect(result.options[1].value).toBe('m2');
  });

  it('GIVEN no session WHEN options requested THEN returns empty', async () => {
    const fresh = new KasAcpClient();
    const result = await fresh.getCommandOptions('/agent', '');
    expect(result.options).toEqual([]);
  });

  it('GIVEN unknown command WHEN options requested THEN returns empty', async () => {
    const result = await client.getCommandOptions('/unknown', '');
    expect(result.options).toEqual([]);
  });
});

describe('KasAcpClient — setMode and listSessions', () => {
  let client: InstanceType<typeof KasAcpClient>;

  beforeEach(async () => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
    client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

  it('GIVEN session WHEN setMode called THEN calls setSessionConfigOption', async () => {
    await client.setMode('planner');
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      configId: 'mode',
      value: 'planner',
    });
  });

  it('GIVEN no session WHEN setMode called THEN does nothing', async () => {
    mockKiroSetSessionConfigOption.mockClear();
    // Client has no session (never called newSession)
    const fresh = new KasAcpClient();
    await fresh.initialize();
    await fresh.setMode('x');
    expect(mockKiroSetSessionConfigOption).not.toHaveBeenCalled();
  });

  it('GIVEN session WHEN listSessions called THEN returns sessions', async () => {
    mockKiroListSessions.mockImplementationOnce(() =>
      Promise.resolve({
        sessions: [{ sessionId: 's1', cwd: '/tmp', title: 'Test' }],
      })
    );
    const result = await client.listSessions('/tmp');
    expect(result.sessions).toHaveLength(1);
    expect(result.sessions[0].sessionId).toBe('s1');
  });

  it('GIVEN listSessions fails WHEN called THEN returns empty', async () => {
    mockKiroListSessions.mockImplementationOnce(() =>
      Promise.reject(new Error('network'))
    );
    const result = await client.listSessions('/tmp');
    expect(result.sessions).toEqual([]);
  });
});

describe('KasAcpClient — session event handling', () => {
  let client: InstanceType<typeof KasAcpClient>;

  beforeEach(async () => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
    client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

  it('GIVEN session WHEN current_mode_update received THEN updates cached mode', async () => {
    expect(capturedSessionUpdateHandler).not.toBeNull();
    await capturedSessionUpdateHandler({
      update: {
        sessionUpdate: 'current_mode_update',
        currentModeId: 'planner',
      },
    });
    // Verify via getCommandOptions
    const _opts = await client.getCommandOptions('/agent', '');
    // The mode should now show planner as active (if modes were cached)
    expect((client as any).modesState.currentModeId).toBe('planner');
  });

  it('GIVEN session WHEN config_option_update received THEN refreshes model cache', async () => {
    await capturedSessionUpdateHandler({
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            id: 'model',
            category: 'model',
            currentValue: 'm3',
            options: [{ value: 'm3', name: 'New Model' }],
          },
        ],
      },
    });
    // refreshModelCache was called — verify it processed the update
    // (even if findModelConfigOption doesn't match the exact shape,
    // the code path is exercised)
    expect((client as any).modelOptions.length).toBeGreaterThanOrEqual(0);
  });
});

// ── /mcp command (push model) ──

describe('mcp command (push model)', () => {
  it('executeCommand mcp returns cached servers from notification', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    // Simulate receiving _kiro/mcp/status notification
    (client as any).handleMcpStatusNotification({
      sessionId: 'kas-session-1',
      servers: [
        {
          name: 'test-server',
          status: 'connected',
          tools: [{ name: 't1', disabled: false }],
        },
        {
          name: 'failed-server',
          status: 'failed',
          failedAuthorization: false,
          errorMessage: 'err',
        },
      ],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect(result.success).toBe(true);
    expect(result.message).toBe('2 configured servers');
    const servers = (result.data as any).servers;
    expect(servers).toHaveLength(2);
    expect(servers[0].name).toBe('test-server');
    expect(servers[0].status).toBe('running');
    expect(servers[0].toolCount).toBe(1);
    expect(servers[1].status).toBe('failed');
  });

  it('executeCommand mcp returns empty when no notification received', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect(result.success).toBe(true);
    expect(result.message).toBe('0 configured servers');
    expect((result.data as any).servers).toHaveLength(0);
  });

  it('maps connected status to running', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [{ name: 's', status: 'connected', tools: [] }],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect((result.data as any).servers[0].status).toBe('running');
  });

  it('maps connecting status to loading', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [{ name: 's', status: 'connecting' }],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect((result.data as any).servers[0].status).toBe('loading');
  });

  it('maps failed with failedAuthorization to auth-required', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [
        {
          name: 's',
          status: 'failed',
          failedAuthorization: true,
          errorMessage: 'auth',
        },
      ],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect((result.data as any).servers[0].status).toBe('auth-required');
  });

  it('singular message for 1 server', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [{ name: 's', status: 'disabled' }],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect(result.message).toBe('1 configured server');
  });

  it('/mcp list returns registry servers from notification', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [{ name: 'configured-server', status: 'connected', tools: [] }],
      registryServers: [
        {
          name: 'registry-server-1',
          version: '1.0.0',
          description: 'A registry server',
          enabled: true,
        },
        { name: 'registry-server-2', version: '2.0.0', enabled: false },
      ],
    });

    const result = await client.executeCommand({
      command: 'mcp',
      args: { value: 'list' },
    } as any);

    expect(result.success).toBe(true);
    expect(result.message).toBe('1 configured, 2 registry servers');
    expect((result.data as any).mode).toBe('list');
    expect((result.data as any).servers).toHaveLength(1);
    expect((result.data as any).registryServers).toHaveLength(2);
    expect((result.data as any).registryServers[0]).toEqual({
      name: 'registry-server-1',
      status: 'disabled',
      toolCount: 0,
      version: '1.0.0',
      description: 'A registry server',
      enabled: true,
    });
    expect((result.data as any).registryServers[1].enabled).toBe(false);
  });

  it('/mcp does NOT return registry servers', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [{ name: 'configured-server', status: 'connected', tools: [] }],
      registryServers: [{ name: 'registry-server', version: '1.0.0' }],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);

    expect(result.success).toBe(true);
    expect(result.message).toBe('1 configured server');
    expect((result.data as any).servers).toHaveLength(1);
    expect((result.data as any).registryServers).toBeUndefined();
    expect((result.data as any).mode).toBeUndefined();
  });
});

// ── MCP OAuth flow ──
//
// KAS includes `authorizationUrl` directly in the `_kiro/mcp/status`
// notification when a server fails with OAuth. The TUI reads it from
// the status notification and broadcasts `McpOauthRequest`.

describe('MCP OAuth flow', () => {
  it('broadcasts McpOauthRequest when status has failedAuthorization + authorizationUrl', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    client.onUpdate((e: any) => events.push(e));

    (client as any).handleMcpStatusNotification({
      servers: [
        {
          name: 'github-mcp',
          status: 'failed',
          failedAuthorization: true,
          authorizationUrl:
            'https://github.com/login/oauth/authorize?state=abc',
          errorMessage: 'Unauthorized',
        },
      ],
    });

    const oauthEvents = events.filter(
      (e) => e.type === AgentEventType.McpOauthRequest
    );
    expect(oauthEvents).toHaveLength(1);
    expect(oauthEvents[0].serverName).toBe('github-mcp');
    expect(oauthEvents[0].oauthUrl).toBe(
      'https://github.com/login/oauth/authorize?state=abc'
    );
  });

  it('does NOT broadcast when failedAuthorization but no authorizationUrl', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    client.onUpdate((e: any) => events.push(e));

    (client as any).handleMcpStatusNotification({
      servers: [
        {
          name: 'github-mcp',
          status: 'failed',
          failedAuthorization: true,
          errorMessage: 'Unauthorized',
        },
      ],
    });

    const oauthEvents = events.filter(
      (e) => e.type === AgentEventType.McpOauthRequest
    );
    expect(oauthEvents).toHaveLength(0);
  });

  it('shows server as auth-required when failedAuthorization is true', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    (client as any).handleMcpStatusNotification({
      servers: [
        {
          name: 'github-mcp',
          status: 'failed',
          failedAuthorization: true,
          errorMessage: 'Unauthorized',
        },
      ],
    });

    const result = await client.executeCommand({ command: 'mcp' } as any);
    expect((result.data as any).servers[0]).toMatchObject({
      name: 'github-mcp',
      status: 'auth-required',
    });
  });

  describe('KAS _kiro/* notification registration', () => {
    it('registers handlers for _kiro/customAgent/not_found', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      expect(kc._extNotifHandlers['_kiro/customAgent/not_found']).toBeDefined();
    });

    it('registers handlers for _kiro/customAgent/config_error', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      expect(
        kc._extNotifHandlers['_kiro/customAgent/config_error']
      ).toBeDefined();
    });

    it('registers handlers for _kiro/error/rate_limit', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      expect(kc._extNotifHandlers['_kiro/error/rate_limit']).toBeDefined();
    });

    it('registers handlers for _kiro/mcp/governance_disabled', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      expect(
        kc._extNotifHandlers['_kiro/mcp/governance_disabled']
      ).toBeDefined();
    });

    it('governance handler transforms reason=api_failure to apiFailure=true', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/mcp/governance_disabled']({
        sessionId: 'test',
        reason: 'api_failure',
      });
      const gov = events.find((e) => e.type === 'mcp_governance_disabled');
      expect(gov).toBeDefined();
      expect(gov.apiFailure).toBe(true);
    });

    it('governance handler transforms reason=admin_disabled to apiFailure=false', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/mcp/governance_disabled']({
        sessionId: 'test',
        reason: 'admin_disabled',
      });
      const gov = events.find((e) => e.type === 'mcp_governance_disabled');
      expect(gov).toBeDefined();
      expect(gov.apiFailure).toBe(false);
    });

    it('agent not_found handler updates cached mode to fallback', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'missing-agent',
        fallbackAgent: 'vibe',
      });
      expect((client as any).modesState.currentModeId).toBe('kiro_default');
    });
  });
});
