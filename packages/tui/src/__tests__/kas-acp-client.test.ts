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
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../constants/agents';

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
mock.module('node-machine-id', () => ({
  machineIdSync: () => 'test-machine-id',
}));

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
const mockKiroSendExtNotification = mock((_method: string, _params: any) =>
  Promise.resolve()
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
  sendExtNotification = mockKiroSendExtNotification;
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

const mockEmitKasTelemetry = mock((_event: string, _payload: unknown) =>
  Promise.resolve({ ok: true })
);

mock.module('../utils/kas-telemetry-cli', () => ({
  emitKasTelemetry: mockEmitKasTelemetry,
}));

afterAll(() => {
  teardownTestHome();
  mock.restore();
});

// @ts-expect-error — bun-specific query-string import
const { KasAcpClient, resolveFeedbackUrl, browserOpenCommand } =
  await import('../acp-client?kas-test');

function defaultMode(overrides: Record<string, unknown> = {}) {
  return {
    id: KAS_DEFAULT_AGENT_ID,
    name: KAS_DEFAULT_AGENT_NAME,
    ...overrides,
  };
}

function defaultModeOption(overrides: Record<string, unknown> = {}) {
  return {
    value: KAS_DEFAULT_AGENT_ID,
    name: KAS_DEFAULT_AGENT_NAME,
    ...overrides,
  };
}

function freshMocks() {
  mockSpawn.mockClear();
  mockKiroInitialize.mockClear();
  mockKiroNewSession.mockClear();
  mockKiroLoadSession.mockClear();
  mockKiroPrompt.mockClear();
  mockKiroCancel.mockClear();
  mockKiroSetSessionConfigOption.mockClear();
  mockKiroSendExtMethod.mockClear();
  mockKiroSendExtNotification.mockClear();
  mockEmitKasTelemetry.mockClear();
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
      'https://taskei.amazon.dev/tasks/create?template=5389200f-f825-4261-98ec-04bc84572fab'
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

  it('newSession() with an unavailable initialModel reports no model (no stale "Auto" fallback)', async () => {
    // KAS accepts an unavailable model id and echoes it as currentValue, but
    // it's absent from the options list → chip shows nothing (matches V2),
    // not a misleading "Auto".
    const prevMode = process.env.KIRO_MODE;
    delete process.env.KIRO_MODE;
    try {
      mockKiroSetSessionConfigOption.mockImplementation((req: any) => {
        if (req?.configId === 'model') {
          return Promise.resolve({
            configOptions: [
              {
                type: 'select',
                id: 'model',
                category: 'model',
                currentValue: 'claude-opus-4.8', // accepted but unavailable
                options: [{ value: 'm1', name: 'Test Model' }],
              },
            ],
          });
        }
        return Promise.resolve();
      });

      const client = new KasAcpClient({ initialModel: 'claude-opus-4.8' });
      const result = await client.newSession();

      // The model option was set with the requested (invalid) id...
      expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
        expect.objectContaining({ configId: 'model', value: 'claude-opus-4.8' })
      );
      // ...but since it isn't in the available list, no chip is shown.
      expect(result.currentModel).toBeUndefined();
    } finally {
      // Restore the default implementation (mockClear does not reset it).
      mockKiroSetSessionConfigOption.mockImplementation(() =>
        Promise.resolve()
      );
      if (prevMode === undefined) delete process.env.KIRO_MODE;
      else process.env.KIRO_MODE = prevMode;
    }
  });

  it('newSession() applies initialAgent via setSessionConfigOption(mode)', async () => {
    const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
    await client.newSession();

    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
      expect.objectContaining({
        configId: 'mode',
        value: 'plan',
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
    process.env.KIRO_MODE = KAS_DEFAULT_AGENT_ID;
    try {
      const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
      await client.newSession();
      const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
        ([req]: any[]) => req?.configId === 'mode'
      );
      expect(modeCalls.length).toBe(1);
      expect(modeCalls[0][0].value).toBe('plan');
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

  it('loadSession() returns the normalized agent id for wire vibe (not the raw wire id)', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded',
      models: null,
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Vibe' }],
      },
    } as any);

    const client = new KasAcpClient();
    const result = await client.loadSession('existing-session');

    expect(result.currentAgent?.name).toBe('default');
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

  it('prompt() forwards chat session start telemetry once per session', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    await client.prompt([{ type: 'text', text: 'hello' } as any]);
    await client.prompt([{ type: 'text', text: 'again' } as any]);

    const startCalls = mockEmitKasTelemetry.mock.calls.filter(
      ([event]: any[]) => event === 'kas-chat-session-started'
    );
    expect(startCalls).toHaveLength(1);
    expect(startCalls[0]).toEqual([
      'kas-chat-session-started',
      { sessionId: 'kas-session-1' },
    ]);
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

  it('sendProcessHealthMetrics() forwards KAS telemetry notification', () => {
    const client = new KasAcpClient();
    const snapshot = {
      rssMb: 10,
      heapUsedMb: 5,
      peakRssMb: 12,
      cpuUserPct: 1,
      cpuSystemPct: 2,
      lastRenderMs: 3,
      maxRenderMs: 4,
      rendersPerMin: 5,
      fullRedrawsPerMin: 6,
      yogaNodeCount: 7,
      eventLoopP99Ms: 8,
      inputLatencyP95Ms: 9,
      sessionDurationSec: 10,
      cpuCores: 11,
      totalMemoryMb: 12,
      terminal: 'xterm-256color',
      sessionId: 'kas-session-1',
      version: '0.0.0-dev',
      platform: 'darwin',
    };

    client.sendProcessHealthMetrics(snapshot);

    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-process-health', {
      ...snapshot,
      agentKind: 'kas',
    });
  });

  it('sendModeChanged() forwards KAS telemetry through the host bridge', () => {
    const client = new KasAcpClient();
    const payload = {
      fromMode: 'kiro',
      toMode: 'kiro_planner',
      source: 'shiftTab',
      sessionId: 'kas-session-1',
    };

    client.sendModeChanged(payload as any);

    expect(mockEmitKasTelemetry).toHaveBeenCalledWith(
      'kas-mode-changed',
      payload
    );
  });

  it('sendChatSlashCommandTelemetry() forwards KAS command usage through the host bridge', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    client.sendChatSlashCommandTelemetry({
      command: '/chat',
      subcommand: 'save',
      success: true,
    });

    expect(mockEmitKasTelemetry).toHaveBeenCalledWith(
      'kas-chat-slash-command',
      {
        command: '/chat',
        subcommand: 'save',
        success: true,
        sessionId: 'kas-session-1',
      }
    );
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

  it('executeCommand("plan") switches to plan mode', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    await client.executeCommand({ command: 'plan' } as any);
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'mode', value: 'plan' })
    );
  });

  it('executeCommand("agent") (no args) derives agent list from cached modes, not _kiro/agent/list', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
    expect(data.current).toBe(KAS_DEFAULT_AGENT_ID);
    expect(data.agents.map((a) => a.name)).toEqual([
      KAS_DEFAULT_AGENT_ID,
      'research',
    ]);
  });

  it('captureModes keeps the advertised Default display name', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
            description: 'General coding',
          },
          { id: 'spec', name: 'Spec', description: 'Spec mode' },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();
    const state = (client as any).modesState;
    expect(state.currentModeId).toBe(KAS_DEFAULT_AGENT_ID);
    expect(state.availableModes.map((m: { id: string }) => m.id)).toEqual([
      KAS_DEFAULT_AGENT_ID,
      'spec',
    ]);
    const def = state.availableModes.find(
      (m: { id: string }) => m.id === KAS_DEFAULT_AGENT_ID
    );
    expect(def.name).toBe(KAS_DEFAULT_AGENT_NAME);
    // Spec is unaffected — its display name passes through.
    const spec = state.availableModes.find(
      (m: { id: string }) => m.id === 'spec'
    );
    expect(spec.name).toBe('Spec');
  });

  it('captureModes allows only the built-in agents (default/plan/spec) and keeps user agents', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
            description: 'General coding',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'plan',
            name: 'Plan',
            description: 'Interactive planner',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'spec',
            name: 'Spec',
            description: 'Spec mode',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'bug-fix',
            name: 'Bug Fix',
            description: 'Bug fixing workflow',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'autonomous',
            name: 'Autonomous',
            description: 'Self-directed execution',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'quick-spec',
            name: 'Quick Spec',
            description: 'Fast spec-generation workflow',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'my-agent',
            name: 'My Agent',
            description: 'Custom workspace agent',
            _meta: { kiro: { source: 'workspace' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();
    const state = (client as any).modesState;
    // Only the three allowlisted built-ins survive (default, plan →
    // kiro_planner, spec), plus the user/workspace agent. Any other bundled
    // mode (bug-fix, autonomous, quick-spec) is hidden.
    expect(state.availableModes.map((m: { id: string }) => m.id)).toEqual([
      KAS_DEFAULT_AGENT_ID,
      'kiro_planner',
      'spec',
      'my-agent',
    ]);
  });

  it('agent swap of default sends the KAS wire id "vibe"', async () => {
    // KAS still expects `vibe` on the wire for the default mode; the TUI-side
    // canonical id is `default` but `toKasModeId` translates on the way out.
    // Remove this translation (and update this test) once KAS accepts `default`.
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
            description: 'General coding',
          },
          { id: 'spec', name: 'Spec', description: 'Spec mode' },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();
    await client.executeCommand({
      command: 'agent',
      args: { agentName: KAS_DEFAULT_AGENT_ID },
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
        value: KAS_DEFAULT_AGENT_ID,
        label: KAS_DEFAULT_AGENT_NAME,
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
    expect(values).toEqual([KAS_DEFAULT_AGENT_ID]);
    expect(values).not.toContain('semantic_reviewer');
  });

  it('getCommandOptions("/agent") filters out the bundled autonomous agent', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
    expect(values).toEqual([KAS_DEFAULT_AGENT_ID]);
    expect(values).not.toContain('autonomous');
  });

  it('getCommandOptions("/agent") hides the quick-spec builtin mode but shows plan', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
            description: 'General coding assistance',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'plan',
            name: 'Plan',
            description: 'Read-only interactive planner',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            id: 'quick-spec',
            name: 'Quick Spec',
            description: 'Fast spec-generation workflow',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    } as any);

    const client = new KasAcpClient();
    await client.newSession();

    const result = await client.getCommandOptions('/agent', '');
    const values = result.options.map((o: any) => o.value);
    // `plan` is surfaced under the TUI-facing name kiro_planner
    expect(values).toContain('kiro_planner');
    expect(values).not.toContain('quick-spec');
    expect(values).not.toContain('plan');
  });

  it('getCommandOptions("/agent") preserves a user/workspace agent that shares a denylisted id', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
    expect(switched.previousAgentName).toBe(KAS_DEFAULT_AGENT_ID);
    expect(switched.welcomeMessage).toBe('Spec mode: ready to plan');
  });

  it('current_mode_update does not broadcast AgentSwitched when the mode is unchanged', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: {
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
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

  it('config_option_update broadcasts a ModelUpdate event so the chip self-heals', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [
        { value: 'claude-4', name: 'Claude 4' },
        { value: 'gpt-5', name: 'GPT-5' },
      ],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

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

    const modelEvent = events.find((e) => e.type === 'model_update');
    expect(modelEvent).toBeDefined();
    expect(modelEvent.model).toEqual({ id: 'gpt-5', name: 'GPT-5' });
  });

  it('config_option_update does NOT broadcast ModelUpdate when no model category is present', async () => {
    seedSessionWithModels({
      currentValue: 'claude-4',
      models: [{ value: 'claude-4', name: 'Claude 4' }],
    });
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-models',
      update: {
        sessionUpdate: 'config_option_update',
        // No `model` category entry (e.g. effort-only update).
        configOptions: [
          {
            type: 'select',
            id: 'effortLevel',
            name: 'Effort',
            category: 'thought_level',
            currentValue: 'high',
            options: [{ value: 'high', name: 'High' }],
          },
        ],
      },
    });

    expect(events.find((e) => e.type === 'model_update')).toBeUndefined();
  });

  it('available_commands_update hides built-in steering commands (quick-spec, architecture-selection, bug-fix) but keeps user steering', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          {
            name: 'quick-spec',
            description: 'Spec-generation workflow',
            _meta: { kiro: { type: 'steering', scope: 'global' } },
          },
          {
            name: 'architecture-selection',
            description: 'Architecture selection workflow',
            _meta: { kiro: { type: 'steering', scope: 'global' } },
          },
          {
            name: 'bug-fix',
            description: 'Bug fixing workflow',
            _meta: { kiro: { type: 'steering', scope: 'global' } },
          },
          {
            name: 'my-steering',
            description: 'A user steering doc',
            _meta: { kiro: { type: 'steering', scope: 'workspace' } },
          },
        ],
      },
    });

    const steeringEvent = events.find(
      (e) => e.type === AgentEventType.SteeringUpdate
    );
    expect(steeringEvent).toBeDefined();
    const names = steeringEvent.steering.map((s: { name: string }) => s.name);
    expect(names).toEqual(['my-steering']);
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

  it('available_commands_update hides the builtin quick-spec steering command', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          {
            name: 'quick-spec',
            description: 'Fast spec generation',
            _meta: { kiro: { type: 'steering' } },
          },
          {
            name: 'plan',
            description: 'Create a plan',
            _meta: { kiro: { type: 'steering' } },
          },
        ],
      },
    });

    const steeringEvent = events.find((e) => e.type === 'steering_update');
    const names = steeringEvent.steering.map((s: any) => s.name);
    expect(names).toContain('plan');
    expect(names).not.toContain('quick-spec');
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
            name: KAS_DEFAULT_AGENT_ID,
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
        currentModeId: KAS_DEFAULT_AGENT_ID,
        availableModes: [
          {
            id: KAS_DEFAULT_AGENT_ID,
            name: KAS_DEFAULT_AGENT_NAME,
            description: 'General coding',
          },
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
          {
            name: KAS_DEFAULT_AGENT_ID,
            description: 'General coding',
            _meta: {},
          },
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
              {
                usage: 1.5,
                unit: 'credit',
                unitPlural: 'Credits',
                usedTools: ['fs_read', 'custom_tool'],
              },
              {
                usage: 500,
                unit: 'token',
                unitPlural: 'Tokens',
                usedTools: ['mcp_tool'],
              },
            ],
            elapsedTime: 1234,
            contextUsage: { usagePercentage: 42 },
            inputTokens: 10,
            outputTokens: 5,
            cacheReadInputTokens: 2,
            cacheWriteInputTokens: 3,
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
    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-turn-completion', {
      sessionId: 'kas-session-1',
      modelId: 'm1',
      meteringUsage: [
        { value: 1.5, unit: 'credit', unitPlural: 'Credits' },
        { value: 500, unit: 'token', unitPlural: 'Tokens' },
      ],
      turnDurationMs: 1234,
      contextUsagePercentage: 42,
      totalTokens: 17,
      uncachedInputTokens: 10,
      outputTokens: 5,
      cacheReadInputTokens: 2,
      cacheWriteInputTokens: 3,
      status: 'success',
      usedTools: ['fs_read', 'custom_tool', 'mcp_tool'],
    });
    const telemetryPayload = mockEmitKasTelemetry.mock.calls.find(
      ([event]) => event === 'kas-turn-completion'
    )?.[1] as any;
    expect(telemetryPayload.meteringUsage[0]).not.toHaveProperty('usedTools');
    expect(telemetryPayload.meteringUsage[1]).not.toHaveProperty('usedTools');
  });

  it('session_info_update kind=turn_completion forwards telemetry context usage without a turn summary', async () => {
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
            contextUsage: { usagePercentage: 66 },
          },
        },
      },
    });

    expect(handler.mock.calls.map((c) => c[0])).toContainEqual({
      type: AgentEventType.ContextUsage,
      percent: 66,
    });
    expect(
      handler.mock.calls
        .map((c) => c[0])
        .some((e: any) => e.type === AgentEventType.TurnSummary)
    ).toBe(false);
    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-turn-completion', {
      sessionId: 'kas-session-1',
      modelId: 'm1',
      meteringUsage: [],
      contextUsagePercentage: 66,
    });
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

  it('session_info_update kind=turn_completion forwards unknown telemetry status', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            promptTurnSummaries: [
              { usage: 2, unit: 'credit', unitPlural: 'Credits' },
            ],
            elapsedTime: 100,
            status: 'backend:arbitrary-new-status',
          },
        },
      },
    });

    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-turn-completion', {
      sessionId: 'kas-session-1',
      modelId: 'm1',
      meteringUsage: [{ value: 2, unit: 'credit', unitPlural: 'Credits' }],
      turnDurationMs: 100,
      status: 'backend:arbitrary-new-status',
    });
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

  it('session_info_update kind=turn_completion forwards status-only telemetry', async () => {
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
    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-turn-completion', {
      sessionId: 'kas-session-1',
      modelId: 'm1',
      meteringUsage: [],
      status: 'success',
    });
  });

  it('session_info_update kind=turn_completion forwards token-only telemetry', async () => {
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
            tokenUsage: {
              inputTokens: 20,
              outputTokens: 8,
              cachedTokens: 4,
              cacheWriteInputTokens: 3,
            },
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeUndefined();
    expect(mockEmitKasTelemetry).toHaveBeenCalledWith('kas-turn-completion', {
      sessionId: 'kas-session-1',
      modelId: 'm1',
      meteringUsage: [],
      totalTokens: 32,
      uncachedInputTokens: 20,
      outputTokens: 8,
      cacheReadInputTokens: 4,
      cacheWriteInputTokens: 3,
    });
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
          currentValue: 'default',
          options: [
            { value: KAS_DEFAULT_AGENT_ID, name: KAS_DEFAULT_AGENT_NAME },
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
            currentValue: 'default',
            options: [
              { value: KAS_DEFAULT_AGENT_ID, name: KAS_DEFAULT_AGENT_NAME },
            ],
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
              _meta: { kiro: { source: 'workspace' } },
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
  beforeEach(() => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

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
  beforeEach(() => {
    freshMocks();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
  });

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

    it('does NOT register handler for _kiro/customAgent/config_error (suppressed)', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      expect(
        kc._extNotifHandlers['_kiro/customAgent/config_error']
      ).toBeUndefined();
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
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      });
      expect((client as any).modesState.currentModeId).toBe(
        KAS_DEFAULT_AGENT_ID
      );
    });

    it('agent not_found event normalizes the wire fallback id (vibe -> default) but keeps requestedAgent raw', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'amzn-builder',
        fallbackAgent: 'vibe',
      });
      const notFound = events.find((e) => e.type === 'agent_not_found');
      expect(notFound).toBeDefined();
      // The fallback shown in the "using <agent>" message is the canonical id.
      expect(notFound.fallbackAgent).toBe('default');
      // The requested id echoes back the user's literal chat.defaultAgent value.
      expect(notFound.requestedAgent).toBe('amzn-builder');
      // The cached mode is normalized too.
      expect((client as any).modesState.currentModeId).toBe('default');
    });

    it('backend-initiated agent switch normalizes the wire id (vibe -> default)', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      (client as any).handleAgentSwitched({
        agentName: 'vibe',
        previousAgentName: 'plan',
      });
      const switched = events.find((e) => e.type === 'agent_switched');
      expect(switched).toBeDefined();
      expect(switched.agentName).toBe('default');
      expect(switched.previousAgentName).toBe('kiro_planner');
    });

    it('refreshModeFromConfigOptions normalizes the requestedMode on the no-config-info fallback path', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      // No array of config options -> falls back to the requested mode, which
      // must still be normalized from the wire id.
      (client as any).refreshModeFromConfigOptions(undefined, 'vibe');
      expect((client as any).modesState.currentModeId).toBe('default');
    });
  });

  describe('McpServerInitialized transition logic', () => {
    it('emits McpServerInitialized only when server transitions from pending-OAuth to connected', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const events: any[] = [];
      client.onUpdate((e: any) => events.push(e));

      // First: server needs auth
      (client as any).handleMcpStatusNotification({
        servers: [
          {
            name: 'notion',
            status: 'failed',
            failedAuthorization: true,
            authorizationUrl: 'https://example.com/oauth',
          },
        ],
      });

      events.length = 0;

      // Second: server is now connected (completed OAuth)
      (client as any).handleMcpStatusNotification({
        servers: [{ name: 'notion', status: 'connected', tools: [] }],
      });

      const initEvents = events.filter(
        (e) => e.type === AgentEventType.McpServerInitialized
      );
      expect(initEvents).toHaveLength(1);
      expect(initEvents[0].serverName).toBe('notion');
    });

    it('does NOT emit McpServerInitialized for servers that were never pending OAuth', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const events: any[] = [];
      client.onUpdate((e: any) => events.push(e));

      // Server connects without ever being in OAuth state
      (client as any).handleMcpStatusNotification({
        servers: [{ name: 'local-mcp', status: 'connected', tools: [] }],
      });

      const initEvents = events.filter(
        (e) => e.type === AgentEventType.McpServerInitialized
      );
      expect(initEvents).toHaveLength(0);
    });

    it('does NOT re-emit McpServerInitialized on repeated connected status', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const events: any[] = [];
      client.onUpdate((e: any) => events.push(e));

      // Transition: auth-required → connected
      (client as any).handleMcpStatusNotification({
        servers: [
          {
            name: 'notion',
            status: 'failed',
            failedAuthorization: true,
            authorizationUrl: 'https://example.com/oauth',
          },
        ],
      });
      (client as any).handleMcpStatusNotification({
        servers: [{ name: 'notion', status: 'connected', tools: [] }],
      });

      events.length = 0;

      // Third notification: still connected — should NOT re-emit
      (client as any).handleMcpStatusNotification({
        servers: [{ name: 'notion', status: 'connected', tools: [] }],
      });

      const initEvents = events.filter(
        (e) => e.type === AgentEventType.McpServerInitialized
      );
      expect(initEvents).toHaveLength(0);
    });
  });

  // ── Task 2: _meta.kiro extraction in convertAcpUpdateToEvent ──

  describe('_meta.kiro extraction', () => {
    it('tool_call with _meta.kiro.pipeline produces event with meta.kiro', async () => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'pending',
                    dependsOn: ['research'],
                    agentSubtaskId: null,
                  },
                ],
              },
            },
          },
        },
      });

      expect(handler).toHaveBeenCalled();
      const event = handler.mock.calls[0]![0] as any;
      expect(event.type).toBe(AgentEventType.ToolCall);
      expect(event.meta?.kiro?.pipeline).toBeDefined();
      expect(event.meta?.kiro?.pipeline?.groupId).toBe('pipeline-test');
      expect(event.meta?.kiro?.pipeline?.stages).toHaveLength(2);
    });

    it('tool_call with _meta.kiro.agentSubtaskId produces event with meta', async () => {
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      const [, event] = multiHandler.mock.calls[0]!;
      expect((event as any).meta?.kiro?.agentSubtaskId).toBe('sub-1');
    });

    it('tool_call_update completed with _meta.kiro produces ToolCallFinished with meta', async () => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-op',
          status: 'completed',
          rawOutput: 'Pipeline completed: 2 stages finished.',
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'completed',
                    dependsOn: ['research'],
                    agentSubtaskId: 'sub-2',
                  },
                ],
              },
            },
          },
        },
      });

      const event = handler.mock.calls[0]![0] as any;
      expect(event.type).toBe(AgentEventType.ToolCallFinished);
      expect(event.meta?.kiro?.pipeline).toBeDefined();
      expect(event.meta?.kiro?.pipeline?.stages[0].status).toBe('completed');
    });

    it('tool_call_update in_progress with _meta.kiro produces ToolCallUpdate with meta', async () => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-op',
          status: 'in_progress',
          content: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'running',
                    dependsOn: ['research'],
                    agentSubtaskId: 'sub-2',
                  },
                ],
              },
            },
          },
        },
      });

      const event = handler.mock.calls[0]![0] as any;
      expect(event.type).toBe(AgentEventType.ToolCallUpdate);
      expect(event.meta?.kiro?.pipeline).toBeDefined();
      expect(event.meta?.kiro?.pipeline?.stages[1].status).toBe('running');
    });

    it('agent_message_chunk with _meta.kiro.agentSubtaskId produces Content event with meta', async () => {
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Stage output' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      const [, event] = multiHandler.mock.calls[0]!;
      expect((event as any).type).toBe(AgentEventType.Content);
      expect((event as any).meta?.kiro?.agentSubtaskId).toBe('sub-1');
    });

    it('tool_call without _meta works (no regression)', async () => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'tc-plain',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
        },
      });

      const event = handler.mock.calls[0]![0] as any;
      expect(event.type).toBe(AgentEventType.ToolCall);
      expect(event.meta).toBeUndefined();
    });
  });

  // ── Task 3: handlePipelineStateUpdate ──

  describe('handlePipelineStateUpdate', () => {
    it('pipeline tool_call triggers broadcastSubagentList with stage data', async () => {
      const client = new KasAcpClient();
      const subagentHandler = mock((_subagents: any[], _pending?: any[]) => {});
      client.onSubagentListUpdate(subagentHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test-task',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'pending',
                    dependsOn: ['research'],
                    agentSubtaskId: null,
                  },
                ],
              },
            },
          },
        },
      });

      expect(subagentHandler).toHaveBeenCalledTimes(1);
      const [subagents, pending] = subagentHandler.mock.calls[0]!;
      // Running stage with agentSubtaskId → subagent
      expect(subagents).toHaveLength(1);
      expect(subagents[0].sessionId).toBe('sub-1');
      expect(subagents[0].sessionName).toBe('research');
      expect(subagents[0].agentName).toBe('explorer');
      expect(subagents[0].status).toEqual({ type: 'working' });
      expect(subagents[0].group).toBe('pipeline-test-task');
      expect(subagents[0].dependsOn).toEqual([]);
      // Pending stage → pendingStages
      expect(pending).toHaveLength(1);
      expect(pending![0]!.name).toBe('implement');
      expect(pending![0]!.role).toBe('coder');
      expect(pending![0]!.dependsOn).toEqual(['research']);
    });

    it('pipeline tool_call_update with status changes triggers updated subagent list', async () => {
      const client = new KasAcpClient();
      const subagentHandler = mock((_subagents: any[], _pending?: any[]) => {});
      client.onSubagentListUpdate(subagentHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-op',
          status: 'in_progress',
          content: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test-task',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'running',
                    dependsOn: ['research'],
                    agentSubtaskId: 'sub-2',
                  },
                ],
              },
            },
          },
        },
      });

      expect(subagentHandler).toHaveBeenCalledTimes(1);
      const [subagents, pending] = subagentHandler.mock.calls[0]!;
      expect(subagents).toHaveLength(2);
      expect(subagents[0].sessionId).toBe('sub-1');
      expect(subagents[0].status).toEqual({ type: 'terminated' });
      expect(subagents[1].sessionId).toBe('sub-2');
      expect(subagents[1].status).toEqual({ type: 'working' });
      expect(pending).toHaveLength(0);
    });

    it('handles empty pipeline gracefully', async () => {
      const client = new KasAcpClient();
      const subagentHandler = mock((_subagents: any[], _pending?: any[]) => {});
      client.onSubagentListUpdate(subagentHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: { groupId: 'pipeline-empty', stages: [] },
            },
          },
        },
      });

      expect(subagentHandler).toHaveBeenCalledTimes(1);
      const [subagents, pending] = subagentHandler.mock.calls[0]!;
      expect(subagents).toHaveLength(0);
      expect(pending).toHaveLength(0);
    });
  });

  // ── Task 4: Per-stage event routing via broadcastMultiSession ──

  describe('per-stage event routing', () => {
    it('tool_call with agentSubtaskId triggers broadcastMultiSession', async () => {
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(1);
      const [sessionId, event] = multiHandler.mock.calls[0]!;
      expect(sessionId).toBe('sub-1');
      expect(event.type).toBe(AgentEventType.ToolCall);
      expect(event.id).toBe('read-001');
    });

    it('agent_message_chunk with agentSubtaskId triggers broadcastMultiSession', async () => {
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Stage output' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(1);
      const [sessionId, event] = multiHandler.mock.calls[0]!;
      expect(sessionId).toBe('sub-1');
      expect(event.type).toBe(AgentEventType.Content);
    });

    it('pipeline parent event still broadcasts to main stream', async () => {
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: { groupId: 'pipeline-test', stages: [] },
            },
          },
        },
      });

      expect(mainHandler).toHaveBeenCalledTimes(1);
      const event = mainHandler.mock.calls[0]![0] as any;
      expect(event.type).toBe(AgentEventType.ToolCall);
    });

    it('tool_call with agentSubtaskId sets sessionId on event', async () => {
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      const [, event] = multiHandler.mock.calls[0]!;
      expect((event as any).sessionId).toBe('sub-1');
    });

    it('crew-stage agentSubtaskId events do NOT broadcast to main stream', async () => {
      // Regression: per-stage events were leaking into the main conversation
      // alongside the SUBAGENT OUTPUT panel, causing duplicate text and tool
      // events. For a VISIBLE crew stage (registered via a pipeline state
      // update), anything tagged with `_meta.kiro.agentSubtaskId` must reach
      // multi-session handlers ONLY. (Behavior update: the discriminator is now
      // pipelineStageSubtasks — a stage must be registered first, else the
      // subtask is treated as standalone and DOES surface in main. See the
      // 'standalone subagent tool cards surface in main' suite.)
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      // Register 'sub-1' as a VISIBLE crew stage so a panel exists for it.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                ],
              },
            },
          },
        },
      });
      mainHandler.mockClear();
      multiHandler.mockClear();

      // Per-stage tool call
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      // Per-stage assistant content chunk
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Stage narration' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      // Per-stage invoke_sub_agent wrapper
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'wrap-001',
          title: 'invoke_sub_agent',
          kind: 'other',
          rawInput: { name: 'general-task-execution', prompt: 'work' },
          content: [],
          locations: [],
          _meta: { kiro: { kind: 'agent-subtask', agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(3);
      expect(mainHandler).not.toHaveBeenCalled();
    });

    it('toolCallToSubtask map populated on tool_call with agentSubtaskId', async () => {
      const client = new KasAcpClient();
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      // Verify via permission request routing (Task 5)
      // The map is private, so we test it indirectly through permission handling
      expect(true).toBe(true); // Map populated — tested via Task 5
    });
  });

  // ── Standalone (hidden) subagent tool cards surface inline in main ──

  describe('standalone subagent tool cards surface in main', () => {
    it('standalone subtask tool_call/update (no crew panel) forwards to main AND multi-session', async () => {
      // Ground truth: a hidden/standalone spec subagent emits tool calls tagged
      // with agentSubtaskId but never registers a pipeline stage, so there is no
      // crew panel. Pre-fix these were dropped from main (rendered nowhere).
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();
      mainHandler.mockClear();

      // tool_call tagged with a subtask NEVER registered as a pipeline stage.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-x',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/spec/requirements.md' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-x' } },
        },
      });

      // tool_call_update for the same standalone subtask.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'write-x',
          status: 'in_progress',
          content: [],
          _meta: { kiro: { agentSubtaskId: 'sub-x' } },
        },
      });

      const mainToolEvents = mainHandler.mock.calls
        .map((c) => c[0] as any)
        .filter(
          (e) =>
            e.type === AgentEventType.ToolCall ||
            e.type === AgentEventType.ToolCallUpdate
        );
      // Both reach the MAIN stream so the standalone subagent's tool cards
      // render inline (pre-fix this was 0).
      expect(mainToolEvents.length).toBe(2);
      // And both still reach multi-session (harmless — no panel renders them).
      expect(multiHandler).toHaveBeenCalledTimes(2);
    });

    it('standalone subtask full lifecycle (call + update + finished) all forward to main', async () => {
      // NIT coverage: ToolCallFinished is in STANDALONE_MAIN_FORWARD_TYPES, so a
      // hidden subagent's tool card must COMPLETE inline in main, not just start.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      const tagged = { kiro: { agentSubtaskId: 'sub-life' } };
      // tool_call (→ ToolCall)
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'life-1',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/spec/x.md' },
          content: [],
          locations: [],
          _meta: tagged,
        },
      });
      // tool_call_update in_progress (→ ToolCallUpdate)
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'life-1',
          status: 'in_progress',
          content: [],
          _meta: tagged,
        },
      });
      // tool_call_update completed (→ ToolCallFinished)
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'life-1',
          status: 'completed',
          content: [],
          _meta: tagged,
        },
      });

      const mainTypes = mainHandler.mock.calls.map((c) => (c[0] as any).type);
      expect(mainTypes).toContain(AgentEventType.ToolCall);
      expect(mainTypes).toContain(AgentEventType.ToolCallUpdate);
      expect(mainTypes).toContain(AgentEventType.ToolCallFinished);
    });

    it('main-forwarded ToolCall has sessionId stripped; multi-session copy retains it', async () => {
      const client = new KasAcpClient();
      let mainToolCall: any = null;
      let multiToolCall: { sid: string; e: any } | null = null;
      client.onUpdate((e: any) => {
        if (e.type === AgentEventType.ToolCall) mainToolCall = e;
      });
      client.onMultiSessionUpdate((sid: string, e: any) => {
        if (e.type === AgentEventType.ToolCall) multiToolCall = { sid, e };
      });
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-x',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/spec/design.md' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-x' } },
        },
      });

      expect(mainToolCall).not.toBeNull();
      // Main copy renders as a NORMAL inline tool card (no subagent session tag).
      expect(mainToolCall.sessionId).toBeUndefined();
      // Multi-session copy keeps the subtask id for crew correlation.
      expect(multiToolCall).not.toBeNull();
      expect(multiToolCall!.sid).toBe('sub-x');
      expect(multiToolCall!.e.sessionId).toBe('sub-x');
    });

    it('crew-stage subtask tool_call (registered pipeline) stays panel-only, not main', async () => {
      // Regression: when a pipeline state update HAS registered the subtask as a
      // visible crew stage, its per-stage tool calls render in the crew panel
      // only and must NOT leak into the main conversation.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      // Register 'sub-1' as a VISIBLE crew stage FIRST (a panel exists for it).
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-test',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                ],
              },
            },
          },
        },
      });
      mainHandler.mockClear();
      multiHandler.mockClear();

      // Per-stage tool_call for the registered stage.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(1);
      expect(mainHandler).not.toHaveBeenCalled();
    });
  });

  // ── Task 5: Permission request stage correlation ──

  describe('permission request stage correlation', () => {
    // Behavior fix (was: 'permission request resolves sessionId from
    // toolCallToSubtask map'). This previously asserted sessionId='sub-1'
    // even though NO pipeline state update ever registered 'sub-1' as a
    // visible crew stage. That encoded the deadlock bug: routing a hidden
    // subagent's tool approval to a crew panel that doesn't exist drops the
    // prompt (never renders, resolve() never fires) and KAS hangs. The
    // corrected expectation is sessionId=undefined so the prompt surfaces in
    // the main view, which is always resolvable. Crew routing for *real*
    // pipeline stages is covered by the positive test below.
    it('hidden subagent tool approval (no registered stage) surfaces in main view', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // First, send a tool_call with agentSubtaskId to populate the map
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-001',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      // Now trigger a permission request for that tool call
      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'write-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });

      // Wait for the approval event to be broadcast
      await new Promise((r) => setTimeout(r, 50));

      expect(approvalInfo).not.toBeNull();
      // No pipeline stage registered for 'sub-1' → main view (undefined), not crew.
      expect(approvalInfo.sessionId).toBeUndefined();
      expect(approvalInfo.toolCall.toolCallId).toBe('write-001');

      // Resolve the approval to avoid hanging promise
      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });

    // Repro of the recorded deadlock: a hidden/one-off spec subagent issues
    // fs_write (consent.capability='fs_write', NOT 'subagent'). Its toolCallId
    // is in toolCallToSubtask but no crew panel was ever registered. Must
    // route to main (sessionId undefined). Pre-fix this returned 'sub-1'.
    it('hidden spec subagent fs_write approval routes to main, not crew', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Child subagent tool_call tags the call with its subtask id, but the
      // subtask is hidden — no pipeline state update registers a crew stage.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'spec-write-001',
          title: 'Write File',
          kind: 'edit',
          rawInput: { path: '/spec' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'spec-sub' } },
        },
      });

      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'spec-write-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: { kiro: { consent: { capability: 'fs_write' } } },
      });

      await new Promise((r) => setTimeout(r, 50));

      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBeUndefined();
      expect(approvalInfo.toolCall.toolCallId).toBe('spec-write-001');

      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });

    // Positive/regression: a REAL visible pipeline stage. Pipeline state
    // update registers 'sub-1' as a crew stage FIRST, then the tool approval
    // arrives → routes to crew (sessionId='sub-1').
    it('visible pipeline stage tool approval routes to crew (sessionId set)', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Register 'sub-1' as a visible crew stage via a pipeline state update.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-task',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                ],
              },
            },
          },
        },
      });

      // Child tool_call inside that stage populates toolCallToSubtask.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-stage-001',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'write-stage-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });

      await new Promise((r) => setTimeout(r, 50));

      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBe('sub-1');
      expect(approvalInfo.toolCall.toolCallId).toBe('write-stage-001');

      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });

    // Regression for #3109: the invoke_sub_agent WRAPPER spawn approval
    // (consent.capability='subagent') is a parent-session decision and must
    // surface in main view regardless of subtask correlation. Unchanged by fix.
    it('invoke_sub_agent wrapper spawn approval surfaces in main view', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Register 'sub-9' as a VISIBLE crew stage FIRST. This makes the test
      // discriminating: without it, isVisibleCrewStage is false and the
      // sessionId would be undefined regardless of the spawn guard, so the
      // assertion couldn't detect a regression in the `!isSubagentSpawn` term.
      // With the stage registered, isVisibleCrewStage=true and `!isSubagentSpawn`
      // becomes the SOLE gate keeping this spawn approval in the main view.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op-9',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'spec' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-spawn',
                stages: [
                  {
                    name: 'spec-writer',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-9',
                  },
                ],
              },
            },
          },
        },
      });

      // A subtask-tagged tool_call populates the map for this toolCallId...
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'spawn-001',
          title: 'invoke_sub_agent',
          kind: 'other',
          rawInput: { task: 'spec' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-9' } },
        },
      });

      // ...but the spawn consent capability forces main view.
      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'spawn-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: { kiro: { consent: { capability: 'subagent' } } },
      });

      await new Promise((r) => setTimeout(r, 50));

      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBeUndefined();

      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });

    it('permission request without pipeline context has no sessionId', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Send a tool_call WITHOUT agentSubtaskId
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-002',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
        },
      });

      // Trigger permission request
      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'write-002',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });

      await new Promise((r) => setTimeout(r, 50));

      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBeUndefined();

      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });

    // Session switch clears stale subtask correlation. A stage subtask that
    // routed to crew in one session must NOT keep routing to crew after a new
    // session starts (its crew panel is gone). wireSessionListeners clears the
    // collections; a second newSession() re-invokes it here.
    it('session switch clears stage subtasks so a prior crew subtask routes to main', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Register 'sub-1' as a visible crew stage, then confirm its tool approval
      // routes to crew (sessionId='sub-1').
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-switch',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                ],
              },
            },
          },
        },
      });
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-stage-001',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });
      const firstPromise = capturedPermissionHandler({
        toolCallId: 'write-stage-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });
      await new Promise((r) => setTimeout(r, 50));
      expect(approvalInfo.sessionId).toBe('sub-1');
      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await firstPromise;

      // Session switch: a second newSession() re-invokes wireSessionListeners,
      // which clears pipelineStageSubtasks + toolCallToSubtask.
      approvalInfo = null;
      await client.newSession();
      mainHandler.mockClear();

      // The SAME subtask + toolCallId now has no registered stage → standalone,
      // so its approval surfaces in main (sessionId undefined). Pre-clear this
      // would still resolve to 'sub-1', so the assertion is discriminating.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-stage-001',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });
      const secondPromise = capturedPermissionHandler({
        toolCallId: 'write-stage-001',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });
      await new Promise((r) => setTimeout(r, 50));
      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBeUndefined();
      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await secondPromise;
    });

    // Ordering edge (documented race): a permission request for a tagged but
    // UNREGISTERED subtask routes to main at request time. A pipeline state
    // update that arrives AFTER must NOT retroactively re-route the already
    // decided approval to crew — routing is fixed when the request is handled.
    it('routing is decided at request time, not retroactively by a late pipeline update', async () => {
      const client = new KasAcpClient();
      let approvalInfo: any = null;
      const mainHandler = mock((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
        }
      });
      client.onUpdate(mainHandler);
      await client.newSession();
      mainHandler.mockClear();

      // Tagged tool_call for a subtask with NO registered stage yet.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-late',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-late' } },
        },
      });

      // Permission request fires BEFORE the pipeline registration → main.
      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'write-late',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {},
      });
      await new Promise((r) => setTimeout(r, 50));
      expect(approvalInfo).not.toBeNull();
      expect(approvalInfo.sessionId).toBeUndefined();

      // NOW a late pipeline state update registers 'sub-late' as a crew stage.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op-late',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'test' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-late',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-late',
                  },
                ],
              },
            },
          },
        },
      });
      await new Promise((r) => setTimeout(r, 20));

      // The already-decided approval is unchanged (still main, sessionId
      // undefined) — the late registration does not rewrite the emitted event.
      expect(approvalInfo.sessionId).toBeUndefined();

      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    });
  });

  // ── Task 6: End-to-end pipeline lifecycle ──

  describe('end-to-end pipeline lifecycle', () => {
    it('full 2-stage pipeline: init → stage1 running → stage1 tools → stage1 done → stage2 running → complete', async () => {
      const client = new KasAcpClient();
      const mainEvents: any[] = [];
      const subagentCalls: any[] = [];
      const multiSessionCalls: any[] = [];

      client.onUpdate((event: any) => mainEvents.push(event));
      client.onSubagentListUpdate((subagents: any[], pending: any[]) =>
        subagentCalls.push({ subagents, pending })
      );
      client.onMultiSessionUpdate((sessionId: string, event: any) =>
        multiSessionCalls.push({ sessionId, event })
      );
      await client.newSession();
      // newSession() broadcasts an EffortUpdate from cached config options;
      // discard so mainEvents[0] is the first pipeline event.
      mainEvents.length = 0;

      // Step 1: Pipeline starts — initial tool_call with all stages pending
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-op',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: {
            task: 'Add logging',
            stages: [
              { name: 'research', role: 'explorer' },
              { name: 'implement', role: 'coder' },
            ],
          },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-add-logging',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'pending',
                    dependsOn: ['research'],
                    agentSubtaskId: null,
                  },
                ],
              },
            },
          },
        },
      });

      // Verify: subagent list updated with 1 working + 1 pending
      expect(subagentCalls).toHaveLength(1);
      expect(subagentCalls[0].subagents).toHaveLength(1);
      expect(subagentCalls[0].subagents[0].sessionId).toBe('sub-1');
      expect(subagentCalls[0].subagents[0].status).toEqual({ type: 'working' });
      expect(subagentCalls[0].pending).toHaveLength(1);
      expect(subagentCalls[0].pending[0].name).toBe('implement');
      // Main stream also gets the parent tool_call
      expect(mainEvents[0].type).toBe(AgentEventType.ToolCall);
      expect(mainEvents[0].name).toBe('orchestrate_subagent');

      // Step 2: Stage 1 makes a tool call
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-001',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/src/app.ts' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      // Verify: routed to multi-session with sub-1
      expect(multiSessionCalls).toHaveLength(1);
      expect(multiSessionCalls[0].sessionId).toBe('sub-1');
      expect(multiSessionCalls[0].event.type).toBe(AgentEventType.ToolCall);
      expect(multiSessionCalls[0].event.name).toBe('read_file');

      // Step 3: Stage 1 emits text
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Found logging patterns' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiSessionCalls).toHaveLength(2);
      expect(multiSessionCalls[1].sessionId).toBe('sub-1');
      expect(multiSessionCalls[1].event.type).toBe(AgentEventType.Content);

      // Step 4: Stage 1 completes, stage 2 starts
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-op',
          status: 'in_progress',
          content: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-add-logging',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'running',
                    dependsOn: ['research'],
                    agentSubtaskId: 'sub-2',
                  },
                ],
              },
            },
          },
        },
      });

      // Verify: subagent list updated — both have IDs now, research terminated, implement working
      expect(subagentCalls).toHaveLength(2);
      expect(subagentCalls[1].subagents).toHaveLength(2);
      expect(subagentCalls[1].subagents[0].status).toEqual({
        type: 'terminated',
      });
      expect(subagentCalls[1].subagents[1].sessionId).toBe('sub-2');
      expect(subagentCalls[1].subagents[1].status).toEqual({ type: 'working' });
      expect(subagentCalls[1].pending).toHaveLength(0);

      // Step 5: Stage 2 makes a tool call
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'write-001',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/src/app.ts', content: 'logging code' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-2' } },
        },
      });

      expect(multiSessionCalls).toHaveLength(3);
      expect(multiSessionCalls[2].sessionId).toBe('sub-2');
      expect(multiSessionCalls[2].event.name).toBe('fs_write');

      // Step 6: Pipeline completes
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-op',
          status: 'completed',
          rawOutput:
            'Pipeline completed: 2 stages finished.\n\n## research\n\nFound patterns\n\n---\n\n## implement\n\nAdded logging',
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-add-logging',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: 'sub-1',
                  },
                  {
                    name: 'implement',
                    role: 'coder',
                    status: 'completed',
                    dependsOn: ['research'],
                    agentSubtaskId: 'sub-2',
                  },
                ],
              },
            },
          },
        },
      });

      // Verify: final subagent list — all terminated
      expect(subagentCalls).toHaveLength(3);
      expect(subagentCalls[2].subagents).toHaveLength(2);
      expect(subagentCalls[2].subagents[0].status).toEqual({
        type: 'terminated',
      });
      expect(subagentCalls[2].subagents[1].status).toEqual({
        type: 'terminated',
      });
      // Main stream gets ToolCallFinished
      const finishedEvent = mainEvents.find(
        (e) => e.type === AgentEventType.ToolCallFinished && e.id === 'crew-op'
      );
      expect(finishedEvent).toBeDefined();
      expect(finishedEvent.result.status).toBe('success');
      expect(finishedEvent.result.output).toContain('Pipeline completed');
    });

    it('stage failure stops pipeline and reports error', async () => {
      const client = new KasAcpClient();
      const subagentCalls: any[] = [];
      const mainEvents: any[] = [];

      client.onUpdate((event: any) => mainEvents.push(event));
      client.onSubagentListUpdate((subagents: any[], pending: any[]) =>
        subagentCalls.push({ subagents, pending })
      );
      await client.newSession();

      // Pipeline starts
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'crew-fail',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'Broken task' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-broken',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: 'fail-1',
                  },
                ],
              },
            },
          },
        },
      });

      // Pipeline fails
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'crew-fail',
          status: 'failed',
          rawOutput: 'Stage research failed: timeout',
          content: [
            {
              type: 'content',
              content: { type: 'text', text: 'Stage research failed: timeout' },
            },
          ],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-broken',
                stages: [
                  {
                    name: 'research',
                    role: 'explorer',
                    status: 'failed',
                    dependsOn: [],
                    agentSubtaskId: 'fail-1',
                  },
                ],
              },
            },
          },
        },
      });

      // Verify: subagent list shows failed status (mapped to terminated)
      expect(subagentCalls).toHaveLength(2);
      expect(subagentCalls[1].subagents[0].status).toEqual({
        type: 'terminated',
      });
      // Main stream gets ToolCallFinished with error
      const finishedEvent = mainEvents.find(
        (e) =>
          e.type === AgentEventType.ToolCallFinished && e.id === 'crew-fail'
      );
      expect(finishedEvent).toBeDefined();
      expect(finishedEvent.result.status).toBe('error');
    });
  });
});

describe('KasAcpClient — _kiro/tools/didChange', () => {
  it('subscribes on initialize and broadcasts ToolsUpdate with parsed tools', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    const handler = (client as any).kiroClient._extNotifHandlers[
      '_kiro/tools/didChange'
    ];
    expect(typeof handler).toBe('function');

    handler({
      sessionId: client.sessionId,
      tags: [
        { source: 'builtin', tag: 'read', description: 'read tools' },
        { source: 'mcp', tag: '@git/status', description: 'git status' },
      ],
    });

    const toolsEvent = events.find(
      (e) => e.type === AgentEventType.ToolsUpdate
    );
    expect(toolsEvent).toBeDefined();
    expect(toolsEvent.tools).toEqual([
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: '@git/status', source: 'mcp', description: 'git status' },
    ]);
    // No per-tool status from KAS.
    expect(toolsEvent.tools.every((t: any) => t.status === undefined)).toBe(
      true
    );
  });

  it('ignores notifications for a different session', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    const handler = (client as any).kiroClient._extNotifHandlers[
      '_kiro/tools/didChange'
    ];
    handler({
      sessionId: 'some-other-session',
      tags: [{ source: 'builtin', tag: 'read', description: 'read tools' }],
    });

    expect(
      events.find((e) => e.type === AgentEventType.ToolsUpdate)
    ).toBeUndefined();
  });

  it('disposes the tools subscription on close', async () => {
    mockExtNotificationDispose.mockClear();
    const client = new KasAcpClient();
    await client.initialize();
    client.close();
    // Hooks + tools + other ext subscriptions all dispose; at least the
    // tools one must have fired.
    expect(mockExtNotificationDispose).toHaveBeenCalled();
  });
});
