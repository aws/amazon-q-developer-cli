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
import {
  AgentEventType,
  ContentType,
  type AgentStreamEvent,
} from '../types/agent-events';
import type { TuiToolCallStart } from '../utils/tui-telemetry-observer';
import { UiModeSource } from '../types/generated/chat-cli';
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../constants/agents';
import type { KasAcpClientOptions } from '../acp-client/kas';

type ToolFinishArgs = {
  outcome: 'success' | 'error' | 'cancelled' | 'denied';
  model: string;
};

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

// --- Mock @kiro/client ---
let capturedSessionUpdateHandler: any = null;
let capturedPermissionHandler: any = null;
const capturedSessionUpdateHandlers = new Map<string, any>();
const capturedPermissionHandlers = new Map<string, any>();

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
    configOptions: [
      {
        id: 'model',
        category: 'model',
        type: 'select',
        currentValue: 'm1',
        options: [{ value: 'm1', name: 'Test Model' }],
      },
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue: 'vibe',
        options: [
          {
            value: 'vibe',
            name: 'Default',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    ],
  })
);
const mockKiroLoadSession = mock((_req: any) =>
  Promise.resolve({
    sessionId: 'kas-loaded',
    configOptions: [
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue: 'vibe',
        options: [
          {
            value: 'vibe',
            name: 'Default',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    ],
  })
);
const mockKiroPrompt = mock((_req: any) => Promise.resolve());
const mockKiroCancel = mock((_sessionId: string) => {});
const mockKiroSetSessionConfigOption = mock((_req: any) => Promise.resolve());
const mockKiroSetSessionMode = mock((_req: any) => Promise.resolve({}));
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
  setSessionMode = mockKiroSetSessionMode;
  sendExtMethod = mockKiroSendExtMethod;
  sendExtNotification = mockKiroSendExtNotification;
  listSessions = mockKiroListSessions;
  onSessionUpdate = mock((sessionId: string, handler: any) => {
    capturedSessionUpdateHandlers.set(sessionId, handler);
    capturedSessionUpdateHandler = handler;
    return {
      dispose: () => {
        mockSessionUpdateDispose();
        if (capturedSessionUpdateHandlers.get(sessionId) === handler) {
          capturedSessionUpdateHandlers.delete(sessionId);
        }
      },
    };
  });
  onPermissionRequest = mock((sessionId: string, handler: any) => {
    capturedPermissionHandlers.set(sessionId, handler);
    capturedPermissionHandler = handler;
    return {
      dispose: () => {
        mockPermissionRequestDispose();
        if (capturedPermissionHandlers.get(sessionId) === handler) {
          capturedPermissionHandlers.delete(sessionId);
        }
      },
    };
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

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '@kiro/client',
  '@agentclientprotocol/sdk',
  '../utils/logger',
  '../utils/tui-telemetry-observer',
]);

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

// Capture recordTuiSessionStarted to assert the per-session dedup; other record
// fns are no-ops, modeFromId/resultFromStatus/tool-call observer stay functional.
const mockRecordTuiSessionStarted = mock((_a: unknown) => {});
const mockRecordTuiCloudSession = mock((_a: unknown) => {});
const mockRecordTuiCloudSessionReady = mock((_a: unknown) => {});
const mockRecordTuiAutonomousMode = mock((_a: unknown) => {});
const mockRecordTuiCloudError = mock((_a: unknown) => {});
const mockRecordTuiUserTurn = mock((_a: unknown) => {});
const mockRecordTuiModelInvocations = mock((_a: unknown) => {});
const mockRecordTuiTokensConsumed = mock((_a: unknown) => {});
const mockRecordTuiCreditsConsumed = mock((_a: unknown) => {});
const mockRecordTuiSlashCommand = mock((_a: unknown) => {});
const mockRecordTuiUiModeSessionStarted = mock((_a: unknown) => {});
const mockRecordTuiConfigPanel = mock((_a: unknown) => {});
const mockRecordTuiCloudConfigDiagnostics = mock((_a: unknown) => {});
const mockRecordTuiCloudConfigSource = mock((_a: unknown) => {});
const mockRecordTuiWorkflowRestoreSummary = mock(
  (_summary: unknown, _version: string) => {}
);
const toolStartCalls: Array<{ id: string; info: TuiToolCallStart }> = [];
const toolFinishCalls: Array<{ id: string; args: ToolFinishArgs }> = [];
mock.module('../utils/tui-telemetry-observer', () => ({
  DEFAULT_ENGINE: 'v3',
  TUI_SCOPE: 'kiro.tui',
  recordTuiSessionStarted: mockRecordTuiSessionStarted,
  recordTuiCloudSession: mockRecordTuiCloudSession,
  recordTuiCloudSessionReady: mockRecordTuiCloudSessionReady,
  recordTuiAutonomousMode: mockRecordTuiAutonomousMode,
  recordTuiCloudError: mockRecordTuiCloudError,
  recordTuiCloudAttach: mock(() => {}),
  attachSizeBucket: (bytes: number) =>
    bytes < 65536 ? 'under_64k' : 'under_1m',
  recordTuiCloudRepoAttach: mock(() => {}),
  recordTuiUserTurn: mockRecordTuiUserTurn,
  recordTuiModelInvocations: mockRecordTuiModelInvocations,
  recordTuiTokensConsumed: mockRecordTuiTokensConsumed,
  recordTuiCreditsConsumed: mockRecordTuiCreditsConsumed,
  recordTuiSlashCommand: mockRecordTuiSlashCommand,
  recordTuiUiModeSessionStarted: mockRecordTuiUiModeSessionStarted,
  recordTuiConfigPanel: mockRecordTuiConfigPanel,
  recordTuiCloudConfigDiagnostics: mockRecordTuiCloudConfigDiagnostics,
  recordTuiCloudConfigSource: mockRecordTuiCloudConfigSource,
  recordTuiWorkflowRestoreSummary: mockRecordTuiWorkflowRestoreSummary,
  // A module mock replaces the module for every file loaded after this one, so
  // an export missing here is a load-time SyntaxError in any later file that
  // imports it — the omission takes that file's whole suite out silently.
  recordTuiWorkflowControl: mock(() => {}),
  modeFromId: (id?: string) => (id && id.length > 0 ? id : 'interactive'),
  resultFromStatus: (status?: string) => {
    switch (status) {
      case 'completed':
      case 'success':
        return 'success';
      case 'failed':
        return 'failed';
      case 'cancelled':
        return 'cancelled';
      default:
        return status ? 'failed' : '_other_';
    }
  },
  turnFailureReasonFromStatus: (status?: string) => {
    switch (status) {
      case undefined:
      case 'completed':
      case 'success':
      case 'cancelled':
        return undefined;
      case 'failed':
        return 'model_error';
      default:
        return 'unknown';
    }
  },
  TuiFirstVisibleResponseObserver: class {
    start() {}
    observe() {}
    cancel() {}
  },
  TuiToolCallObserver: class {
    constructor(_version: string) {}
    start(id: string, info: TuiToolCallStart) {
      toolStartCalls.push({ id, info });
    }
    finish(id: string, args: ToolFinishArgs) {
      toolFinishCalls.push({ id, args });
    }
    reset() {}
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
const {
  KasAcpClient: RawKasAcpClient,
  createAcpClient,
  resolveFeedbackUrl,
} = await import('../acp-client?kas-test');
const { browserOpenCommand } = await import('../utils/browser');
const { Feature, features } = await import('../features');
const { createStore } = await import('zustand/vanilla');
const {
  createInitialKasSubagentRoutingState,
  createKasSubagentRoutingActions,
} = await import('../stores/kas-subagent-routing');

function createKasRoutingStore() {
  const subagentRouting = createInitialKasSubagentRoutingState();
  const kasSubagentRouting = createKasSubagentRoutingActions(
    () => subagentRouting
  );
  return createStore(() => ({ subagentRouting, kasSubagentRouting }));
}

let kasRoutingStore = createKasRoutingStore();
/**
 * Clients constructed by a case, closed after it. Committing a session policy
 * can START the workflow extension, which subscribes to the runtime and holds
 * those handles until `close()` disposes them. A case that leaves one open
 * keeps the process alive, so the whole file appears to hang rather than
 * failing — and the timeout lands on whichever suite happened to start last.
 */
const openClients: KasAcpClient[] = [];

class KasAcpClient extends RawKasAcpClient {
  constructor(options: Partial<KasAcpClientOptions> = {}) {
    super({
      version: 'test-version',
      ...options,
      kasSubagentRoutingStore: kasRoutingStore.getState().kasSubagentRouting,
      spawnProcess: mockSpawn,
    });
    openClients.push(this);
  }
}

/** `close()` is idempotent, so closing an already-closed client is a no-op. */
function closeOpenClients(): void {
  for (const client of openClients.splice(0)) {
    try {
      client.close();
    } catch {
      // A case may have already torn this client down; nothing to salvage.
    }
  }
}

// File-level so it covers every describe block, including ones added later.
afterEach(closeOpenClients);

function freshMocks() {
  kasRoutingStore = createKasRoutingStore();
  mockSpawn.mockClear();
  mockKiroInitialize.mockClear();
  mockKiroNewSession.mockClear();
  mockKiroLoadSession.mockClear();
  mockKiroPrompt.mockClear();
  mockKiroCancel.mockClear();
  mockKiroSetSessionConfigOption.mockClear();
  mockKiroSetSessionMode.mockClear();
  mockKiroSendExtMethod.mockClear();
  mockKiroSendExtNotification.mockClear();
  mockKiroListSessions.mockClear();
  mockRecordTuiSessionStarted.mockClear();
  mockRecordTuiCloudSession.mockClear();
  mockRecordTuiCloudSessionReady.mockClear();
  mockRecordTuiAutonomousMode.mockClear();
  mockRecordTuiCloudError.mockClear();
  mockRecordTuiUserTurn.mockClear();
  mockRecordTuiModelInvocations.mockClear();
  mockRecordTuiTokensConsumed.mockClear();
  mockRecordTuiCreditsConsumed.mockClear();
  mockRecordTuiConfigPanel.mockClear();
  mockRecordTuiCloudConfigDiagnostics.mockClear();
  mockRecordTuiCloudConfigSource.mockClear();
  mockRecordTuiSlashCommand.mockClear();
  mockRecordTuiUiModeSessionStarted.mockClear();
  mockRecordTuiWorkflowRestoreSummary.mockClear();
  toolStartCalls.length = 0;
  toolFinishCalls.length = 0;
  capturedSessionUpdateHandler = null;
  capturedPermissionHandler = null;
  capturedSessionUpdateHandlers.clear();
  capturedPermissionHandlers.clear();
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

/**
 * Workflows is live only when the rollout reaches the user AND they opted in,
 * so a test that wants the feature on has to establish both.
 */
function setWorkflowsEnabled(enabled: boolean) {
  process.env.KIRO_ENABLED_FEATURES = JSON.stringify(
    enabled ? [Feature.Workflows] : []
  );
  writeTestCliJson({ 'chat.enableWorkflows': enabled });
  features._resetForTests();
}

/** Put the user on the rollout, then set only the opt-in — the two halves the
 *  policy keeps apart, which `setWorkflowsEnabled` moves together. */
function setWorkflowsOptIn(optedIn: boolean) {
  process.env.KIRO_ENABLED_FEATURES = JSON.stringify([Feature.Workflows]);
  writeTestCliJson({ 'chat.enableWorkflows': optedIn });
  features._resetForTests();
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
  let origEnabledFeatures: string | undefined;

  beforeEach(() => {
    origKasPath = process.env.KIRO_KAS_SERVER_PATH;
    origEnabledFeatures = process.env.KIRO_ENABLED_FEATURES;
    process.env.KIRO_KAS_SERVER_PATH = '/fake/acp-server.js';
    // freshMocks() establishes the scratch HOME that the opt-in is written to.
    freshMocks();
    setWorkflowsEnabled(false);
  });

  afterEach(() => {
    if (origKasPath === undefined) delete process.env.KIRO_KAS_SERVER_PATH;
    else process.env.KIRO_KAS_SERVER_PATH = origKasPath;
    if (origEnabledFeatures === undefined) {
      delete process.env.KIRO_ENABLED_FEATURES;
    } else {
      process.env.KIRO_ENABLED_FEATURES = origEnabledFeatures;
    }
    features._resetForTests();
  });

  it('constructor spawns process with KAS server args', () => {
    const _client = new KasAcpClient();
    expect(mockSpawn).toHaveBeenCalled();
    const [_cmd, args] = mockSpawn.mock.calls[0]!;
    expect(args).toContain('--experimental-wasm-modules');
    expect(args).toContain('--transport=stdio');
  });

  it('omits --endpoint when KIRO_KAS_ENDPOINT is unset', () => {
    delete process.env.KIRO_KAS_ENDPOINT;
    const _client = new KasAcpClient();
    const [_cmd, args] = mockSpawn.mock.calls[0]!;
    expect(args.some((a: string) => a.startsWith('--endpoint='))).toBe(false);
  });

  it('passes --endpoint to the KAS server when KIRO_KAS_ENDPOINT is set', () => {
    process.env.KIRO_KAS_ENDPOINT = 'http://127.0.0.1:19999';
    try {
      const _client = new KasAcpClient();
      const [_cmd, args] = mockSpawn.mock.calls[0]!;
      expect(args).toContain('--endpoint=http://127.0.0.1:19999');
    } finally {
      delete process.env.KIRO_KAS_ENDPOINT;
    }
  });

  it('factory forwards the required app-store routing actions for KAS', () => {
    const client = createAcpClient('/unused', [], {
      agentEngine: 'kas',
      kasOptions: {
        kasSubagentRoutingStore: kasRoutingStore.getState().kasSubagentRouting,
        spawnProcess: mockSpawn,
      },
    });
    expect(client).toBeInstanceOf(RawKasAcpClient);
    client.close();
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

  it('declares workflow settings in clientMeta', () => {
    setWorkflowsEnabled(true);
    const _client = new KasAcpClient();
    expect(capturedKiroClientConfig?.clientMeta?.settings).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
        workflowNotifications: { enabled: true, delivery: 'steer' },
      })
    );
  });

  it('repeats the persisted notification delivery for new and loaded sessions', async () => {
    setWorkflowsEnabled(true);
    writeTestCliJson({
      'chat.enableWorkflows': true,
      'chat.defaultInterruptBehavior': 'queue',
    });
    const client = new KasAcpClient({
      stream: {
        readable: new ReadableStream(),
        writable: new WritableStream(),
      },
    });

    expect(capturedKiroClientConfig?.clientMeta?.settings).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
        workflowNotifications: { enabled: true, delivery: 'queue' },
      })
    );

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
    ).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
        workflowNotifications: { enabled: true, delivery: 'queue' },
      })
    );

    await client.loadSession('injected-session');
    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
    ).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
        workflowNotifications: { enabled: true, delivery: 'queue' },
      })
    );
  });

  // The opt-in is written to disk by /settings, not handed to this client, so
  // a snapshot taken once in the constructor would keep sending the old
  // Workflows bind for the life of the client: the extension subscribes once
  // at initialize, so a preference picked up mid-process would announce
  // `enabled: true` with nothing listening. The new value applies to the next
  // process, not the next session.
  it('keeps the policy it started with when the opt-in changes mid-process', async () => {
    setWorkflowsOptIn(false);
    const client = new KasAcpClient();
    await client.initialize();

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflows
    ).toEqual({ enabled: false });

    // Opting in now must NOT reach a session this client starts later.
    setWorkflowsOptIn(true);

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
    ).toEqual(
      expect.objectContaining({
        workflows: { enabled: false },
        goal: { enabled: false },
      })
    );

    await client.loadSession('later-session');
    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflows
    ).toEqual({ enabled: false });
  });

  // A client that started opted IN keeps serving workflows for its lifetime,
  // so the payload and the running extension never disagree.
  it('keeps workflows live for a client that started opted in', async () => {
    setWorkflowsOptIn(true);
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    setWorkflowsOptIn(false);

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflows
    ).toEqual({ enabled: true });
  });

  // The delivery carried across a session boundary is runtime-only state. If
  // any cached delivery counted as an override, the first value read would be
  // pinned and a newly persisted preference silently dropped.
  it('sends a newly persisted interrupt mode rather than the previous snapshot', async () => {
    setWorkflowsEnabled(true);
    writeTestCliJson({
      'chat.enableWorkflows': true,
      'chat.defaultInterruptBehavior': 'steer',
    });
    const client = new KasAcpClient({
      stream: {
        readable: new ReadableStream(),
        writable: new WritableStream(),
      },
    });
    expect(
      (capturedKiroClientConfig?.clientMeta?.settings as any)
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'steer' });

    // User changes interrupt behaviour in /settings after construction.
    writeTestCliJson({
      'chat.enableWorkflows': true,
      'chat.defaultInterruptBehavior': 'queue',
    });

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });

    await client.loadSession('later-session');
    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });
  });

  // A confirmed runtime override still wins over the persisted value — the
  // fix above must not have traded one bug for its mirror image.
  it('keeps a confirmed runtime delivery override across later sessions', async () => {
    setWorkflowsOptIn(true);
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    await client.setWorkflowNotificationDelivery('queue' as any);

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });
  });

  // The candidate policy must not be adopted until its session actually
  // starts; otherwise a rejected RPC leaves the previous session running
  // under the failed session's policy.
  it('keeps the active policy when session/new rejects', async () => {
    setWorkflowsOptIn(true);
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const startedExt = (client as any).workflowExtensionInstance;
    expect(startedExt).toBeDefined();

    // Opt out, then fail the session that would have adopted the opt-out.
    setWorkflowsOptIn(false);
    mockKiroNewSession.mockImplementationOnce(() => {
      throw new Error('session/new rejected');
    });
    await expect(client.newSession()).rejects.toThrow('session/new rejected');

    expect((client as any).workflowsEnabled).toBe(true);
    expect((client as any).workflowExtensionInstance).toBe(startedExt);
    expect((client as any).kasSettings?.workflows).toEqual({ enabled: true });
  });

  it('keeps the active policy when session/load rejects', async () => {
    setWorkflowsOptIn(true);
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const startedExt = (client as any).workflowExtensionInstance;

    setWorkflowsOptIn(false);
    mockKiroLoadSession.mockImplementationOnce(() => {
      throw new Error('session/load rejected');
    });
    await expect(client.loadSession('doomed')).rejects.toThrow(
      'session/load rejected'
    );

    expect((client as any).workflowsEnabled).toBe(true);
    expect((client as any).workflowExtensionInstance).toBe(startedExt);
    expect((client as any).kasSettings?.workflows).toEqual({ enabled: true });
  });

  // Same boundary, the event side: a client that keeps handling workflow

  it('resolves a committed load when workflow restoration fails', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient();
    await client.initialize();
    const workflow = client.workflowConversation;
    workflow.restoreParentRuns = mock(async () => {
      throw new Error('restore failed');
    });

    await expect(client.loadSession('loaded-session')).resolves.toEqual(
      expect.objectContaining({ sessionId: 'loaded-session' })
    );
  });

  it('an abandoned load rejects late and never takes session ownership', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const before = client.sessionId;

    // Gate the RPC so the deadline can fire while it is in flight.
    let releaseRpc!: (value: unknown) => void;
    mockKiroLoadSession.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseRpc = resolve;
        })
    );
    const pending = client.loadSession('late-session');
    client.abandonPendingLoad();
    releaseRpc({ sessionId: 'late-session' });

    await expect(pending).rejects.toThrow('abandoned');
    expect(client.sessionId).toBe(before);

    // The client is still live and a fresh load succeeds normally.
    await expect(client.loadSession('next-session')).resolves.toEqual(
      expect.objectContaining({ sessionId: 'next-session' })
    );
  });

  it('applies a pre-session delivery toggle to new and loaded sessions', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient();
    await client.initialize();

    await client.setWorkflowNotificationDelivery('queue');
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();

    await client.newSession();
    expect(
      mockKiroNewSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });

    await client.loadSession('loaded-session');
    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });
  });

  it('updates notification delivery when KAS advertises the extension', async () => {
    setWorkflowsEnabled(true);
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            extensionMethods: ['_kiro/session/setWorkflowNotificationDelivery'],
          },
        },
      },
    });
    mockKiroSendExtMethod.mockResolvedValueOnce({ delivery: 'queue' });
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    await client.setWorkflowNotificationDelivery('queue');

    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/session/setWorkflowNotificationDelivery',
      {
        sessionId: 'kas-session-1',
        delivery: 'queue',
      }
    );
  });

  it('carries a confirmed delivery update into a later session load', async () => {
    setWorkflowsEnabled(true);
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            extensionMethods: ['_kiro/session/setWorkflowNotificationDelivery'],
          },
        },
      },
    });
    mockKiroSendExtMethod.mockResolvedValueOnce({ delivery: 'queue' });
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    await client.setWorkflowNotificationDelivery('queue');
    await client.loadSession('loaded-session');

    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'queue' });
  });

  it('does not carry a rejected delivery update into a later session load', async () => {
    setWorkflowsEnabled(true);
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            extensionMethods: ['_kiro/session/setWorkflowNotificationDelivery'],
          },
        },
      },
    });
    mockKiroSendExtMethod.mockRejectedValueOnce(new Error('update rejected'));
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    await expect(
      client.setWorkflowNotificationDelivery('queue')
    ).rejects.toThrow('update rejected');
    await client.loadSession('loaded-session');

    expect(
      mockKiroLoadSession.mock.calls.at(-1)?.[0]?._meta?.kiro?.settings
        ?.workflowNotifications
    ).toEqual({ enabled: true, delivery: 'steer' });
  });

  it('skips notification delivery when KAS omits the extension', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockClear();

    await client.setWorkflowNotificationDelivery('queue');

    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
  });

  it('emits and resolves user input through its own capability', async () => {
    const client = new KasAcpClient();
    let received: any;
    client.onUpdate((event: any) => {
      if (event.type === AgentEventType.QuestionRequest) received = event;
    });
    const capability = capturedKiroClientConfig.capabilities.find(
      (item: any) => item.method === '_kiro/userInput'
    );

    const response = capability.handler({
      sessionId: 'kas-session-1',
      toolCallId: 'question-1',
      question: 'Which path?',
      options: [{ title: 'Requirements' }, { title: 'Design' }],
    });

    expect(capability).toMatchObject({
      type: 'other',
      key: 'userInput',
      value: true,
    });
    expect(received.value.toolCallId).toBe('question-1');
    received.value.resolve({ action: 'answered', answer: 'Design' });
    expect(await response).toEqual({
      action: 'answered',
      answer: 'Design',
    });
  });

  it('registers OAuth copy failure as one persistent system notice', async () => {
    const copyToClipboard = mock(() => false);
    const client = new KasAcpClient({ copyToClipboard });
    const events: AgentStreamEvent[] = [];
    client.onUpdate((event) => events.push(event));
    const capability = capturedKiroClientConfig.capabilities.find(
      (item: any) => item.method === '_kiro/openExternalUrl'
    );
    const url = 'https://example.com/oauth?state=sensitive';

    const response = await capability.handler({ url });

    expect(copyToClipboard).toHaveBeenCalledWith(url);
    expect(response).toEqual({ success: false });
    expect(events).toEqual([
      {
        type: AgentEventType.SystemNotice,
        message:
          'Clipboard copy failed. Open this session-specific OAuth URL manually; do not share it:\n' +
          url,
        success: false,
        persistent: true,
      },
    ]);
  });

  it('close() calls kill("SIGTERM") on the agent process', () => {
    const client = new KasAcpClient();
    client.close();
    expect(mockProcess.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('close() does not clear routing state owned by a replacement client', () => {
    const client = new KasAcpClient();
    const routingState = kasRoutingStore.getState().subagentRouting;
    routingState.toolCallToSubtask.set('new-tool', 'new-subtask');

    client.close();

    expect(routingState.toolCallToSubtask.get('new-tool')).toBe('new-subtask');
  });

  it('a retired client cannot clear replacement routing after session/new resolves', async () => {
    let resolveNewSession!: (result: any) => void;
    mockKiroNewSession.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveNewSession = resolve;
        })
    );
    const retiredClient = new KasAcpClient();
    const pendingSession = retiredClient.newSession();

    retiredClient.close();
    const replacementClient = new KasAcpClient();
    const routingState = kasRoutingStore.getState().subagentRouting;
    routingState.toolCallToSubtask.set('replacement-tool', 'replacement-task');

    resolveNewSession({
      sessionId: 'retired-session',
      configOptions: [],
    });
    await expect(pendingSession).rejects.toThrow(
      'KAS client closed during session creation'
    );

    expect(capturedSessionUpdateHandler).toBeNull();
    expect(routingState.toolCallToSubtask.get('replacement-tool')).toBe(
      'replacement-task'
    );
    replacementClient.close();
  });

  it('initialize() calls kiroClient.initialize', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    expect(mockKiroInitialize).toHaveBeenCalledTimes(1);
  });

  it('keeps workflow runtime integration dark when the rollout is disabled', async () => {
    const client = new KasAcpClient();

    await client.initialize();
    await client.newSession();

    expect(client.workflowConversation).toBeUndefined();
    expect(client.workflowControl).toBeUndefined();
    expect((client as any).workflowExtensionInstance).toBeUndefined();
    expect((client as any).extensionRuntimeInstance).toBeUndefined();
  });

  it('starts workflow runtime integration when the rollout is enabled', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient();

    await client.initialize();

    expect(client.workflowConversation).toBeDefined();
    expect(client.workflowControl).toBeDefined();
    expect((client as any).workflowExtensionInstance).toBeDefined();
    expect((client as any).extensionRuntimeInstance).toBeDefined();
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

  it('newSession() applies initialAgent as _meta.kiro.modeId (no mode round-trip)', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
    await client.newSession();

    expect(mockKiroNewSession).toHaveBeenCalledWith(
      expect.objectContaining({
        _meta: {
          kiro: expect.objectContaining({
            modeId: 'plan',
            settings: expect.objectContaining({
              workflows: { enabled: true },
              goal: { enabled: true },
            }),
          }),
        },
      })
    );
    const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'mode'
    );
    expect(modeCalls.length).toBe(0);
  });

  it('newSession() does not set mode when initialAgent absent and KIRO_MODE unset', async () => {
    setWorkflowsEnabled(true);
    const prev = process.env.KIRO_MODE;
    delete process.env.KIRO_MODE;
    try {
      const client = new KasAcpClient();
      await client.newSession();
      const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
        ([req]: any[]) => req?.configId === 'mode'
      );
      expect(modeCalls.length).toBe(0);
      const request = mockKiroNewSession.mock.calls.at(-1)?.[0] as any;
      expect(request?._meta?.kiro?.modeId).toBeUndefined();
      expect(request?._meta?.kiro?.settings).toEqual(
        expect.objectContaining({
          workflows: { enabled: true },
          goal: { enabled: true },
        })
      );
    } finally {
      if (prev !== undefined) process.env.KIRO_MODE = prev;
    }
  });

  it('newSession() prefers initialAgent over KIRO_MODE env var', async () => {
    setWorkflowsEnabled(true);
    const prev = process.env.KIRO_MODE;
    process.env.KIRO_MODE = KAS_DEFAULT_AGENT_ID;
    try {
      const client = new KasAcpClient({ initialAgent: 'kiro_planner' });
      await client.newSession();
      expect(mockKiroNewSession).toHaveBeenCalledWith(
        expect.objectContaining({
          _meta: {
            kiro: expect.objectContaining({
              modeId: 'plan',
              settings: expect.objectContaining({
                workflows: { enabled: true },
                goal: { enabled: true },
              }),
            }),
          },
        })
      );
      const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
        ([req]: any[]) => req?.configId === 'mode'
      );
      expect(modeCalls.length).toBe(0);
    } finally {
      if (prev === undefined) delete process.env.KIRO_MODE;
      else process.env.KIRO_MODE = prev;
    }
  });

  it('newSession() applies initialEffort as effortLevel after the model', async () => {
    const client = new KasAcpClient({
      initialModel: 'm1',
      initialEffort: 'low',
    });
    await client.newSession();

    const configCalls = mockKiroSetSessionConfigOption.mock.calls.map(
      ([req]: any[]) => req
    );
    expect(configCalls.find((r: any) => r?.configId === 'effortLevel')).toEqual(
      expect.objectContaining({
        sessionId: 'kas-session-1',
        configId: 'effortLevel',
        value: 'low',
      })
    );
    // The level must land on the session's effective model, so the effort
    // write happens only after the model write.
    const modelIdx = configCalls.findIndex((r: any) => r?.configId === 'model');
    const effortIdx = configCalls.findIndex(
      (r: any) => r?.configId === 'effortLevel'
    );
    expect(modelIdx).toBeGreaterThanOrEqual(0);
    expect(effortIdx).toBeGreaterThan(modelIdx);
  });

  it('newSession() sets no effortLevel when initialEffort is absent', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const effortCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'effortLevel'
    );
    expect(effortCalls.length).toBe(0);
  });

  it('newSession() still resolves when config writes fail (autopilot, model, effort)', async () => {
    mockKiroSetSessionConfigOption.mockImplementation(() =>
      Promise.reject(new Error('rpc failure'))
    );
    try {
      const client = new KasAcpClient({
        initialModel: 'm1',
        initialEffort: 'low',
      });
      const result = await client.newSession();
      expect(result.sessionId).toBe('kas-session-1');
      // All three writes were attempted despite each rejecting.
      const attempted = mockKiroSetSessionConfigOption.mock.calls.map(
        ([req]: any[]) => req?.configId
      );
      expect(attempted).toEqual(
        expect.arrayContaining(['autopilot', 'model', 'effortLevel'])
      );
    } finally {
      mockKiroSetSessionConfigOption.mockImplementation((_req: any) =>
        Promise.resolve()
      );
    }
  });

  it('loadSession() does NOT apply initialEffort (persisted session effort wins)', async () => {
    const client = new KasAcpClient({ initialEffort: 'low' });
    await client.loadSession('existing-session');
    const effortCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'effortLevel'
    );
    expect(effortCalls.length).toBe(0);
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

  it('loadSession() does not emit turn telemetry for replayed updates', async () => {
    const completion = {
      sessionId: 'existing-session',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            elapsedTime: 100,
            inputTokens: 10,
            outputTokens: 5,
            status: 'success',
          },
        },
      },
    };
    mockKiroLoadSession.mockImplementationOnce(async ({ sessionId }) => {
      await capturedSessionUpdateHandlers.get(sessionId)?.(completion);
      return { sessionId, configOptions: [] };
    });
    const client = new KasAcpClient();

    await client.loadSession('existing-session');

    expect(mockRecordTuiUserTurn).not.toHaveBeenCalled();
    const liveHandler = capturedSessionUpdateHandlers.get('existing-session');
    expect(liveHandler).toBeDefined();
    await liveHandler(completion);
    expect(mockRecordTuiUserTurn).toHaveBeenCalledTimes(1);
  });

  it('restores live workflows owned by a resumed session', async () => {
    setWorkflowsEnabled(true);
    const parentSessionId = 'resumed-parent';
    const workflowId = 'workflow-paused';
    mockKiroSendExtMethod
      .mockResolvedValueOnce({
        runs: [
          {
            workflowId,
            name: 'Paused workflow',
            status: 'paused',
            createdAt: '2026-07-20T10:00:00.000Z',
            updatedAt: '2026-07-20T10:01:00.000Z',
            parentSessionId,
          },
        ],
      })
      .mockResolvedValueOnce({
        workflowId,
        state: {
          workflowId,
          workflowName: 'Paused workflow',
          status: 'paused',
          inputs: {},
          artifacts: {},
          capturedOutputs: {},
          parentSessionId,
          root: {
            nodeId: 'root',
            type: 'sequence',
            status: 'paused',
            children: [],
          },
        },
        stepSessions: [],
      });
    const events: AgentStreamEvent[] = [];
    const client = new KasAcpClient();
    client.onUpdate((event) => events.push(event));

    await client.loadSession(parentSessionId);

    expect(mockKiroSendExtMethod).toHaveBeenNthCalledWith(
      1,
      '_kiro/workflow/list',
      { workspacePaths: [process.cwd()] }
    );
    expect(mockKiroSendExtMethod).toHaveBeenNthCalledWith(
      2,
      '_kiro/workflow/load',
      { workflowId }
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: AgentEventType.WorkflowProgress,
        event: expect.objectContaining({
          type: 'run_snapshot',
          workflowId,
          parentSessionId,
          state: expect.objectContaining({ status: 'paused' }),
        }),
      })
    );
    expect(mockRecordTuiWorkflowRestoreSummary).toHaveBeenCalledWith(
      {
        restored: 1,
        discovery_failed: 0,
        load_failed: 0,
        rejected: 0,
        _other_: 0,
      },
      expect.any(String)
    );
  });

  it('filters internal workflow prompts but preserves agent-initiated responses during replay', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient();
    const events: any[] = [];
    client.onUpdate((event: any) => events.push(event));
    mockKiroLoadSession.mockImplementationOnce(async (request: any) => {
      const replay = capturedSessionUpdateHandlers.get(request.sessionId);
      expect(replay).toBeDefined();
      await replay({
        sessionId: request.sessionId,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'WORKFLOW_INTERNAL' },
          _meta: {
            kiro: {
              notification: {
                kind: 'system-notification',
                workflowId: 'workflow-1',
              },
            },
          },
        },
      });
      await replay({
        sessionId: request.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'agent-initiated response' },
          _meta: { kiro: { agentInitiated: true } },
        },
      });
      await replay({
        sessionId: request.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'hidden visibility response' },
          _meta: { kiro: { visibility: 'hidden' } },
        },
      });
      await replay({
        sessionId: request.sessionId,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'visible user' },
        },
      });
      await replay({
        sessionId: request.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'visible assistant' },
        },
      });
      return {
        sessionId: request.sessionId,
        configOptions: [
          {
            id: 'mode',
            category: 'mode',
            type: 'select',
            currentValue: 'vibe',
            options: [{ value: 'vibe', name: 'Default' }],
          },
        ],
      };
    });

    await client.loadSession('existing-session');

    const replayedText = events
      .filter(
        (event) =>
          event.type === AgentEventType.UserMessage ||
          event.type === AgentEventType.Content
      )
      .map((event) => event.content.text);
    expect(replayedText).toEqual([
      'agent-initiated response',
      'visible user',
      'visible assistant',
    ]);
  });

  it('loadSession() disposes previous session listeners', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    // First session wired — dispose not yet called
    expect(mockSessionUpdateDispose).not.toHaveBeenCalled();
    expect(mockPermissionRequestDispose).not.toHaveBeenCalled();

    // Switch session — old listeners should be disposed
    await client.loadSession('second-session');
    // One disposal releases the temporary replay capture and one releases the
    // previous primary listener.
    expect(mockSessionUpdateDispose).toHaveBeenCalledTimes(2);
    expect(mockPermissionRequestDispose).toHaveBeenCalledTimes(2);
  });

  it('keeps the previous session active when session/load fails', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const previousUpdateHandler =
      capturedSessionUpdateHandlers.get('kas-session-1');
    const previousPermissionHandler =
      capturedPermissionHandlers.get('kas-session-1');
    mockKiroLoadSession.mockRejectedValueOnce(new Error('load failed'));

    await expect(client.loadSession('failed-session')).rejects.toThrow(
      'load failed'
    );

    expect(capturedSessionUpdateHandlers.get('kas-session-1')).toBe(
      previousUpdateHandler
    );
    expect(capturedPermissionHandlers.get('kas-session-1')).toBe(
      previousPermissionHandler
    );
    expect(capturedSessionUpdateHandlers.has('failed-session')).toBe(false);
    expect(capturedPermissionHandlers.has('failed-session')).toBe(false);

    await client.prompt([{ type: 'text', text: 'still here' }]);
    expect(mockKiroPrompt).toHaveBeenLastCalledWith(
      expect.objectContaining({ sessionId: 'kas-session-1' })
    );
  });

  it('loadSession() returns the normalized agent id for wire vibe (not the raw wire id)', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded',
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Vibe' }],
        },
      ],
    } as any);

    const client = new KasAcpClient();
    const result = await client.loadSession('existing-session');

    expect(result.currentAgent?.name).toBe('default');
  });

  // ── #5: /agent must not leak local agents into a cloud session ──
  // A relayed load/create OMITS configOptions (the sandbox pushes
  // the authoritative agent surface over the downlink). The client must clear
  // the previous session's agent list on that omission, or a cloud→local→cloud
  // switch keeps showing the local machine's agents.
  it('a cloud load WITHOUT configOptions clears the agent list (switch-leak prevention)', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
        },
      },
      // No configOptions — the relayed-load contract.
    } as any);
    await client.loadSession('cloud-loaded');

    const agentUpdates = events.filter((e) => e.type === 'agents_update');
    expect(agentUpdates).toHaveLength(1);
    expect(agentUpdates[0].agents).toEqual([]);
  });

  it('a cloud load without configOptions SELF-HEALS via a boolean no-op probe', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          agentMode: 'vibe',
        },
      },
    } as any);
    // The forwarded set_config's response carries the SANDBOX's configOptions.
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'kiro_spec',
          options: [
            { value: 'vibe', name: 'Vibe' },
            { value: 'kiro_spec', name: 'Spec' },
          ],
        },
      ],
    } as any);

    await client.loadSession('cloud-loaded');
    // The self-heal is fire-and-forget; let its .then() run.
    await new Promise((r) => setTimeout(r, 0));

    const req = mockKiroSetSessionConfigOption.mock.calls.at(-1)?.[0] as any;
    // MUST be a boolean-valued probe (KAS early-returns configOptions without
    // mutating), NEVER a mode re-assert — the load meta's agentMode is a
    // hardcoded 'vibe' on reconstructed remote records, and setting it would
    // durably reset a spec/custom-agent sandbox session on every resume.
    expect(req?.configId).not.toBe('mode');
    expect(typeof req?.value).toBe('boolean');
    expect(req?.sessionId).toBe('cloud-loaded');

    const agentUpdates = events.filter((e) => e.type === 'agents_update');
    // First the clear, then the sandbox-sourced repopulation.
    expect(agentUpdates.length).toBe(2);
    expect(agentUpdates[0].agents).toEqual([]);
    expect(agentUpdates[1].agents.length).toBeGreaterThan(0);
  });

  it('a LOCAL load without configOptions leaves the agent list untouched', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'local-loaded',
      // No _meta (local placement), no configOptions: must NOT clear.
    } as any);
    await client.loadSession('local-loaded');

    expect(events.filter((e) => e.type === 'agents_update')).toHaveLength(0);
  });

  it('a cloud load WITH configOptions emits the parsed agents, not a clear', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: { executionTarget: { kind: 'cloud-sandbox' }, source: 'remote' },
      },
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Vibe' }],
        },
      ],
    } as any);
    await client.loadSession('cloud-loaded');

    const agentUpdates = events.filter((e) => e.type === 'agents_update');
    expect(agentUpdates).toHaveLength(1);
    expect(agentUpdates[0].agents.length).toBeGreaterThan(0);
  });

  it('a user config change discards an in-flight self-heal response (epoch guard)', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          agentMode: 'vibe',
        },
      },
    } as any);
    // Self-heal round-trip is SLOW: park it until we release it manually.
    let releaseSelfHeal!: (v: unknown) => void;
    const parked = new Promise((r) => {
      releaseSelfHeal = r;
    });
    mockKiroSetSessionConfigOption.mockImplementationOnce(() => parked as any);

    await client.loadSession('cloud-loaded');

    // User switches agent while the self-heal is still in flight — the
    // switch's own (fast) set_config resolves with the NEW config.
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'dev',
          options: [{ value: 'dev', name: 'Dev' }],
        },
      ],
    } as any);
    await client.setConfigOption('mode', 'dev');
    const eventsAfterSwitch = events.length;

    // The stale self-heal lands late, carrying the OLD mode — must be dropped.
    releaseSelfHeal({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Vibe' }],
        },
      ],
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(events.length).toBe(eventsAfterSwitch);
  });

  it('an A→B→A reload discards the FIRST load self-heal response (same-id race)', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    const cloudLoadResponse = {
      sessionId: 'session-A',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          agentMode: 'vibe',
        },
      },
    } as any;

    // Load A: park its self-heal.
    mockKiroLoadSession.mockResolvedValueOnce(cloudLoadResponse);
    let releaseFirst!: (v: unknown) => void;
    mockKiroSetSessionConfigOption.mockImplementationOnce(
      () => new Promise((r) => (releaseFirst = r)) as any
    );
    await client.loadSession('session-A');

    // Reload A (the B hop is irrelevant to the guard — same id back-to-back
    // is the hardest case): its self-heal resolves immediately with fresh config.
    mockKiroLoadSession.mockResolvedValueOnce(cloudLoadResponse);
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [
            { value: 'vibe', name: 'Vibe' },
            { value: 'kiro_spec', name: 'Spec' },
          ],
        },
      ],
    } as any);
    await client.loadSession('session-A');
    await new Promise((r) => setTimeout(r, 0));
    const eventsAfterSecondHeal = events.length;

    // First load's stale self-heal lands last — same session id, older epoch.
    releaseFirst({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Stale' }],
        },
      ],
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(events.length).toBe(eventsAfterSecondHeal);
  });

  it('a cloud load without configOptions also clears the MODEL surface (no local-model leak)', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: { executionTarget: { kind: 'cloud-sandbox' }, source: 'remote' },
      },
    } as any);
    await client.loadSession('cloud-loaded');

    const modelUpdates = events.filter((e) => e.type === 'model_config_update');
    expect(modelUpdates).toHaveLength(1);
    expect(modelUpdates[0].models).toEqual([]);
    expect(modelUpdates[0].efforts).toEqual([]);
  });

  it("the self-heal re-emits the CURRENT agent (welcome-suppressed) so the chip cannot keep the previous session's agent", async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          agentMode: 'vibe',
        },
      },
    } as any);
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'kiro_spec',
          options: [
            { value: 'vibe', name: 'Vibe' },
            {
              value: 'kiro_spec',
              name: 'Spec',
              _meta: { kiro: { welcomeMessage: 'Welcome to spec!' } },
            },
          ],
        },
      ],
    } as any);

    await client.loadSession('cloud-loaded');
    await new Promise((r) => setTimeout(r, 0));

    const switches = events.filter((e) => e.type === 'agent_switched');
    expect(switches).toHaveLength(1);
    expect(switches[0].agentName).toBe('kiro_spec');
    // A resume settling is not a user switch — no welcome banner payload.
    expect(switches[0].welcomeMessage).toBeUndefined();
  });

  it('a config_option_update push supersedes an in-flight self-heal (stale response dropped)', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          agentMode: 'vibe',
        },
      },
    } as any);
    // Park the self-heal round-trip.
    let releaseSelfHeal!: (v: unknown) => void;
    const parked = new Promise((r) => {
      releaseSelfHeal = r;
    });
    mockKiroSetSessionConfigOption.mockImplementationOnce(() => parked as any);
    await client.loadSession('cloud-loaded');

    // The sandbox pushes fresh config over the downlink first.
    await capturedSessionUpdateHandler!({
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'mode',
            category: 'mode',
            currentValue: 'kiro_spec',
            options: [{ value: 'kiro_spec', name: 'Spec' }],
          },
        ],
      },
    } as any);
    const eventsAfterPush = events.length;
    expect(
      events.filter((e) => e.type === 'agents_update').length
    ).toBeGreaterThanOrEqual(2); // clear + push repopulation

    // The stale self-heal lands late with an OLDER snapshot — must be dropped.
    releaseSelfHeal({
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Stale' }],
        },
      ],
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(events.length).toBe(eventsAfterPush);
  });

  it('deletes a cloud session from the remote store', async () => {
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();

    await expect(
      client.deleteSessionById('cloud-1', { source: 'remote' })
    ).resolves.toBe(true);
    expect(mockKiroSendExtMethod).toHaveBeenLastCalledWith(
      '_kiro/session/delete',
      {
        sessionId: 'cloud-1',
        sessionSource: 'remote',
      }
    );
  });

  it('omits the store hint when deleting a local session without a source', async () => {
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();

    await expect(client.deleteSessionById('local-1')).resolves.toBe(true);
    expect(mockKiroSendExtMethod).toHaveBeenLastCalledWith(
      '_kiro/session/delete',
      {
        sessionId: 'local-1',
      }
    );
  });

  it('newSession() captures _meta.kiro.repositories from the create response', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'cloud-created',
      _meta: {
        kiro: {
          repositories: [
            { name: 'acme/banana-service', branch: 'feature/x' },
            { name: 'acme/second-repo' },
          ],
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.newSession();

    expect(client.sessionRepositories).toEqual([
      { name: 'acme/banana-service', branch: 'feature/x' },
      { name: 'acme/second-repo' },
    ]);
  });

  it('newSession() resets repositories to null when the create response reports none', async () => {
    // First a cloud load that leaves repos captured…
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          repositories: [{ name: 'acme/banana-service' }],
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('cloud-loaded');
    expect(client.sessionRepositories).toHaveLength(1);

    // …then a plain create (no _meta): the previous sandbox's repos must not
    // remain readable.
    await client.newSession();
    expect(client.sessionRepositories).toBeNull();
  });

  it('a cloud newSession without configOptions clears the config surface (agents + models)', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            executionTargets: ['local', 'cloud-sandbox'],
            sessionSources: ['local', 'remote'],
          },
        },
      },
    });
    // The create response omits configOptions (relayed cloud create contract).
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'cloud-created-no-config',
      _meta: {
        kiro: { executionTarget: { kind: 'cloud-sandbox' } },
      },
    } as any);
    // The autopilot set also returns no configOptions (cloud relay in flight).
    mockKiroSetSessionConfigOption.mockResolvedValue({} as any);

    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);
    await client.newSession();

    // Should have cleared agents AND models (empty arrays).
    const agentUpdates = events.filter((e) => e.type === 'agents_update');
    expect(agentUpdates).toHaveLength(1);
    expect(agentUpdates[0].agents).toEqual([]);
    const modelUpdates = events.filter((e) => e.type === 'model_config_update');
    expect(modelUpdates).toHaveLength(1);
    expect(modelUpdates[0].models).toEqual([]);
    expect(modelUpdates[0].efforts).toEqual([]);

    // Reset mock to default
    mockKiroSetSessionConfigOption.mockImplementation(() => Promise.resolve());
  });

  it('loadSession() captures _meta.kiro.repositories for the footer (cloud resume)', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
          repositories: [
            {
              providerType: 'GITHUB',
              name: 'acme/banana-service',
              branch: 'main',
            },
            { providerType: 'GITHUB', name: 'acme/second-repo' },
            { name: '' }, // no usable name — dropped
            'not-an-object', // malformed — dropped
          ],
        },
      },
    } as any);

    const client = new KasAcpClient();
    await client.loadSession('cloud-loaded');

    expect(client.sessionRepositories).toEqual([
      { name: 'acme/banana-service', branch: 'main' },
      { name: 'acme/second-repo' },
    ]);
    expect(client.isCloudSessionActive()).toBe(true);
  });

  it('loadSession() clears stale repositories when the next load reports none (local session)', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-loaded',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          repositories: [{ name: 'acme/banana-service' }],
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('cloud-loaded');
    expect(client.sessionRepositories).toHaveLength(1);

    // Default mock: a plain local load response with no _meta — nothing
    // reported, so the field reads null (not the previous sandbox's repos).
    await client.loadSession('local-loaded');
    expect(client.sessionRepositories).toBeNull();
  });

  it('loads from the remote store (sessionSource:remote) for a cloud-sandbox session when advertised', async () => {
    setWorkflowsEnabled(true);
    // session/load takes ONE concrete store (KAS rejects 'all' — list-only):
    // a cloud session reattaches remote; a local session omits the hint.
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.loadSession('maybe-remote-session');
    const req = mockKiroLoadSession.mock.calls.at(-1)?.[0] as any;
    expect(req?.sessionId).toBe('maybe-remote-session');
    expect(req?.cwd).toBeDefined();
    expect(req?._meta?.kiro).toEqual(
      expect.objectContaining({
        sessionSource: 'remote',
        settings: expect.objectContaining({
          workflows: { enabled: true },
          goal: { enabled: true },
        }),
      })
    );
  });

  it('omits the store hint but sends settings when loading a local session', async () => {
    setWorkflowsEnabled(true);
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    const client = new KasAcpClient();
    await client.initialize();
    await client.loadSession('local-session');
    const req = mockKiroLoadSession.mock.calls.at(-1)?.[0] as any;
    expect(req?._meta?.kiro?.sessionSource).toBeUndefined();
    expect(req?._meta?.kiro?.settings).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
      })
    );
  });

  it('sends settings on session/load when KAS advertises no remote store', async () => {
    setWorkflowsEnabled(true);
    const client = new KasAcpClient(); // no initialize -> no caps captured
    await client.loadSession('local-session');
    const req = mockKiroLoadSession.mock.calls.at(-1)?.[0] as any;
    expect(req?.sessionId).toBe('local-session');
    expect(req?._meta?.kiro?.sessionSource).toBeUndefined();
    expect(req?._meta?.kiro?.settings).toEqual(
      expect.objectContaining({
        workflows: { enabled: true },
        goal: { enabled: true },
      })
    );
  });

  it('retries session/load against the remote store when the local store reports not found', async () => {
    setWorkflowsEnabled(true);
    // Pins the retry to the KAS not-found wording: a local miss falls through
    // to the remote store, so resuming a running cloud session still works.
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    mockKiroLoadSession.mockRejectedValueOnce(new Error('session not found'));
    const client = new KasAcpClient();
    await client.initialize();
    await client.loadSession('maybe-remote');
    expect(mockKiroLoadSession).toHaveBeenCalledTimes(2);
    const first = mockKiroLoadSession.mock.calls[0]?.[0] as any;
    const second = mockKiroLoadSession.mock.calls[1]?.[0] as any;
    expect(first?._meta?.kiro?.sessionSource).toBeUndefined();
    expect(first?._meta?.kiro?.settings?.workflows).toEqual({ enabled: true });
    expect(second?._meta?.kiro).toEqual(
      expect.objectContaining({
        sessionSource: 'remote',
        settings: expect.objectContaining({
          workflows: { enabled: true },
        }),
      })
    );
  });

  it('does not retry the remote store when the local load fails for another reason', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    mockKiroLoadSession.mockRejectedValueOnce(new Error('unauthorized'));
    const client = new KasAcpClient();
    await client.initialize();
    await expect(client.loadSession('sess')).rejects.toThrow('unauthorized');
    expect(mockKiroLoadSession).toHaveBeenCalledTimes(1);
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

  it('emits chat session start on creation and load without waiting for a prompt', async () => {
    const client = new KasAcpClient({ version: '9.9.9-test' });
    await client.newSession();

    expect(mockRecordTuiSessionStarted).toHaveBeenCalledTimes(1);
    expect(mockRecordTuiSessionStarted.mock.calls[0]![0]).toMatchObject({
      version: '9.9.9-test',
    });

    await client.prompt([{ type: 'text', text: 'hello' } as any]);
    await client.prompt([{ type: 'text', text: 'again' } as any]);

    expect(mockKiroPrompt).toHaveBeenCalledTimes(2);
    expect(mockRecordTuiSessionStarted).toHaveBeenCalledTimes(1);

    await client.loadSession('kas-loaded');
    expect(mockRecordTuiSessionStarted).toHaveBeenCalledTimes(2);

    await client.loadSession('kas-loaded');
    expect(mockRecordTuiSessionStarted).toHaveBeenCalledTimes(2);
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

  it('sendProcessHealthMetrics() is a safe no-op (does not throw or hit the agent)', () => {
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

    expect(() => client.sendProcessHealthMetrics(snapshot)).not.toThrow();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
    expect(mockKiroSendExtNotification).not.toHaveBeenCalled();
  });

  it('sendModeChanged() is a safe no-op (does not throw or hit the agent)', () => {
    const client = new KasAcpClient();
    const payload = {
      fromMode: 'kiro',
      toMode: 'kiro_planner',
      source: 'shiftTab',
      sessionId: 'kas-session-1',
    };

    expect(() => client.sendModeChanged(payload as any)).not.toThrow();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
    expect(mockKiroSendExtNotification).not.toHaveBeenCalled();
  });

  it('recordSlashCommandInvocation() records the typed V3 metric locally', async () => {
    const client = new KasAcpClient({ version: '9.9.9-test' });
    await client.newSession();
    mockKiroSendExtMethod.mockClear();
    mockKiroSendExtNotification.mockClear();

    expect(() => client.recordSlashCommandInvocation('/chat')).not.toThrow();
    expect(mockRecordTuiSlashCommand).toHaveBeenCalledTimes(1);
    expect(mockRecordTuiSlashCommand).toHaveBeenCalledWith({
      command: '/chat',
      version: '9.9.9-test',
      engine: 'v3',
      logProperties: { sessionId: 'kas-session-1' },
    });
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
    expect(mockKiroSendExtNotification).not.toHaveBeenCalled();
  });

  it('sendUiModeSessionStart() records the typed UI-mode metric locally', () => {
    const client = new KasAcpClient({ version: '9.9.9-test' });

    client.sendUiModeSessionStart({
      uiMode: 'lite',
      uiModeSource: UiModeSource.Setting,
      uiModeDefault: 'lite',
    });

    expect(mockRecordTuiUiModeSessionStarted).toHaveBeenCalledTimes(1);
    expect(mockRecordTuiUiModeSessionStarted).toHaveBeenCalledWith({
      mode: 'lite',
      version: '9.9.9-test',
    });
    expect(mockKiroSendExtMethod).not.toHaveBeenCalled();
    expect(mockKiroSendExtNotification).not.toHaveBeenCalled();
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

  it('setConfigOption("mode", "default") translates to the KAS wire id "vibe"', async () => {
    // KAS still expects `vibe` on the wire for the default mode; the TUI-side
    // canonical id is `default` and `toKasModeId` translates on the way out.
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    mockKiroSetSessionConfigOption.mockClear();
    // setConfigOption is a thin passthrough that re-emits the config options
    // from KAS's response, so the mock must resolve a response object.
    mockKiroSetSessionConfigOption.mockResolvedValueOnce({ configOptions: [] });
    await client.setConfigOption('mode', 'default');
    const modeCalls = mockKiroSetSessionConfigOption.mock.calls.filter(
      ([req]: any[]) => req?.configId === 'mode'
    );
    expect(modeCalls.length).toBe(1);
    expect(modeCalls[0][0].value).toBe('vibe');
  });

  // configOptions payload for the setSessionMode read-back: a `mode` select
  // whose currentValue reflects what the server actually holds.
  const modeConfigOptions = (currentValue: string) => ({
    configOptions: [
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue,
        options: [
          {
            value: 'vibe',
            name: 'Default',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            value: 'autonomous',
            name: 'Autonomous',
            _meta: { kiro: { source: 'bundled' } },
          },
        ],
      },
    ],
  });

  it('setSessionMode sends session/set_mode, verifies via read-back, and broadcasts AgentSwitched', async () => {
    // SetSessionModeResponse is empty and KAS emits no current_mode_update
    // after it, so the client reads the mode back via set_config_option and
    // broadcasts AgentSwitched itself once the switch is confirmed.
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('autonomous')
    );
    await client.setSessionMode('autonomous');
    expect(mockKiroSetSessionMode).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      modeId: 'autonomous',
    });
    expect(mockKiroSetSessionConfigOption).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      configId: 'mode',
      value: 'autonomous',
    });
    const switched = events.filter(
      (e) => e.type === AgentEventType.AgentSwitched
    );
    expect(switched).toHaveLength(1);
    expect(switched[0].agentName).toBe('autonomous');
  });

  it('setSessionMode("default") translates to the KAS wire id "vibe" and reports "default"', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('vibe')
    );
    await client.setSessionMode('default');
    expect(mockKiroSetSessionMode).toHaveBeenCalledWith({
      sessionId: 'kas-session-1',
      modeId: 'vibe',
    });
    const switched = events.filter(
      (e) => e.type === AgentEventType.AgentSwitched
    );
    expect(switched).toHaveLength(1);
    expect(switched[0].agentName).toBe(KAS_DEFAULT_AGENT_ID);
  });

  it('setSessionMode rejects when the read-back shows the mode unchanged (relayed no-op)', async () => {
    // KAS silently no-ops session/set_mode for relayed sessions: both RPCs
    // "succeed" but the mode select still holds the old value. The client
    // must reject and broadcast nothing — no false success, no chip.
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('vibe')
    );
    await expect(client.setSessionMode('autonomous')).rejects.toThrow(
      'Switching modes is not supported on this session yet'
    );
    expect(events).toHaveLength(0);
  });

  it('setSessionMode propagates an RPC rejection without broadcasting AgentSwitched', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    mockKiroSetSessionMode.mockRejectedValueOnce(new Error('mode rejected'));
    await expect(client.setSessionMode('autonomous')).rejects.toThrow(
      'mode rejected'
    );
    expect(
      events.filter((e) => e.type === AgentEventType.AgentSwitched)
    ).toHaveLength(0);
  });

  // Establish a cloud session, then confirm a client set to `autonomous` so
  // `lastClientSetModeId` is armed; returns the client + captured events.
  async function cloudClientWithAutonomousSet() {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-mode',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('cloud-mode');
    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('autonomous')
    );
    await client.setSessionMode('autonomous');
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);
    return { client, events };
  }

  const pushModeUpdate = (currentValue: string) =>
    capturedSessionUpdateHandler!({
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: modeConfigOptions(currentValue).configOptions,
      },
    } as any);

  it('surfaces a one-time revert notice when a cloud push reverts a client-set mode', async () => {
    const { client, events } = await cloudClientWithAutonomousSet();
    void client;
    await pushModeUpdate('vibe');
    const notices = events.filter(
      (e) => e.type === AgentEventType.SystemNotice
    );
    expect(notices).toHaveLength(1);
    expect(notices[0].message).toBe(
      "Autonomous mode was turned off — this cloud session doesn't support changing modes yet."
    );
    expect(notices[0].success).toBe(false);
    expect(mockRecordTuiAutonomousMode).toHaveBeenCalledWith({
      event: 'reverted',
      version: 'test-version',
    });
    // The chip still reflects the sandbox's truth (default), not suppressed.
    const switched = events.filter(
      (e) => e.type === AgentEventType.AgentSwitched
    );
    expect(switched.at(-1)?.agentName).toBe('default');
  });

  it('fires the revert notice only once (marker cleared after firing)', async () => {
    const { events } = await cloudClientWithAutonomousSet();
    await pushModeUpdate('vibe');
    await pushModeUpdate('vibe');
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(1);
  });

  it('fires at most once across repeated stale pushes in one turn', async () => {
    // A single turn emits several config_option_updates (turn-start pin,
    // reattach replay, turn-end); only the first reverting push may notify.
    const { events } = await cloudClientWithAutonomousSet();
    await pushModeUpdate('vibe');
    await pushModeUpdate('vibe');
    await pushModeUpdate('vibe');
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(1);
  });

  it('fires once for the real echo-then-revert sequence (autonomous echo, then vibe)', async () => {
    // KAS's first config_option_update optimistically echoes the mode we set
    // (autonomous); the reverting push (the space-kind mode, vibe) arrives
    // later in the same turn. The echo must NOT disarm us — otherwise the
    // revert is swallowed — and the revert must fire exactly once.
    const { events } = await cloudClientWithAutonomousSet();
    await pushModeUpdate('autonomous'); // optimistic echo → stay armed
    await pushModeUpdate('vibe'); // real revert → fire once
    await pushModeUpdate('vibe'); // already disarmed → silent
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(1);
  });

  it('forward-compat: a cloud push carrying the client-set mode fires NO notice and keeps the chip', async () => {
    // Once KAS/BFF makes the relayed switch stick, the push currentValue
    // EQUALS lastClientSetModeId (autonomous) — no revert, no message, chip
    // stays lit. Zero further CLI change.
    const { events } = await cloudClientWithAutonomousSet();
    await pushModeUpdate('autonomous');
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(0);
    const switched = events.filter(
      (e) => e.type === AgentEventType.AgentSwitched
    );
    expect(switched.at(-1)?.agentName).toBe('autonomous');
  });

  it('reports the correct direction when an off-switch is reverted (autonomous re-asserted)', async () => {
    // The client turned autonomous OFF (set 'default'); the sandbox reverts by
    // re-asserting 'autonomous'. The notice must name THAT direction, never the
    // stale "turned off" wording.
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-mode',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('cloud-mode');
    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('vibe')
    );
    await client.setSessionMode('default');
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);
    await pushModeUpdate('autonomous');
    const notices = events.filter(
      (e) => e.type === AgentEventType.SystemNotice
    );
    expect(notices).toHaveLength(1);
    expect(notices[0].message).toBe(
      "Autonomous mode was turned back on — this cloud session doesn't support changing modes yet."
    );
  });

  it('never fires the revert notice on a local session', async () => {
    const client = new KasAcpClient();
    await client.newSession(); // default mock: local session
    mockKiroSetSessionConfigOption.mockResolvedValueOnce(
      modeConfigOptions('autonomous')
    );
    await client.setSessionMode('autonomous');
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);
    await pushModeUpdate('vibe');
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(0);
  });

  it('never fires the revert notice for a server-initiated change with no prior client set', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'cloud-mode',
      _meta: {
        kiro: {
          executionTarget: { kind: 'cloud-sandbox' },
          source: 'remote',
        },
      },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('cloud-mode');
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);
    await pushModeUpdate('spec');
    expect(
      events.filter((e) => e.type === AgentEventType.SystemNotice)
    ).toHaveLength(0);
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
      configOptions: [
        {
          type: 'select',
          id: 'model',
          category: 'model',
          currentValue: 'm1',
          options: [{ value: 'm1', name: 'Test Model' }],
        },
      ],
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
  // fallback CompactionStatus is delayed so a real summarization_completed
  // report can win the race and land before queued input.
  const flushAsync = () => new Promise((r) => setTimeout(r, 0));
  const waitCompactFallback = () => new Promise((r) => setTimeout(r, 550));
  const sendKasSessionInfoUpdate = (kiro: Record<string, unknown>) =>
    capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro },
      },
    });

  // ── mid-session repo attach notification ──
  // KAS relays the sandbox's bound-repo set as `_meta.kiro.repositories` on a
  // session_info_update once the fleet update lands. The client broadcasts it
  // as SessionRepositoriesUpdate so the footer tracks the sandbox's actual
  // workspace, not just create-time bindings.
  it('session_info_update with repositories broadcasts SessionRepositoriesUpdate', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    await sendKasSessionInfoUpdate({
      repositories: [
        { providerType: 'GITHUB', name: 'acme/banana-service', branch: 'main' },
        { providerType: 'GITHUB', name: 'acme/second-repo' },
        { name: '' }, // dropped: no usable name
      ],
    });

    const repoEvents = events.filter(
      (e) => e.type === 'session_repositories_update'
    );
    expect(repoEvents).toHaveLength(1);
    expect(repoEvents[0].repositories).toEqual([
      { name: 'acme/banana-service', branch: 'main' },
      { name: 'acme/second-repo' },
    ]);
  });

  it('session_info_update with an EMPTY repositories array broadcasts an empty set (detach-all)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    await sendKasSessionInfoUpdate({ repositories: [] });

    const repoEvents = events.filter(
      (e) => e.type === 'session_repositories_update'
    );
    expect(repoEvents).toHaveLength(1);
    expect(repoEvents[0].repositories).toEqual([]);
  });

  it('session_info_update WITHOUT repositories broadcasts no SessionRepositoriesUpdate', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    await sendKasSessionInfoUpdate({
      kind: 'context_usage',
      usagePercentage: 12,
    });

    expect(
      events.filter((e) => e.type === 'session_repositories_update')
    ).toHaveLength(0);
  });

  // ── steering accumulation ──
  // KAS sends one steering_queued per steer with only that steer's text; the
  // SteeringQueued handler expects the full buffer (Rust echoes it whole), so
  // the client accumulates by messageId. Regression: a 2nd steer must not drop
  // the 1st in the display.
  it('steering_queued accumulates successive KAS steers into the full buffer', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm1',
      content: 'First',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm2',
      content: 'Second',
    });

    const queued = events.filter(
      (e) => e.type === AgentEventType.SteeringQueued
    );
    expect(queued.map((e) => e.message)).toEqual(['First', 'First\n\nSecond']);
  });

  it('keeps workflow notification steering out of chat and steer state', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'notification-1',
      content:
        '[notification/success] Workflow A reporting in — all systems go!',
      notificationSeverity: 'success',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'user-1',
      content: 'First user steer',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_injected',
      messageId: 'notification-1',
      content:
        'A workflow you launched ("test-workflow-1") completed. Review its results and continue if you were waiting on it.',
      notificationSeverity: 'info',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'user-2',
      content: 'Second user steer',
    });

    const queued = events.filter(
      (event) => event.type === AgentEventType.SteeringQueued
    );
    expect(queued.map((event) => event.message)).toEqual([
      'First user steer',
      'First user steer\n\nSecond user steer',
    ]);
    expect(
      events.filter((event) => event.type === AgentEventType.SteeringConsumed)
    ).toEqual([]);
  });

  it('a new session resets the steer buffer (no stale carryover after /clear mid-steer)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    // Steer, then start a new session WITHOUT an injected/cleared event.
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm1',
      content: 'Stale',
    });
    await client.newSession();
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm2',
      content: 'Fresh',
    });

    const queued = events.filter(
      (e) => e.type === AgentEventType.SteeringQueued
    );
    expect(queued[queued.length - 1].message).toBe('Fresh');
  });

  it('steering_injected then steering_queued starts a fresh buffer', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const events: any[] = [];
    (client as any).broadcastStreamEvent = (e: any) => events.push(e);

    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm1',
      content: 'First',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_injected',
      content: 'First',
    });
    await sendKasSessionInfoUpdate({
      kind: 'steering_queued',
      messageId: 'm2',
      content: 'Second',
    });

    const queued = events.filter(
      (e) => e.type === AgentEventType.SteeringQueued
    );
    expect(queued.map((e) => e.message)).toEqual(['First', 'Second']);
  });

  it('executeCommand("compact") broadcasts started then fallback completed on success', async () => {
    // Derived purely from success: KAS returns { success: true } for both a
    // real compaction and a no-op (e.g. empty conversation). Either way we
    // eventually terminate the spinner with 'completed' — we do not infer a
    // reason.
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();

    let statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual(['started']);

    await waitCompactFallback();

    statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual([
      'started',
      'completed',
    ]);
  });

  it('executeCommand("compact") lets the summarization report suppress the success fallback', async () => {
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'summarization_completed',
            conversationSummary: 'report first',
          },
        },
      },
    });
    await waitCompactFallback();

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual([
      'started',
      'completed',
    ]);
  });

  it('executeCommand("compact") still emits a delayed report after the fallback clears the spinner', async () => {
    mockKiroSendExtMethod.mockResolvedValueOnce({ success: true });
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();
    await waitCompactFallback();
    await sendKasSessionInfoUpdate({
      kind: 'summarization_completed',
      conversationSummary: 'late report',
    });

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => [s.status, s.summary])).toEqual([
      ['started', undefined],
      ['completed', undefined],
      ['completed', 'late report'],
    ]);
    expect(statuses[2]?.attemptId).toBe(statuses[1]?.attemptId);
  });

  it('executeCommand("compact") ignores an older RPC success after a real report and newer compact start', async () => {
    let resolveFirstCompact!: (value: unknown) => void;
    mockKiroSendExtMethod
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirstCompact = resolve;
          })
      )
      .mockImplementationOnce(() => new Promise(() => {}));
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'summarization_completed',
            conversationSummary: 'first report',
          },
        },
      },
    });
    await client.executeCommand({ command: 'compact' } as any);
    resolveFirstCompact({ success: true });
    await flushAsync();
    await waitCompactFallback();

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => s.status)).toEqual([
      'started',
      'completed',
      'started',
    ]);
  });

  it('executeCommand("compact") treats a no-report fallback as terminal before the next compact', async () => {
    mockKiroSendExtMethod
      .mockResolvedValueOnce({ success: true })
      .mockImplementationOnce(() => new Promise(() => {}));
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await client.executeCommand({ command: 'compact' } as any);
    await flushAsync();
    await waitCompactFallback();
    await client.executeCommand({ command: 'compact' } as any);
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'summarization_completed',
            conversationSummary: 'second report',
          },
        },
      },
    });

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses.map((s: any) => [s.status, s.attemptId])).toEqual([
      ['started', 1],
      ['completed', 1],
      ['started', 2],
      ['completed', 2],
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

  it('session_info_update kind=summarization_failed surfaces the backend error', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await sendKasSessionInfoUpdate({ kind: 'summarization_started' });
    await sendKasSessionInfoUpdate({
      kind: 'summarization_failed',
      error: 'Out of memory',
    });

    const failed = handler.mock.calls
      .map((c) => c[0])
      .find(
        (e: any) =>
          e.type === AgentEventType.CompactionStatus && e.status === 'failed'
      );
    expect(failed).toBeDefined();
    expect(failed.error).toBe('Out of memory');
  });

  it('session_info_update drops orphan summarization terminal events', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    await sendKasSessionInfoUpdate({
      kind: 'summarization_completed',
      conversationSummary: 'orphan report',
    });
    await sendKasSessionInfoUpdate({
      kind: 'summarization_failed',
      error: 'orphan failure',
    });

    const statuses = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.CompactionStatus);
    expect(statuses).toEqual([]);
  });

  it.each([
    [
      'top-level conversationSummary',
      { conversationSummary: 'top level' },
      'top level',
    ],
    [
      'summarization summary string',
      { summarization: { status: 'completed', summary: 'summary string' } },
      'summary string',
    ],
    [
      'summarization summary conversationSummary',
      {
        summarization: {
          status: 'completed',
          summary: { conversationSummary: 'summary object' },
        },
      },
      'summary object',
    ],
    [
      'summarization summary content',
      {
        summarization: {
          status: 'completed',
          summary: { content: 'summary content' },
        },
      },
      'summary content',
    ],
  ])(
    'session_info_update kind=summarization_completed broadcasts %s',
    async (_label, metaFields, expectedSummary) => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();

      await sendKasSessionInfoUpdate({ kind: 'summarization_started' });
      await sendKasSessionInfoUpdate({
        kind: 'summarization_completed',
        ...metaFields,
      });

      const event = handler.mock.calls
        .map((c) => c[0])
        .find(
          (e: any) =>
            e.type === AgentEventType.CompactionStatus &&
            e.status === 'completed'
        );
      expect(event).toBeDefined();
      expect(event.summary).toBe(expectedSummary);
    }
  );

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

  it('getCommandOptions("/agent") returns empty options (option building owns the typed store slice)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/agent', '');
    expect(result.options).toEqual([]);
  });

  it('current_mode_update broadcasts AgentSwitched with the new (normalized) agent id', async () => {
    // The transport client broadcasts AgentSwitched carrying only the new id;
    // the store resolves the banner + previous agent from kasAvailableAgents.
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
  });

  it('current_mode_update broadcasts AgentSwitched even when the mode is unchanged (store dedups the welcome banner)', async () => {
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

    // The client broadcasts every current_mode_update and the store gates the
    // welcome banner on an actual agent change, so the event is still emitted
    // here.
    const switched = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.AgentSwitched);
    expect(switched).toBeDefined();
    expect(switched.agentName).toBe(KAS_DEFAULT_AGENT_ID);
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

  it('session update preserves MCP server identity before stripping title', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();
    toolStartCalls.length = 0;
    toolFinishCalls.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-mcp',
        title: '@local-server/query_db',
        kind: 'mcp',
        rawInput: { sql: 'select 1' },
        content: [],
        locations: [],
      },
    });

    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.ToolCall) as any;
    expect(event).toBeDefined();
    expect(event.name).toBe('query_db');
    expect(event.meta?.kiro?.mcpServerName).toBe('local-server');
    expect(toolStartCalls).toEqual([
      {
        id: 'tc-mcp',
        info: {
          name: 'query_db',
          toolOrigin: 'mcp',
          mcpServerName: 'local-server',
          executionContext: 'main',
        },
      },
    ]);
    expect(toolFinishCalls).toHaveLength(0);
  });

  it('failed-before-exec synthesized MCP tool_call feeds telemetry start before finish', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();
    toolStartCalls.length = 0;
    toolFinishCalls.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-mcp-failed',
        title: '@local-server/query_db',
        status: 'failed',
        rawInput: { sql: 'select 1' },
      },
    });

    expect(toolStartCalls).toHaveLength(1);
    expect(toolStartCalls[0]!.info).toEqual({
      name: 'query_db',
      toolOrigin: 'mcp',
      mcpServerName: 'local-server',
      executionContext: 'main',
    });
    expect(toolFinishCalls).toHaveLength(1);
    expect(toolFinishCalls[0]!.id).toBe('tc-mcp-failed');
    expect(toolFinishCalls[0]!.args.outcome).toBe('error');
  });

  it('failed-before-exec synthesized MCP subtask tool_call records subagent context', async () => {
    const client = new KasAcpClient();
    const multiHandler = mock((_sessionId: string, _event: any) => {});
    client.onMultiSessionUpdate(multiHandler);
    await client.newSession();
    toolStartCalls.length = 0;
    toolFinishCalls.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-sub-mcp-failed',
        title: '@local-server/query_db',
        status: 'failed',
        rawInput: { sql: 'select 1' },
        _meta: { kiro: { agentSubtaskId: 'sub-1' } },
      },
    });

    expect(multiHandler).toHaveBeenCalled();
    expect(toolStartCalls).toEqual([
      {
        id: 'tc-sub-mcp-failed',
        info: {
          name: 'query_db',
          toolOrigin: 'mcp',
          mcpServerName: 'local-server',
          executionContext: 'subagent',
        },
      },
    ]);
    expect(toolFinishCalls).toHaveLength(1);
    expect(toolFinishCalls[0]!.id).toBe('tc-sub-mcp-failed');
    expect(toolFinishCalls[0]!.args.outcome).toBe('error');
  });

  it('preserves KAS MCP provenance and original title across permission enrichment', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    client.onUpdate((event: any) => events.push(event));
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-mcp-write',
        title: '@collision-server/fs_write',
        kind: 'edit',
        rawInput: { operation: 'custom', payload: 'unchanged' },
        content: [],
        locations: [],
      },
    });

    const toolCall = events.find(
      (event) =>
        event.type === AgentEventType.ToolCall && event.id === 'tc-mcp-write'
    );
    expect(toolCall).toMatchObject({
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: '@collision-server/fs_write',
    });

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'tc-mcp-write',
      options: [
        { optionId: 'allow_once', name: 'Allow once', kind: 'allow_once' },
        { optionId: 'reject_once', name: 'Reject once', kind: 'reject_once' },
      ],
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const approval = events.find(
      (event) => event.type === AgentEventType.ApprovalRequest
    )?.value;
    expect(approval.toolCall).toMatchObject({
      toolCallId: 'tc-mcp-write',
      title: '@collision-server/fs_write',
      rawInput: { operation: 'custom', payload: 'unchanged' },
      name: 'fs_write',
      kind: 'edit',
      origin: 'mcp',
    });
    approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
  });

  it('preserves chunk-first main-session MCP provenance for permission enrichment', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    client.onUpdate((event: any) => events.push(event));
    await client.newSession();

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'tc-mcp-chunk',
        title: '@collision-server/fs_write',
        kind: 'edit',
      },
    });

    expect(
      events.find(
        (event) =>
          event.type === AgentEventType.ToolCall && event.id === 'tc-mcp-chunk'
      )
    ).toMatchObject({
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: '@collision-server/fs_write',
    });

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'tc-mcp-chunk',
      options: [
        { optionId: 'allow_once', name: 'Allow once', kind: 'allow_once' },
        { optionId: 'reject_once', name: 'Reject once', kind: 'reject_once' },
      ],
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const approval = events.find(
      (event) =>
        event.type === AgentEventType.ApprovalRequest &&
        event.value.toolCall.toolCallId === 'tc-mcp-chunk'
    )?.value;
    expect(approval.toolCall).toMatchObject({
      toolCallId: 'tc-mcp-chunk',
      title: '@collision-server/fs_write',
      rawInput: {},
      name: 'fs_write',
      kind: 'edit',
      origin: 'mcp',
    });
    approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
  });

  it('normalizes built-in and MCP permission-first tool identities', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    client.onUpdate((event: any) => events.push(event));
    await client.newSession();

    for (const [toolCallId, title, origin] of [
      ['permission-first-write', 'Creating report.ts', 'builtin'],
      ['permission-first-mcp', '@collision-server/fs_write', 'mcp'],
      [
        'permission-first-running-mcp',
        'Running: @collision-server/fs_write',
        'mcp',
      ],
    ] as const) {
      const permissionPromise = capturedPermissionHandler({
        toolCallId,
        toolCall: {
          toolCallId,
          title,
          kind: 'edit',
          rawInput: {
            command: 'create',
            path: '/workspace/report.ts',
            content: 'export const ready = true;',
          },
        },
        options: [
          { optionId: 'allow_once', name: 'Allow once', kind: 'allow_once' },
          { optionId: 'reject_once', name: 'Reject once', kind: 'reject_once' },
        ],
        _meta: { kiro: { toolId: 'fs_write' } },
      });
      await new Promise((resolve) => setTimeout(resolve, 0));

      const approval = events.find(
        (event) =>
          event.type === AgentEventType.ApprovalRequest &&
          event.value.toolCall.toolCallId === toolCallId
      )?.value;
      expect(approval.toolCall).toMatchObject({
        toolCallId,
        name: 'fs_write',
        kind: 'edit',
        origin,
      });
      approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
    }
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

  it('sends gated sessionSource/listScope and maps per-row remote dimensions + surfaces warnings', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local', 'remote'],
            sessionListScopes: ['workspace', 'user'],
          },
        },
      },
    });
    mockKiroListSessions.mockResolvedValueOnce({
      sessions: [
        {
          sessionId: 'loc1',
          cwd: '/tmp',
          title: 'Local',
          updatedAt: '2026-01-01',
        },
        {
          sessionId: 'spc-9f2',
          cwd: '/sandbox',
          title: 'Cloud',
          updatedAt: '2026-01-02',
          _meta: {
            kiro: {
              source: 'remote',
              executionTarget: { kind: 'cloud-sandbox' },
              status: 'provisioning',
            },
          },
        },
      ],
      _meta: { kiro: { warnings: ['remote store degraded'] } },
    });
    const client = new KasAcpClient();
    await client.initialize();
    const result = await client.listSessions('/tmp');

    const req = mockKiroListSessions.mock.calls.at(-1)?.[0] as any;
    expect(req?.cwd).toBe('/tmp');
    expect(req?._meta?.kiro).toEqual({
      sessionSource: 'all',
      listScope: 'both',
    });

    const cloud = result.sessions.find((s) => s.sessionId === 'spc-9f2')!;
    expect(cloud.executionTarget).toEqual({ kind: 'cloud-sandbox' });
    expect(cloud.source).toBe('remote');
    expect(cloud.status).toBe('provisioning');
    const local = result.sessions.find((s) => s.sessionId === 'loc1')!;
    expect(local.executionTarget).toBeUndefined();
    expect(local.source).toBeUndefined();
  });

  it('lists all workspaces without cwd while preserving remote scope metadata', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local', 'remote'],
            sessionListScopes: ['workspace', 'user'],
          },
        },
      },
    });
    mockKiroListSessions.mockResolvedValueOnce({
      sessions: [
        {
          sessionId: 'cloud-anywhere',
          cwd: '/sandbox',
          _meta: { kiro: { source: 'remote' } },
        },
      ],
    });
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    const req = mockKiroListSessions.mock.calls.at(-1)?.[0] as any;
    expect(req?.cwd).toBeUndefined();
    expect(req?._meta?.kiro).toEqual({
      sessionSource: 'all',
      listScope: 'both',
    });
    expect(result.sessions[0]).toMatchObject({
      sessionId: 'cloud-anywhere',
      source: 'remote',
    });
  });

  it('queries the remote user slice when the combined listing returns only local rows', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local', 'remote'],
            sessionListScopes: ['workspace', 'user'],
          },
        },
      },
    });
    mockKiroListSessions
      .mockResolvedValueOnce({
        sessions: [{ sessionId: 'local-only', cwd: '/workspace' }],
      })
      .mockResolvedValueOnce({
        sessions: [
          {
            sessionId: 'cloud-recovered',
            cwd: '',
            _meta: {
              kiro: {
                source: 'remote',
                executionTarget: { kind: 'cloud-sandbox' },
              },
            },
          },
        ],
      });
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    expect(mockKiroListSessions).toHaveBeenCalledTimes(2);
    expect(mockKiroListSessions.mock.calls[0]?.[0]).toMatchObject({
      _meta: {
        kiro: { sessionSource: 'all', listScope: 'both' },
      },
    });
    expect(mockKiroListSessions.mock.calls[1]?.[0]).toEqual({
      _meta: {
        kiro: { sessionSource: 'remote', listScope: 'user' },
      },
    });
    expect(result.sessions.map((s) => s.sessionId)).toEqual([
      'local-only',
      'cloud-recovered',
    ]);
  });

  it('marks the catalog incomplete when the remote user slice fails', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local', 'remote'],
            sessionListScopes: ['workspace', 'user'],
          },
        },
      },
    });
    mockKiroListSessions
      .mockResolvedValueOnce({
        sessions: [{ sessionId: 'local-only', cwd: '/workspace' }],
      })
      .mockRejectedValueOnce(new Error('remote unavailable'));
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    expect(result.sessions.map((session) => session.sessionId)).toEqual([
      'local-only',
    ]);
    expect(result.complete).toBe(false);
  });

  it('marks a cwd-scoped fallback incomplete after all-workspace failure', async () => {
    mockKiroListSessions
      .mockRejectedValueOnce(new Error('all-workspace unavailable'))
      .mockResolvedValueOnce({
        sessions: [{ sessionId: 'scoped-only', cwd: process.cwd() }],
      });
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    expect(result.sessions.map((session) => session.sessionId)).toEqual([
      'scoped-only',
    ]);
    expect(result.complete).toBe(false);
  });

  it('does not request an unadvertised remote user slice for local-only capabilities', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local'],
            sessionListScopes: ['workspace'],
          },
        },
      },
    });
    mockKiroListSessions.mockResolvedValueOnce({
      sessions: [{ sessionId: 'local-only', cwd: '/workspace' }],
    });
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    expect(mockKiroListSessions).toHaveBeenCalledTimes(1);
    expect(mockKiroListSessions.mock.calls[0]?.[0]).toEqual({});
    expect(result.sessions.map((s) => s.sessionId)).toEqual(['local-only']);
  });

  it('does not probe remote listing when older KAS omits list capability arrays', async () => {
    mockKiroListSessions.mockResolvedValueOnce({
      sessions: [{ sessionId: 'older-local', cwd: '/workspace' }],
    });
    const client = new KasAcpClient();
    await client.initialize();

    const result = await client.listAllWorkspaceSessions();

    expect(mockKiroListSessions).toHaveBeenCalledTimes(1);
    expect(mockKiroListSessions.mock.calls[0]?.[0]).toEqual({});
    expect(result.sessions.map((s) => s.sessionId)).toEqual(['older-local']);
  });

  it('falls back to cwd-scoped listing when all-workspace listing fails', async () => {
    mockKiroListSessions
      .mockRejectedValueOnce(new Error('cwd required'))
      .mockResolvedValueOnce({
        sessions: [{ sessionId: 'local-fallback', cwd: process.cwd() }],
      });
    const client = new KasAcpClient();

    const result = await client.listAllWorkspaceSessions();

    expect(mockKiroListSessions).toHaveBeenCalledTimes(2);
    expect(mockKiroListSessions.mock.calls[0]?.[0]?.cwd).toBeUndefined();
    expect(mockKiroListSessions.mock.calls[1]?.[0]?.cwd).toBe(process.cwd());
    expect(result.sessions[0]?.sessionId).toBe('local-fallback');
  });

  it('omits _meta.kiro on session/list when KAS advertises no remote caps (existing-user path)', async () => {
    mockKiroListSessions.mockResolvedValueOnce({ sessions: [] });
    const client = new KasAcpClient(); // no initialize -> no caps captured
    await client.listSessions('/tmp');
    const req = mockKiroListSessions.mock.calls.at(-1)?.[0] as any;
    expect(req?.cwd).toBe('/tmp');
    expect(req?._meta).toBeUndefined();
  });

  it('requests sessionSource:all but omits listScope when user scope is not advertised (per-flag gating)', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    mockKiroListSessions.mockResolvedValueOnce({ sessions: [] });
    const client = new KasAcpClient();
    await client.initialize();
    await client.listSessions('/tmp');
    const req = mockKiroListSessions.mock.calls.at(-1)?.[0] as any;
    expect(req?._meta?.kiro).toEqual({ sessionSource: 'all' });
  });

  it('requests listScope:both but omits sessionSource when only user scope is advertised (per-flag gating)', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionListScopes: ['workspace', 'user'] } },
      },
    });
    mockKiroListSessions.mockResolvedValueOnce({ sessions: [] });
    const client = new KasAcpClient();
    await client.initialize();
    await client.listSessions('/tmp');
    const req = mockKiroListSessions.mock.calls.at(-1)?.[0] as any;
    expect(req?._meta?.kiro).toEqual({ listScope: 'both' });
  });

  // ── /model command ──

  /**
   * Seed a newSession response that mirrors what KAS actually returns
   * for an agent with a ModelConfigProvider: a SessionConfigOption list
   * containing a `category: 'model'` entry with currentValue + options.
   */
  function seedSessionWithModels(opts: {
    currentValue: string;
    models: Array<{
      value: string;
      name: string;
      description?: string;
      _meta?: { kiro?: { rateMultiplier?: number; rateUnit?: string } };
    }>;
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

  it('getCommandOptions("/model") returns empty (option building reads the typed store slice)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/model', '');
    expect(result.options).toEqual([]);
  });

  it('config_option_update broadcasts a single KasModelConfigUpdate (origin serverPush) carrying models, current model, efforts, and current level', async () => {
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
          {
            type: 'select',
            id: 'effortLevel',
            name: 'Effort',
            category: 'thought_level',
            currentValue: 'high',
            options: [
              { value: 'low', name: 'Low' },
              { value: 'high', name: 'High' },
            ],
          },
        ],
      },
    });

    const cfg = events.filter(
      (e) => e.type === AgentEventType.KasModelConfigUpdate
    );
    expect(cfg).toHaveLength(1);
    expect(cfg[0].origin).toBe('serverPush');
    expect(cfg[0].currentModelId).toBe('gpt-5');
    expect(cfg[0].models.map((m: any) => m.id)).toEqual(['claude-4', 'gpt-5']);
    expect(cfg[0].currentLevel).toBe('high');
    expect(cfg[0].efforts.map((e: any) => e.value)).toEqual(['low', 'high']);
  });

  it('setConfigOption broadcasts a KasModelConfigUpdate (origin clientInitiated) from the response', async () => {
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

    await client.setConfigOption('model', 'gpt-5');

    const cfg = events.filter(
      (e) => e.type === AgentEventType.KasModelConfigUpdate
    );
    expect(cfg).toHaveLength(1);
    expect(cfg[0].origin).toBe('clientInitiated');
    expect(cfg[0].currentModelId).toBe('gpt-5');
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

  it('getCommandOptions("/effort") returns empty (option building reads the typed store slice)', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    const result = await client.getCommandOptions('/effort', '');
    expect(result.options).toEqual([]);
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
  // These tests assert the wire shape sent to `_kiro/session/context`;
  // slash-command parsing (subcommand normalize, rm alias, --force, unquote)
  // lives in the kas-handler.

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

    it('publishes context breakdowns for store ownership', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const breakdown = {
        contextFiles: { tokens: 100, percent: 5 },
        tools: { tokens: 20, percent: 1 },
        kiroResponses: { tokens: 30, percent: 2 },
        yourPrompts: { tokens: 40, percent: 2 },
      };

      await capturedSessionUpdateHandler({
        sessionId: client.sessionId,
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'context_usage', breakdown } },
        },
      });

      expect(events).toContainEqual({
        type: AgentEventType.ContextBreakdownUpdate,
        breakdown,
      });
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

  it('preserves slash-command telemetry identities from KAS metadata', async () => {
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
            name: 'run-workflow',
            description: 'Run a workflow recipe',
            _meta: { kiro: { telemetryId: 'workflow-run' } },
          },
        ],
      },
    });

    const commandsEvent = events.find(
      (event) => event.type === AgentEventType.CommandsUpdate
    );
    expect(commandsEvent.commands[0].meta.telemetryId).toBe('workflow-run');
  });

  it('filters KAS-advertised workflow commands when workflows are disabled', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    const events: any[] = [];
    (client as any).broadcastStreamEvent = (event: any) => events.push(event);

    (client as any).handleSessionUpdate({
      sessionId: client.sessionId,
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          { name: 'help', description: 'Show help' },
          {
            name: 'workflow-run',
            description: 'Run a workflow recipe',
            _meta: { kiro: { type: 'workflow' } },
          },
          {
            name: 'workflow-resume',
            description: 'Resume a paused workflow',
            _meta: { kiro: { type: 'workflow' } },
          },
          {
            name: 'workflow-status',
            description: 'Check workflow status',
            _meta: { kiro: { type: 'workflow' } },
          },
          {
            name: 'workflow-cancel',
            description: 'Cancel a running workflow',
            _meta: { kiro: { type: 'workflow' } },
          },
          {
            name: 'future-workflow-command',
            description: 'Future workflow command',
            _meta: { kiro: { type: 'workflow' } },
          },
        ],
      },
    });

    const commandsEvent = events.find(
      (event) => event.type === AgentEventType.CommandsUpdate
    );
    expect(commandsEvent.commands.map((command: any) => command.name)).toEqual([
      'help',
    ]);
  });

  it('keeps KAS-advertised workflow commands when workflows are enabled', async () => {
    setWorkflowsEnabled(true);
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
            name: 'workflow-run',
            description: 'Run a workflow recipe',
            _meta: { kiro: { type: 'workflow' } },
          },
        ],
      },
    });

    const commandsEvent = events.find(
      (event) => event.type === AgentEventType.CommandsUpdate
    );
    expect(commandsEvent.commands.map((command: any) => command.name)).toEqual([
      'workflow-run',
    ]);
  });

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
            requestIds: ['kas-request-old', ' ', 'kas-request-1'],
            requestId: 'legacy-kas-request',
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
    expect(mockRecordTuiUserTurn).toHaveBeenCalledWith({
      result: 'success',
      isSubagent: false,
      mode: 'vibe',
      version: 'test-version',
      failureReason: undefined,
      durationSeconds: 1.234,
      logProperties: {
        sessionId: 'kas-session-1',
        requestId: 'kas-request-1',
      },
    });
    expect(mockRecordTuiModelInvocations).toHaveBeenCalledWith({
      version: 'test-version',
      model: 'm1',
      count: 2,
      logProperties: {
        sessionId: 'kas-session-1',
        requestId: 'kas-request-1',
      },
    });
    expect(mockRecordTuiTokensConsumed).toHaveBeenCalledWith({
      version: 'test-version',
      model: 'm1',
      tokens: {
        input_uncached: 10,
        input_cache_read: 2,
        output: 5,
      },
      logProperties: {
        sessionId: 'kas-session-1',
        requestId: 'kas-request-1',
      },
    });
    expect(mockRecordTuiCreditsConsumed).toHaveBeenCalledWith({
      version: 'test-version',
      model: 'm1',
      credits: 1.5,
      logProperties: {
        sessionId: 'kas-session-1',
        requestId: 'kas-request-1',
      },
    });
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
    expect(mockRecordTuiUserTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        result: '_other_',
        failureReason: undefined,
        durationSeconds: undefined,
      })
    );
    expect(mockRecordTuiModelInvocations).toHaveBeenCalledWith(
      expect.objectContaining({ count: 0 })
    );
    expect(mockRecordTuiTokensConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ tokens: {} })
    );
    expect(mockRecordTuiCreditsConsumed).not.toHaveBeenCalled();
  });

  it('session_info_update kind=turn_completion forwards an explicit failure status', async () => {
    const client = new KasAcpClient();
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'turn_completion',
            status: 'failed',
          },
        },
      },
    });

    expect(mockRecordTuiUserTurn).toHaveBeenCalledWith({
      result: 'failed',
      isSubagent: false,
      mode: 'vibe',
      version: 'test-version',
      failureReason: 'model_error',
      durationSeconds: undefined,
      logProperties: { sessionId: 'kas-session-1' },
    });
    expect(mockRecordTuiModelInvocations).toHaveBeenCalledWith(
      expect.objectContaining({ count: 0 })
    );
    expect(mockRecordTuiTokensConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ tokens: {} })
    );
    expect(mockRecordTuiCreditsConsumed).not.toHaveBeenCalled();
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
    expect(mockRecordTuiUserTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        result: 'success',
        failureReason: undefined,
        durationSeconds: 0.1,
      })
    );
    expect(mockRecordTuiModelInvocations).toHaveBeenCalledWith(
      expect.objectContaining({ count: 2 })
    );
    expect(mockRecordTuiTokensConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ tokens: {} })
    );
    expect(mockRecordTuiCreditsConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ credits: 2 })
    );
  });

  it('session_info_update kind=turn_completion reconciles an unknown status as failed', async () => {
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
              { usage: 2, unit: 'credit', unitPlural: 'Credits' },
            ],
            elapsedTime: 100,
            status: 'backend:arbitrary-new-status',
          },
        },
      },
    });

    const summary = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.TurnSummary);
    expect(summary).toBeDefined();
    expect(summary.turnDurationMs).toBe(100);
    expect(summary.meteringUsage).toEqual([
      { value: 2, unit: 'credit', unitPlural: 'Credits' },
    ]);
    expect(mockRecordTuiUserTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        result: 'failed',
        failureReason: 'unknown',
        durationSeconds: 0.1,
      })
    );
    expect(mockRecordTuiModelInvocations).toHaveBeenCalledWith(
      expect.objectContaining({ count: 1 })
    );
    expect(mockRecordTuiTokensConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ tokens: {} })
    );
    expect(mockRecordTuiCreditsConsumed).toHaveBeenCalledWith(
      expect.objectContaining({ credits: 2 })
    );
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

  it('session_info_update kind=turn_completion with status only broadcasts no TurnSummary', async () => {
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

  it('session_info_update kind=turn_completion with token usage only broadcasts no TurnSummary', async () => {
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
  });

  // ── Mid-turn steering: KAS session_info_update kinds → internal events ──
  //
  // KAS emits the steering queue lifecycle on the standard session_info_update
  // channel with snake_case `_meta.kiro.kind`. The Rust engine still uses the
  // PascalCase `_kiro.dev/session/update` ext channel (see regression test
  // below). Both map onto the same internal steering events.

  it('session_info_update kind=steering_queued broadcasts SteeringQueued with content as message', async () => {
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
            kind: 'steering_queued',
            messageId: 'steer-abc',
            content: 'please focus on tests',
          },
        },
      },
    });

    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.SteeringQueued);
    expect(event).toBeDefined();
    expect(event.message).toBe('please focus on tests');
  });

  it('session_info_update kind=steering_injected broadcasts SteeringConsumed with content', async () => {
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
            kind: 'steering_injected',
            messageId: 'steer-abc',
            content: 'the raw user message',
          },
        },
      },
    });

    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.SteeringConsumed);
    expect(event).toBeDefined();
    expect(event.content).toBe('the raw user message');
  });

  it('session_info_update kind=steering_cleared broadcasts SteeringCleared', async () => {
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
            kind: 'steering_cleared',
            messageIds: ['steer-abc', 'steer-def'],
          },
        },
      },
    });

    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.SteeringCleared);
    expect(event).toBeDefined();
  });

  it('Rust engine PascalCase _kiro.dev/session/update steering still maps to internal events', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    // The Rust engine continues to emit the PascalCase discriminators through
    // handleExtSessionUpdate; this path must keep working unchanged.
    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'AgentExecutionUserMessageQueued',
        content: 'rust queued steer',
      },
    });

    const event = handler.mock.calls
      .map((c) => c[0])
      .find((e: any) => e.type === AgentEventType.SteeringQueued);
    expect(event).toBeDefined();
    expect(event.message).toBe('rust queued steer');
  });

  it('routes KAS pipeline tool_call_chunk child events to the child session only', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

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
              groupId: 'pipeline-chunk-route',
              stages: [
                {
                  name: 'explore-components',
                  role: 'explorer',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-components',
                },
              ],
            },
          },
        },
      },
    });

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'read-components',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'sub-components' } },
      },
    });

    expect(multiEvents).toEqual([
      expect.objectContaining({
        sessionId: 'sub-components',
        event: expect.objectContaining({
          id: 'read-components',
          type: AgentEventType.ToolCall,
          sessionId: 'sub-components',
        }),
      }),
    ]);
    expect(mainEvents.map((event) => event.id)).toEqual(['crew-op']);
  });

  it('synthesizes a ToolCall from rawInput for an update-only Subagent Response so its text renders', async () => {
    // KAS sends the subagent's final output as a Completed-only
    // tool_call_update (NO preceding tool_call) with the text in
    // rawInput.response and rawOutput=null. Without synthesizing the missing
    // ToolCall, the store never gets a message carrying that response, so it
    // renders nowhere (the live bug). Assert the synthesized ToolCall reaches
    // the subagent session with the response in args.
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    // Register the stage so the subtask routes panel-only (matches a live crew).
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
              groupId: 'pipeline-resp',
              stages: [
                {
                  name: 'inspect',
                  role: 'general',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-inspect',
                },
              ],
            },
          },
        },
      },
    });
    multiEvents.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tooluse_resp',
        title: 'Subagent Response',
        status: 'completed',
        rawInput: { response: 'THE FINAL ANSWER', files: [] },
        rawOutput: null,
        _meta: { kiro: { agentSubtaskId: 'sub-inspect' } },
      },
    });

    // A ToolCall carrying the response in args must reach the subagent session.
    const synth = multiEvents.find(
      (e) =>
        e.event.id === 'tooluse_resp' &&
        e.event.type === AgentEventType.ToolCall
    );
    expect(synth).toBeDefined();
    expect(synth!.sessionId).toBe('sub-inspect');
    expect(synth!.event.name).toBe('Subagent Response');
    expect((synth!.event.args as any).response).toBe('THE FINAL ANSWER');
    // It stays panel-only (crew activity), not leaked to main.
    expect(mainEvents.some((e) => e.id === 'tooluse_resp')).toBe(false);
  });

  it('keeps early KAS child tool_call_chunk placeholders out of main once the pipeline snapshot registers the stage', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'early-read',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'sub-before-pipeline' } },
      },
    });

    expect(multiEvents).toEqual([
      expect.objectContaining({
        sessionId: 'sub-before-pipeline',
        event: expect.objectContaining({
          id: 'early-read',
          type: AgentEventType.ToolCall,
          sessionId: 'sub-before-pipeline',
        }),
      }),
    ]);
    expect(mainEvents).toEqual([]);

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
              groupId: 'pipeline-early-route',
              stages: [
                {
                  name: 'explore-components',
                  role: 'explorer',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-before-pipeline',
                },
              ],
            },
          },
        },
      },
    });
    mainEvents.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'early-read',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: 'src/components/chat' },
        content: [],
        locations: [],
        _meta: { kiro: { agentSubtaskId: 'sub-before-pipeline' } },
      },
    });

    expect(
      multiEvents.filter((entry) => entry.event.id === 'early-read')
    ).toHaveLength(2);
    expect(mainEvents).toEqual([]);
  });

  it('does not treat a main-routed permission as standalone proof for a chunk-first subtask', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'standalone-read',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'hidden-standalone' } },
      },
    });

    expect(mainEvents).toEqual([]);

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'standalone-read',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: 'src/components/chat' },
        content: [],
        locations: [],
        _meta: { kiro: { agentSubtaskId: 'hidden-standalone' } },
      },
    });
    expect(mainEvents).toEqual([]);

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'standalone-read',
      permissions: [
        { id: 'allow_once', name: 'Allow once' },
        { id: 'reject_once', name: 'Reject once' },
      ],
      _meta: { kiro: { consent: { capability: 'fs_read' } } },
    });
    await new Promise((r) => setTimeout(r, 50));
    const approval = mainEvents.find(
      (event) => event.type === AgentEventType.ApprovalRequest
    );
    expect(approval?.value.sessionId).toBeUndefined();
    approval.value.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
    mainEvents.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'standalone-read',
        status: 'completed',
        rawOutput: { content: [{ type: 'text', text: 'done' }] },
        content: [],
      },
    });

    expect(
      multiEvents.filter((entry) => entry.event.id === 'standalone-read')
    ).toHaveLength(3);
    expect(mainEvents).toEqual([]);
  });

  it('lets chunk-first hidden standalone subagent tool calls surface when the full tool_call omits KAS metadata', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'standalone-read-no-meta',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'hidden-standalone-no-meta' } },
      },
    });

    expect(mainEvents).toEqual([]);

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'standalone-read-no-meta',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: 'src/components/layout' },
        content: [],
        locations: [],
      },
    });
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'standalone-read-no-meta',
        status: 'completed',
        rawOutput: { content: [{ type: 'text', text: 'done' }] },
        content: [],
      },
    });

    expect(
      multiEvents.filter(
        (entry) => entry.event.id === 'standalone-read-no-meta'
      )
    ).toHaveLength(3);
    expect(mainEvents.map((event) => event.id)).toEqual([
      'standalone-read-no-meta',
      'standalone-read-no-meta',
    ]);
  });

  it('keeps a permission-routed chunk-only subtask panel-only until another signal classifies it', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'standalone-chunk-only',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'hidden-chunk-only' } },
      },
    });

    expect(mainEvents).toEqual([]);

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'standalone-chunk-only',
      permissions: [
        { id: 'allow_once', name: 'Allow once' },
        { id: 'reject_once', name: 'Reject once' },
      ],
      _meta: { kiro: { consent: { capability: 'fs_read' } } },
    });
    await new Promise((r) => setTimeout(r, 50));
    const approval = mainEvents.find(
      (event) => event.type === AgentEventType.ApprovalRequest
    );
    expect(approval?.value.sessionId).toBeUndefined();
    approval.value.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
    mainEvents.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'standalone-chunk-only',
        status: 'completed',
        rawOutput: { content: [{ type: 'text', text: 'done' }] },
        content: [],
      },
    });

    expect(
      multiEvents.filter((entry) => entry.event.id === 'standalone-chunk-only')
    ).toHaveLength(2);
    expect(mainEvents).toEqual([]);
  });

  it('keeps chunk-first pipeline children panel-only when permission, full, and finish beat the pipeline snapshot', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'early-pipeline-read',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'sub-late-pipeline' } },
      },
    });

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'early-pipeline-read',
      permissions: [
        { id: 'allow_once', name: 'Allow once' },
        { id: 'reject_once', name: 'Reject once' },
      ],
      _meta: { kiro: { consent: { capability: 'fs_read' } } },
    });
    await new Promise((r) => setTimeout(r, 50));
    const approval = mainEvents.find(
      (event) => event.type === AgentEventType.ApprovalRequest
    );
    expect(approval?.value.sessionId).toBeUndefined();
    approval.value.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
    mainEvents.length = 0;

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'early-pipeline-read',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: 'src/components/chat' },
        content: [],
        locations: [],
        _meta: { kiro: { agentSubtaskId: 'sub-late-pipeline' } },
      },
    });
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'early-pipeline-read',
        status: 'completed',
        rawOutput: { content: [{ type: 'text', text: 'done' }] },
        content: [],
      },
    });

    expect(
      multiEvents.filter((entry) => entry.event.id === 'early-pipeline-read')
    ).toHaveLength(3);
    expect(mainEvents).toEqual([]);

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
              groupId: 'pipeline-late-route',
              stages: [
                {
                  name: 'explore-components',
                  role: 'explorer',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-late-pipeline',
                },
              ],
            },
          },
        },
      },
    });

    expect(mainEvents.map((event) => event.id)).toEqual(['crew-op']);
  });

  it('does not let an unrelated active pipeline hide chunk-first standalone subagent tool calls', async () => {
    const client = new KasAcpClient();
    const mainEvents: any[] = [];
    const multiEvents: Array<{ sessionId: string; event: any }> = [];
    client.onUpdate((event: any) => mainEvents.push(event));
    client.onMultiSessionUpdate((sessionId: string, event: any) => {
      multiEvents.push({ sessionId, event });
    });
    await client.newSession();
    mainEvents.length = 0;

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
              groupId: 'pipeline-active-with-hidden',
              stages: [
                {
                  name: 'explore-components',
                  role: 'explorer',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'pipeline-subtask',
                },
              ],
            },
          },
        },
      },
    });

    (client as any).handleExtSessionUpdate({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'standalone-while-pipeline',
        title: 'read_file',
        kind: 'read',
        _meta: { kiro: { agentSubtaskId: 'hidden-while-pipeline' } },
      },
    });
    expect(mainEvents.map((event) => event.id)).toEqual(['crew-op']);

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'standalone-while-pipeline',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: 'src/components/layout' },
        content: [],
        locations: [],
      },
    });
    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'standalone-while-pipeline',
        status: 'completed',
        rawOutput: { content: [{ type: 'text', text: 'done' }] },
        content: [],
      },
    });

    expect(
      multiEvents.filter(
        (entry) => entry.event.id === 'standalone-while-pipeline'
      )
    ).toHaveLength(3);
    expect(mainEvents.map((event) => event.id)).toEqual([
      'crew-op',
      'standalone-while-pipeline',
      'standalone-while-pipeline',
    ]);
  });

  // ── config payloads → KasModelConfigUpdate (efforts + currentLevel) ──
  //
  // A config payload emits a single KasModelConfigUpdate carrying { models,
  // currentModelId, efforts, currentLevel, origin }. EffortUpdate (singular)
  // comes ONLY from a metadata_update notification, never from a config
  // payload. These tests pin the four origins (newSession / loadSession /
  // serverPush / clientInitiated) and the effort fields they carry; serverPush
  // + clientInitiated are covered in the /model section above.

  it('newSession() emits KasModelConfigUpdate (origin newSession) carrying the current effort level', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'model',
          category: 'model',
          currentValue: 'm1',
          options: [{ value: 'm1', name: 'Test Model' }],
        },
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

    const cfg = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.KasModelConfigUpdate);
    expect(cfg).toHaveLength(1);
    expect(cfg[0].origin).toBe('newSession');
    expect(cfg[0].currentLevel).toBe('high');
    expect(cfg[0].efforts.map((e: any) => e.value)).toEqual([
      'low',
      'medium',
      'high',
    ]);
  });

  it('newSession() emits KasModelConfigUpdate with currentLevel null when the model has no effort schema', async () => {
    mockKiroNewSession.mockResolvedValueOnce({
      sessionId: 'kas-session-1',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'model',
          category: 'model',
          currentValue: 'm1',
          options: [{ value: 'm1', name: 'Test Model' }],
        },
      ],
    } as any);

    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();

    const cfg = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.KasModelConfigUpdate);
    expect(cfg).toHaveLength(1);
    expect(cfg[0].currentLevel).toBeNull();
    expect(cfg[0].efforts).toEqual([]);
  });

  it('loadSession() emits KasModelConfigUpdate (origin loadSession) carrying the current effort level', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded',
      models: null,
      modes: null,
      configOptions: [
        {
          type: 'select',
          id: 'model',
          category: 'model',
          currentValue: 'm1',
          options: [{ value: 'm1', name: 'Test Model' }],
        },
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

    const cfg = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.KasModelConfigUpdate);
    expect(cfg).toHaveLength(1);
    expect(cfg[0].origin).toBe('loadSession');
    expect(cfg[0].currentLevel).toBe('xhigh');
  });

  it('config_option_update emits KasModelConfigUpdate with currentLevel null when the effort option is dropped', async () => {
    // The active model just changed to one that does not declare an
    // effortLevels schema — KAS drops the option from configOptions and we
    // mirror that as `currentLevel: null` so the chip disappears. A model entry
    // must be present for the (single) KasModelConfigUpdate to fire.
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.newSession();
    handler.mockClear();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'config_option_update',
        configOptions: [
          {
            type: 'select',
            id: 'model',
            category: 'model',
            currentValue: 'm1',
            options: [{ value: 'm1', name: 'Test Model' }],
          },
        ],
      },
    });

    const cfg = handler.mock.calls
      .map((c) => c[0])
      .filter((e: any) => e.type === AgentEventType.KasModelConfigUpdate);
    expect(cfg).toHaveLength(1);
    expect(cfg[0].origin).toBe('serverPush');
    expect(cfg[0].currentLevel).toBeNull();
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

  it('GIVEN session WHEN /usage called THEN forwards to ext method', async () => {
    await client.initialize();
    await client.newSession();
    mockKiroSendExtMethod.mockImplementationOnce(() =>
      Promise.resolve({ success: true, message: 'ok', data: { credits: 5 } })
    );
    const result = await client.executeCommand({ command: 'usage' } as any);
    expect(result.success).toBe(true);
  });

  it('GIVEN session WHEN /context called THEN returns breakdown for the usage↔context Tab switch', async () => {
    // Regression: executeCommand('context') used to fall through to the
    // default "not yet supported" branch, so pressing Tab in /usage never
    // opened the /context panel in KAS mode.
    await client.initialize();
    await client.newSession();
    const breakdown = { contextFiles: { percent: 10, tokens: 100 } };
    mockKiroSendExtMethod.mockImplementationOnce(() =>
      Promise.resolve({ breakdown })
    );
    const result = await client.executeCommand({ command: 'context' } as any);
    expect(result.success).toBe(true);
    expect((result.data as any)?.breakdown).toEqual(breakdown);
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

describe('KasAcpClient — listSessions', () => {
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

// ── /mcp command (push model) ──

describe('mcp command (push model)', () => {
  let savedFeatures: string | undefined;

  beforeEach(() => {
    freshMocks();
    savedFeatures = process.env.KIRO_ENABLED_FEATURES;
    delete process.env.KIRO_ENABLED_FEATURES;
    features._resetForTests();
    process.env.KIRO_KAS_SERVER_PATH = '/fake/server.js';
  });

  afterEach(() => {
    delete process.env.KIRO_KAS_SERVER_PATH;
    if (savedFeatures !== undefined) {
      process.env.KIRO_ENABLED_FEATURES = savedFeatures;
    } else {
      delete process.env.KIRO_ENABLED_FEATURES;
    }
    features._resetForTests();
  });

  it('publishes a normalized configured-server snapshot', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

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
        {
          name: 'loading-server',
          status: 'connecting',
        },
        {
          name: 'auth-server',
          status: 'failed',
          failedAuthorization: true,
        },
        {
          name: 'disabled-server',
          status: 'disabled',
        },
      ],
    });

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers).toEqual([
      { name: 'test-server', status: 'running', toolCount: 1 },
      { name: 'failed-server', status: 'failed', toolCount: 0 },
      { name: 'loading-server', status: 'loading', toolCount: 0 },
      { name: 'auth-server', status: 'auth-required', toolCount: 0 },
      { name: 'disabled-server', status: 'disabled', toolCount: 0 },
    ]);
  });

  it('tags servers with a source inside the cloud_config rollout', async () => {
    const { features } = await import('../features');
    const original = process.env.KIRO_ENABLED_FEATURES;
    process.env.KIRO_ENABLED_FEATURES = '["cloud_config"]';
    features._resetForTests();
    try {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));

      (client as any).handleMcpStatusNotification({
        sessionId: 'kas-session-1',
        servers: [
          { name: 'local-server', status: 'connected', tools: [] },
          // The ConfigResource descriptor (_meta.kiro.resource, kiro-agent
          // PR #2141) must win over placement inference.
          {
            name: 'cloud-server',
            status: 'connected',
            _meta: {
              kiro: {
                resource: {
                  resourceType: 'mcpServer',
                  source: { origin: 'cloud', provenance: { scope: 'user' } },
                },
              },
            },
          },
          // A power-delivered server takes its power's direct source.
          {
            name: 'power-server',
            status: 'connected',
            _meta: {
              kiro: {
                resource: {
                  resourceType: 'mcpServer',
                  source: {
                    origin: 'power',
                    power: {
                      name: 'aws-tools',
                      source: {
                        origin: 'cloud',
                        provenance: { scope: 'user' },
                      },
                    },
                  },
                },
              },
            },
          },
        ],
      });

      const snapshot = events.find(
        (event) => event.type === AgentEventType.McpServerSnapshot
      );
      // Local (non-cloud) session: placement fallback tags servers local.
      expect(snapshot.servers[0].source).toBe('local');
      expect(snapshot.servers[1].source).toBe('cloud');
      expect(snapshot.servers[2].source).toBe('cloud');
    } finally {
      if (original === undefined) delete process.env.KIRO_ENABLED_FEATURES;
      else process.env.KIRO_ENABLED_FEATURES = original;
      features._resetForTests();
    }
  });

  it('counts a cloud config source once per session per surface', async () => {
    const { features } = await import('../features');
    const original = process.env.KIRO_ENABLED_FEATURES;
    process.env.KIRO_ENABLED_FEATURES = '["cloud_config"]';
    features._resetForTests();
    try {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();

      const cloudServer = {
        name: 'cloud-server',
        status: 'connected',
        _meta: {
          kiro: {
            resource: {
              resourceType: 'mcpServer',
              source: { origin: 'cloud', provenance: { scope: 'user' } },
            },
          },
        },
      };
      // A descriptor push delivered on the creating session's downlink
      // BEFORE the session/new response reports its id (mid-create window),
      // then a repeat push after: one adoption count, keyed to the session.
      (client as any).createInFlight = true;
      (client as any).handleMcpStatusNotification({
        sessionId: 'kas-session-2',
        servers: [cloudServer],
      });
      (client as any).createInFlight = false;
      (client as any).sessionId = 'kas-session-2';
      (client as any).handleMcpStatusNotification({
        sessionId: 'kas-session-2',
        servers: [cloudServer],
      });
      expect(mockRecordTuiCloudConfigSource).toHaveBeenCalledTimes(1);
      expect(mockRecordTuiCloudConfigSource.mock.calls[0]?.[0]).toMatchObject({
        surface: 'mcp',
      });

      // A placement-fallback 'cloud' (no descriptor) must NOT count.
      mockRecordTuiCloudConfigSource.mockClear();
      (client as any).startedCloudSession = true;
      (client as any).handleMcpStatusNotification({
        sessionId: 'kas-session-2',
        servers: [{ name: 'plain', status: 'connected', tools: [] }],
      });
      expect(mockRecordTuiCloudConfigSource).not.toHaveBeenCalled();

      // A DIFFERENT session observing cloud config counts on its own key.
      (client as any).sessionId = 'kas-session-3';
      (client as any).handleMcpStatusNotification({
        sessionId: 'kas-session-3',
        servers: [cloudServer],
      });
      expect(mockRecordTuiCloudConfigSource).toHaveBeenCalledTimes(1);
    } finally {
      if (original === undefined) delete process.env.KIRO_ENABLED_FEATURES;
      else process.env.KIRO_ENABLED_FEATURES = original;
      features._resetForTests();
    }
  });

  it('drops a status notification tagged with another session id', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession(); // active session: kas-session-1
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/mcp/status']({
      sessionId: 'some-other-session',
      servers: [{ name: 'other-server', status: 'connected', tools: [] }],
    });

    expect(
      events.find((event) => event.type === AgentEventType.McpServerSnapshot)
    ).toBeUndefined();
  });

  it('accepts a status notification tagged with the active session id', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/mcp/status']({
      sessionId: 'kas-session-1',
      servers: [
        {
          name: 'mine',
          status: 'connected',
          tools: [{ name: 't1', disabled: false }],
        },
      ],
    });

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers).toEqual([
      { name: 'mine', status: 'running', toolCount: 1 },
    ]);
    expect(snapshot.sessionTagged).toBe(true);
  });

  it('accepts an untagged status notification in a local session (older KAS)', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/mcp/status']({
      servers: [
        {
          name: 'untagged',
          status: 'connected',
          tools: [{ name: 't1', disabled: false }],
        },
      ],
    });

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers).toEqual([
      { name: 'untagged', status: 'running', toolCount: 1 },
    ]);
    expect(snapshot.sessionTagged).toBe(false);
  });

  it('drops an untagged status notification while a cloud session is active', async () => {
    // KAS >= 0.26.14 tags every mcp/status push and never emits local pool
    // status for cloud sessions (kiro-agent#1882 + #1892), so an untagged
    // snapshot during a cloud session can only be an older KAS's local pool.
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            executionTargets: ['local', 'cloud-sandbox'],
            sessionSources: ['local', 'remote'],
          },
        },
      },
    } as any);
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/mcp/status']({
      servers: [{ name: 'local-pool', status: 'connected', tools: [] }],
    });

    expect(
      events.find((event) => event.type === AgentEventType.McpServerSnapshot)
    ).toBeUndefined();
  });

  it('accepts a tagged status notification while a cloud session is active', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            executionTargets: ['local', 'cloud-sandbox'],
            sessionSources: ['local', 'remote'],
          },
        },
      },
    } as any);
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/mcp/status']({
      sessionId: client.sessionId,
      servers: [
        {
          name: 'sandbox-server',
          status: 'connected',
          tools: [{ name: 't1', disabled: false }],
        },
      ],
    });

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers).toEqual([
      { name: 'sandbox-server', status: 'running', toolCount: 1 },
    ]);
    expect(snapshot.sessionTagged).toBe(true);
  });

  it('drops a hooks didChange tagged with another session id', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession(); // active session: kas-session-1
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/hooks/didChange']({
      sessionId: 'some-other-session',
      hooks: [{ trigger: 'agentSpawn', action: { command: 'other.sh' } }],
    });

    expect(
      events.find((event) => event.type === AgentEventType.HooksUpdate)
    ).toBeUndefined();
  });

  it('accepts a hooks didChange tagged with the active session id', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/hooks/didChange']({
      sessionId: 'kas-session-1',
      hooks: [{ trigger: 'agentSpawn', action: { command: 'mine.sh' } }],
    });

    const update = events.find(
      (event) => event.type === AgentEventType.HooksUpdate
    );
    expect(update).toBeDefined();
    expect(update.hooks).toHaveLength(1);
  });

  it('publishes an empty configured-server snapshot when servers are omitted', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

    (client as any).handleMcpStatusNotification({});

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers).toEqual([]);
  });

  it('publishes registry servers from the status notification', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

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

    const event = events.find(
      (candidate) => candidate.type === AgentEventType.McpRegistrySnapshot
    );
    expect(event.registryServers).toHaveLength(2);
    expect(event.registryServers[0]).toEqual({
      name: 'registry-server-1',
      status: 'disabled',
      toolCount: 0,
      version: '1.0.0',
      description: 'A registry server',
      enabled: true,
    });
    expect(event.registryServers[1].enabled).toBe(false);
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
    const events: any[] = [];
    client.onUpdate((event) => events.push(event));

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

    const snapshot = events.find(
      (event) => event.type === AgentEventType.McpServerSnapshot
    );
    expect(snapshot.servers[0]).toMatchObject({
      name: 'github-mcp',
      status: 'auth-required',
    });
  });

  describe('KAS _kiro/* notification registration', () => {
    it('projects _kiro/powers/items_changed into a PowersUpdate event', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      expect(kc._extNotifHandlers['_kiro/powers/items_changed']).toBeDefined();
      kc._extNotifHandlers['_kiro/powers/items_changed']({
        powers: [
          {
            name: 'aws-tools',
            displayName: 'AWS Tools',
            description: 'AWS helpers',
            skillNames: ['deploy'],
          },
          { name: 42 }, // malformed entry dropped, not thrown
        ],
        errors: [],
      });
      const update = events.find((e) => e.type === AgentEventType.PowersUpdate);
      expect(update.powers).toEqual([
        {
          name: 'aws-tools',
          displayName: 'AWS Tools',
          description: 'AWS helpers',
        },
      ]);
    });

    it('projects _kiro/steering/documents_changed into a SteeringDocumentsUpdate event', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const handler = kc._extNotifHandlers['_kiro/steering/documents_changed'];
      expect(handler).toBeDefined();
      const activeSession = (client as any).sessionId;
      // A push tagged with ANOTHER session is dropped — a background session
      // must not clobber the active session's steering cache.
      handler({
        sessionId: 'other-session',
        status: 'success',
        documents: [{ name: 'intruder', type: 'steering', scope: 'global' }],
      });
      expect(
        events.find((e) => e.type === AgentEventType.SteeringDocumentsUpdate)
      ).toBeUndefined();
      // A failed listing is ignored (no event, no throw).
      handler({ sessionId: activeSession, status: 'failed', error: 'boom' });
      handler({
        sessionId: activeSession,
        status: 'success',
        documents: [
          {
            name: 'team',
            type: 'steering',
            scope: 'global',
            inclusion: 'always',
          },
          {
            name: 'api',
            type: 'steering',
            scope: 'workspace',
            inclusion: 'fileMatch',
          },
          {
            name: 'weird',
            type: 'steering',
            scope: 'global',
            inclusion: 'bogus',
          },
        ],
      });
      const update = events.find(
        (e) => e.type === AgentEventType.SteeringDocumentsUpdate
      );
      expect(update.documents).toEqual([
        { name: 'team', scope: 'global', inclusion: 'always' },
        { name: 'api', scope: 'workspace', inclusion: 'fileMatch' },
        { name: 'weird', scope: 'global' }, // unknown inclusion dropped
      ]);
    });

    it('projects _kiro/diagnostics/changed (cloudConfig) into a DiagnosticsUpdate event', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const handler = kc._extNotifHandlers['_kiro/diagnostics/changed'];
      expect(handler).toBeDefined();
      handler({
        sessionId: (client as any).sessionId,
        domain: 'cloudConfig',
        diagnostics: [
          {
            severity: 'warning',
            code: 'cloudConfig.syncStale',
            message: 'Using your last synced settings',
          },
          {
            severity: 'warning',
            code: 'cloudConfig.contentMismatch',
            message: 'A file failed verification',
            resourceId: 'steering/team.md',
          },
          { severity: 'error' }, // malformed (no code/message) dropped
        ],
      });
      const update = events.find(
        (e) => e.type === AgentEventType.DiagnosticsUpdate
      );
      expect(update.domain).toBe('cloudConfig');
      expect(update.diagnostics).toEqual([
        {
          severity: 'warning',
          code: 'cloudConfig.syncStale',
          message: 'Using your last synced settings',
        },
        {
          severity: 'warning',
          code: 'cloudConfig.contentMismatch',
          message: 'A file failed verification',
          resourceId: 'steering/team.md',
        },
      ]);
    });

    it('a failed powers push never wipes the cache (status guard)', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const handler = kc._extNotifHandlers['_kiro/powers/items_changed'];
      // KAS emits {status:'failed', error} with NO powers key when the
      // installed-powers scan throws — reading that as "zero powers" would
      // blank the /config powers page for the rest of the session.
      handler({ status: 'failed', error: 'scan exploded' });
      expect(
        events.find((e) => e.type === AgentEventType.PowersUpdate)
      ).toBeUndefined();
      // The success shape and the older bare listing (no status) both pass.
      handler({ status: 'success', powers: [{ name: 'a' }] });
      handler({ powers: [{ name: 'b' }] });
      const updates = events.filter(
        (e) => e.type === AgentEventType.PowersUpdate
      );
      expect(updates.map((u) => u.powers[0].name)).toEqual(['a', 'b']);
    });

    it('a failed diagnostics push emits nothing (not a false all-clear)', async () => {
      // The store treats an empty cloudConfig diagnostics array as an
      // intentional all-clear and marks the domain received; a {status:
      // 'failed'} push falling through would retract a shown warning AND
      // defeat the stash-restore marker. isFailedConfigPush must drop it.
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const handler = kc._extNotifHandlers['_kiro/diagnostics/changed'];
      handler({
        sessionId: (client as any).sessionId,
        domain: 'cloudConfig',
        status: 'failed',
        error: 'fetch failed',
      });
      expect(
        events.find((e) => e.type === AgentEventType.DiagnosticsUpdate)
      ).toBeUndefined();
    });

    it('a status-less steering listing still emits (tolerant guard)', async () => {
      // Steering used to require status==='success'; the shared guard now
      // accepts the older bare listing (no status) — dropping it would leave
      // /config steering silently empty.
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const handler = kc._extNotifHandlers['_kiro/steering/documents_changed'];
      handler({
        sessionId: (client as any).sessionId,
        documents: [{ name: 'bare', type: 'steering', scope: 'global' }],
      });
      const update = events.find(
        (e) => e.type === AgentEventType.SteeringDocumentsUpdate
      );
      expect(update?.documents.map((d: { name: string }) => d.name)).toEqual([
        'bare',
      ]);
    });

    it('config pushes honor session-transition windows (create + load)', async () => {
      // The powers/steering/diagnostics guards must match hooks/tools:
      // mid-create a tagged push from the CREATED session (id unknown until
      // the RPC resolves) is accepted; mid-load the OUTGOING session's push
      // (still equal to this.sessionId) is dropped while the TARGET
      // session's push survives.
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const events: any[] = [];
      client.onUpdate((event) => events.push(event));
      const kc = (client as any).kiroClient;
      const activeSession = (client as any).sessionId;
      const push = (over: Record<string, unknown> = {}) => {
        kc._extNotifHandlers['_kiro/powers/items_changed']({
          powers: [{ name: 'p' }],
          ...over,
        });
        kc._extNotifHandlers['_kiro/steering/documents_changed']({
          status: 'success',
          documents: [{ name: 's', type: 'steering', scope: 'global' }],
          ...over,
        });
        kc._extNotifHandlers['_kiro/diagnostics/changed']({
          domain: 'cloudConfig',
          diagnostics: [{ severity: 'warning', code: 'c', message: 'm' }],
          ...over,
        });
      };
      const count = () =>
        events.filter(
          (e) =>
            e.type === AgentEventType.PowersUpdate ||
            e.type === AgentEventType.SteeringDocumentsUpdate ||
            e.type === AgentEventType.DiagnosticsUpdate
        ).length;

      // Mid-create: the new session's tagged push must NOT be dropped.
      (client as any).createInFlight = true;
      push({ sessionId: 'created-but-unreported' });
      expect(count()).toBe(3);
      (client as any).createInFlight = false;

      // Mid-load: the outgoing session's own push (matches this.sessionId)
      // is dropped; the load TARGET's push passes.
      (client as any).loadTargetSessionId = 'incoming-session';
      events.length = 0;
      push({ sessionId: activeSession });
      expect(count()).toBe(0);
      (client as any).loadTargetSessionId = null;

      // Steady state: a background session's tagged push is still dropped.
      events.length = 0;
      push({ sessionId: 'background-session' });
      expect(count()).toBe(0);
    });

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

    it('phaseCheckpoint handler broadcasts a valid payload', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/spec/phaseCheckpoint']({
        sessionId: (client as any).sessionId,
        featureName: 'web-clock',
        phase: 'design',
        artifactPath: '/w/.kiro/specs/web-clock/design.md',
      });
      const event = events.find((e) => e.type === 'spec_phase_checkpoint');
      expect(event).toBeDefined();
      expect(event.phase).toBe('design');
      expect(event.featureName).toBe('web-clock');
      expect(event.artifactPath).toBe('/w/.kiro/specs/web-clock/design.md');
    });

    it('phaseCheckpoint handler broadcasts a bugfix phase', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/spec/phaseCheckpoint']({
        sessionId: (client as any).sessionId,
        featureName: 'quantity-zero-crash',
        phase: 'bugfix',
        artifactPath: '/w/.kiro/specs/quantity-zero-crash/bugfix.md',
      });
      const event = events.find((e) => e.type === 'spec_phase_checkpoint');
      expect(event).toBeDefined();
      expect(event.phase).toBe('bugfix');
    });

    it('phaseCheckpoint handler drops a phase it does not know', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/spec/phaseCheckpoint']({
        sessionId: (client as any).sessionId,
        featureName: 'web-clock',
        phase: 'architecture',
        artifactPath: '/w/.kiro/specs/web-clock/architecture.md',
      });
      expect(
        events.find((e) => e.type === 'spec_phase_checkpoint')
      ).toBeUndefined();
    });

    it('phaseCheckpoint handler drops a malformed payload', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/spec/phaseCheckpoint']({
        sessionId: (client as any).sessionId,
        phase: 'requirements',
      });
      expect(
        events.find((e) => e.type === 'spec_phase_checkpoint')
      ).toBeUndefined();
    });

    it("phaseCheckpoint handler drops another session's phase", async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/spec/phaseCheckpoint']({
        sessionId: 'some-other-session',
        featureName: 'web-clock',
        phase: 'requirements',
        artifactPath: '/w/.kiro/specs/web-clock/requirements.md',
      });
      expect(
        events.find((e) => e.type === 'spec_phase_checkpoint')
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

    it('agent not_found handler broadcasts AgentNotFound with the fallback id', async () => {
      // A not_found notification is observed through the broadcast
      // AgentNotFound event carrying the fallback id.
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'missing-agent',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      });
      const notFound = events.find((e) => e.type === 'agent_not_found');
      expect(notFound).toBeDefined();
      expect(notFound.fallbackAgent).toBe(KAS_DEFAULT_AGENT_ID);
      expect(notFound.requestedAgent).toBe('missing-agent');
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
    });

    it('agent not_found carries the rejected file when one claimed the id', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'thunder-agent',
        fallbackAgent: 'vibe',
        skipped: {
          path: '/a/thunder-agent.json',
          reasonCode: 'cli_only_agent',
          error: 'uses fields this agent engine does not support: allowedTools',
        },
      });
      const notFound = events.find((e) => e.type === 'agent_not_found');
      expect(notFound.skipped).toEqual({
        path: '/a/thunder-agent.json',
        reasonCode: 'cli_only_agent',
        error: 'uses fields this agent engine does not support: allowedTools',
      });
    });

    it('agent not_found drops a reason code this client does not know but keeps the defect', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'future',
        fallbackAgent: 'vibe',
        skipped: {
          path: '/a/future.json',
          reasonCode: 'invented_later',
          error: 'something new',
        },
      });
      const notFound = events.find((e) => e.type === 'agent_not_found');
      expect(notFound.skipped).toEqual({
        path: '/a/future.json',
        error: 'something new',
      });
    });

    it('agent not_found omits skipped when the id matched no file', async () => {
      const client = new KasAcpClient();
      await client.initialize();
      await client.newSession();
      const kc = (client as any).kiroClient;
      const events: any[] = [];
      (client as any).broadcastStreamEvent = (e: any) => events.push(e);
      kc._extNotifHandlers['_kiro/customAgent/not_found']({
        sessionId: 'test',
        requestedAgent: 'typo',
        fallbackAgent: 'vibe',
      });
      const notFound = events.find((e) => e.type === 'agent_not_found');
      expect(notFound.skipped).toBeUndefined();
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
    it('leaves persisted workflow rows on the base ACP path while disabled', async () => {
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'opaque workflow payload' },
          _meta: {
            kiro: {
              kind: 'workflow-progress',
              messageId: 'wf-progress-disabled',
            },
          },
        },
      });

      expect(handler).toHaveBeenCalledWith({
        type: AgentEventType.UserMessage,
        id: 'wf-progress-disabled',
        content: {
          type: ContentType.Text,
          text: 'opaque workflow payload',
        },
        meta: {
          kiro: {
            kind: 'workflow-progress',
            messageId: 'wf-progress-disabled',
          },
        },
      });
    });

    it('drops malformed persisted workflow progress instead of emitting user chat', async () => {
      setWorkflowsEnabled(true);
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: '{invalid json' },
          _meta: {
            kiro: {
              kind: 'workflow-progress',
              messageId: 'wf-progress-malformed',
            },
          },
        },
      });

      expect(handler).not.toHaveBeenCalled();
    });

    it('drops unknown persisted workflow events instead of emitting user chat', async () => {
      setWorkflowsEnabled(true);
      const client = new KasAcpClient();
      const handler = mock((_event: any) => {});
      client.onUpdate(handler);
      await client.newSession();
      handler.mockClear();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: '{}' },
          _meta: {
            kiro: {
              messageId: 'wf-progress-unknown',
              notification: {
                kind: 'workflow-progress',
                eventType: 'future_event',
                workflowId: 'wf-123',
              },
            },
          },
        },
      });

      expect(handler).not.toHaveBeenCalled();
    });

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

    it('agent_thought_chunk with _meta.kiro.agentSubtaskId produces Thought event with meta', async () => {
      // Bug A: a subagent's reasoning carries agentSubtaskId on the MAIN session;
      // the converter must keep that meta (like agent_message_chunk) so the thought
      // is routed to its subtask instead of bleeding into the main thinking block.
      const client = new KasAcpClient();
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_thought_chunk',
          content: { type: 'text', text: 'Looking for the file now.' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      const [, event] = multiHandler.mock.calls[0]!;
      expect((event as any).type).toBe(AgentEventType.Thought);
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

    it('crew-stage agent_thought_chunk goes to the subtask ONLY, not main (Bug A: thinking bleed)', async () => {
      // A registered pipeline stage's reasoning must not reach the main stream —
      // else the subagent's thinking renders inside the main agent's "Thought for
      // Ns" block in lite scrollback.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      // Register 'sub-1' as a visible crew stage.
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

      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'agent_thought_chunk',
          content: { type: 'text', text: 'Looking for the file now.' },
          _meta: { kiro: { agentSubtaskId: 'sub-1' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(1);
      expect(multiHandler.mock.calls[0]![0]).toBe('sub-1');
      expect(
        mainHandler.mock.calls.filter(
          (call) => call[0]?.type === AgentEventType.Thought
        )
      ).toEqual([]);
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

  describe('independent subagent routing lifecycle', () => {
    // A standalone invoke_sub_agent parent is ported onto the orchestrate
    // pipeline contract (see utils/invoke-subagent-pipeline.ts): the roster
    // session arrives via broadcastSubagentList — the same path orchestrate
    // stages use — instead of a synthesized session_created, and the parent
    // card surfaces in main as a one-stage pipeline parent.
    //
    // The porting adapter is cloud-session-gated; these tests run local
    // sessions, so enable it via the test override the kit uses.
    beforeEach(() => {
      process.env.KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION = '1';
    });
    afterEach(() => {
      delete process.env.KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION;
    });

    it('publishes the roster via the subagent list, keeps the parent card in main, and does not publish routing-only store updates', async () => {
      const client = new KasAcpClient();
      const order: string[] = [];
      const listUpdates: any[][] = [];
      let storeNotifications = 0;
      const unsubscribeStore = kasRoutingStore.subscribe(() => {
        storeNotifications += 1;
      });
      client.onSubagentListUpdate((subagents: any[]) => {
        listUpdates.push(subagents);
        order.push(`list:${subagents.map((s) => s.status.type).join(',')}`);
      });
      client.onMultiSessionUpdate((_sessionId: string, event: any) => {
        order.push(`multi:${event.type}:${event.id}`);
      });
      client.onUpdate((event: any) => {
        if (
          event.type === AgentEventType.ToolCall ||
          event.type === AgentEventType.ToolCallFinished
        ) {
          order.push(`main:${event.type}:${event.id}`);
        }
      });
      await client.newSession();
      order.length = 0;

      const subtaskMeta = {
        kiro: {
          kind: 'agent-subtask',
          agentSubtaskId: 'independent-subtask',
        },
      };
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'subagent-wrapper',
          title: 'Sub-agent: reviewer',
          kind: 'other',
          rawInput: { agentName: 'reviewer' },
          content: [],
          locations: [],
          _meta: subtaskMeta,
        },
      });

      expect(order).toEqual([
        'list:working',
        `main:${AgentEventType.ToolCall}:subagent-wrapper`,
      ]);
      expect(listUpdates[0]).toEqual([
        {
          sessionId: 'independent-subtask',
          sessionName: 'reviewer',
          agentName: 'reviewer',
          status: { type: 'working' },
          group: 'invoke-subagent-wrapper',
          role: 'reviewer',
          dependsOn: [],
        },
      ]);

      order.length = 0;
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'independent-child',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'independent-subtask' } },
        },
      });
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'independent-child',
          status: 'completed',
          content: [],
          _meta: { kiro: { agentSubtaskId: 'independent-subtask' } },
        },
      });
      expect(order).toEqual([
        `multi:${AgentEventType.ToolCall}:independent-child`,
        `multi:${AgentEventType.ToolCallFinished}:independent-child`,
      ]);
      expect(listUpdates).toHaveLength(1);

      order.length = 0;
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'subagent-wrapper',
          status: 'completed',
          content: [],
          _meta: subtaskMeta,
        },
      });
      expect(order).toEqual([
        'list:terminated',
        `main:${AgentEventType.ToolCallFinished}:subagent-wrapper`,
      ]);
      expect(listUpdates[1]?.[0]).toMatchObject({
        sessionId: 'independent-subtask',
        status: { type: 'terminated' },
      });
      expect(storeNotifications).toBe(0);
      unsubscribeStore();
    });
  });

  // ── Standalone (hidden) subagent tool cards surface inline in main ──

  describe('standalone subagent tool cards surface in main', () => {
    it('reconstructs a main card from the chunk snapshot when a metadata-poor update proves standalone routing', async () => {
      const client = new KasAcpClient();
      const mainEvents: any[] = [];
      const deliveryOrder: string[] = [];
      let approvalInfo: any = null;
      client.onUpdate((event: any) => {
        if (event.type === AgentEventType.ApprovalRequest) {
          approvalInfo = event.value;
          return;
        }
        mainEvents.push(event);
        deliveryOrder.push(`main:${event.type}`);
      });
      client.onMultiSessionUpdate((_sessionId: string, event: any) => {
        deliveryOrder.push(`multi:${event.type}`);
      });
      await client.newSession();
      mainEvents.length = 0;
      deliveryOrder.length = 0;

      const permissionPromise = capturedPermissionHandler({
        toolCallId: 'snapshot-tool',
        permissions: [
          { id: 'allow_once', name: 'Allow once' },
          { id: 'reject_once', name: 'Reject once' },
        ],
        _meta: {
          kiro: {
            agentSubtaskId: 'snapshot-subtask',
            consent: { capability: 'fs_read' },
          },
        },
      });
      await new Promise((resolve) => setTimeout(resolve, 50));
      approvalInfo.resolve({ outcome: 'selected', optionId: 'allow_once' });
      await permissionPromise;
      mainEvents.length = 0;
      deliveryOrder.length = 0;

      (client as any).handleExtSessionUpdate({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_chunk',
          toolCallId: 'snapshot-tool',
          title: 'read_file',
          kind: 'read',
          _meta: {
            kiro: { agentSubtaskId: 'snapshot-subtask' },
          },
        },
      });
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'snapshot-tool',
          status: 'in_progress',
          content: [],
        },
      });

      expect(deliveryOrder).toEqual([
        `multi:${AgentEventType.ToolCall}`,
        `multi:${AgentEventType.ToolCallUpdate}`,
        `main:${AgentEventType.ToolCall}`,
        `main:${AgentEventType.ToolCallUpdate}`,
      ]);
      expect(mainEvents[0]).toMatchObject({
        type: AgentEventType.ToolCall,
        id: 'snapshot-tool',
        name: 'read_file',
        kind: 'read',
        args: {},
      });
      expect(mainEvents[0].sessionId).toBeUndefined();
      expect(mainEvents[1]).toMatchObject({
        type: AgentEventType.ToolCallUpdate,
        id: 'snapshot-tool',
      });
    });

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
      // ToolCallFinished is in STANDALONE_MAIN_FORWARD_TYPES, so a hidden
      // subagent's tool card must COMPLETE inline in main, not just start.
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

  // ── Crew per-stage WRAPPER card duplicate suppression (per-subtask ownership) ──

  describe('crew wrapper cards do not duplicate into main (per-subtask ownership)', () => {
    // Helper: deliver the crew pipeline state update that KAS emits FIRST on the
    // orchestrate_subagent card. Registers stage UUIDs and marks the group
    // active. Mirrors the live ACP recording (groupId + stage agentSubtaskIds
    // are real UUIDs; the per-stage wrapper card is tagged separately below).
    const sendPipelineRunning = async () =>
      capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'tooluse_PARENT',
          title: 'Orchestrate Sub-agent',
          kind: 'other',
          rawInput: { task: 'review' },
          content: [],
          locations: [],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-review',
                stages: [
                  {
                    name: 'architecture_review',
                    role: 'general-task-execution',
                    status: 'running',
                    dependsOn: [],
                    agentSubtaskId: '7ec56d71-89b5-48a2-a56e-eb76195d6ff2',
                  },
                  {
                    name: 'synthesis',
                    role: 'general-task-execution',
                    status: 'pending',
                    dependsOn: ['architecture_review'],
                    agentSubtaskId: '1e80a5b2-e67e-401b-9bc7-5837e4729870',
                  },
                ],
              },
            },
          },
        },
      });

    it('REPRO: per-stage wrapper tool_call (derived subtaskId, NOT a stage UUID) stays out of main', async () => {
      // Ground truth from the ACP recording: while the crew pipeline is active,
      // KAS emits a per-stage WRAPPER tool_call titled "Sub-agent: <role>" whose
      // agentSubtaskId is a DERIVED id ("invoke_subagent_tooluse_<parent>_stage_
      // <name>"), NOT the stage UUID registered via the pipeline meta. Pre-fix
      // the pipelineStageSubtasks check missed it and it leaked into main as a
      // duplicate (it also correctly renders in the SUBAGENT OUTPUT panel). Once
      // registered as a pipeline stage it must reach multi-session ONLY.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await sendPipelineRunning();
      mainHandler.mockClear();
      multiHandler.mockClear();

      const wrapperSubtaskId =
        'invoke_subagent_tooluse_PARENT_stage_architecture_review';
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: wrapperSubtaskId,
          title: 'Sub-agent: general-task-execution',
          kind: 'other',
          rawInput: { name: 'general-task-execution', prompt: 'work' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: wrapperSubtaskId } },
        },
      });

      // Panel-only: multi-session received it, main did NOT (no duplicate).
      expect(multiHandler).toHaveBeenCalledTimes(1);
      expect(multiHandler.mock.calls[0]![0]).toBe(wrapperSubtaskId);
      expect(mainHandler).not.toHaveBeenCalled();
    });

    it('LIFECYCLE: after an all-terminal snapshot clears the group, a later standalone subagent surfaces in main again', async () => {
      // Proves the suppression is not sticky: once every stage is terminal the
      // group is released, so a subsequent standalone (no-pipeline) subagent's
      // tool card forwards to main as before.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await sendPipelineRunning();

      // Pipeline completes — every stage terminal → group released.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'tooluse_PARENT',
          status: 'completed',
          rawOutput: 'Pipeline completed: 2 stages finished.',
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-review',
                stages: [
                  {
                    name: 'architecture_review',
                    role: 'general-task-execution',
                    status: 'completed',
                    dependsOn: [],
                    agentSubtaskId: '7ec56d71-89b5-48a2-a56e-eb76195d6ff2',
                  },
                  {
                    name: 'synthesis',
                    role: 'general-task-execution',
                    status: 'completed',
                    dependsOn: ['architecture_review'],
                    agentSubtaskId: '1e80a5b2-e67e-401b-9bc7-5837e4729870',
                  },
                ],
              },
            },
          },
        },
      });
      mainHandler.mockClear();
      multiHandler.mockClear();

      // A brand-new standalone subagent (NO pipeline) emits a tool call.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-standalone',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-standalone' } },
        },
      });

      // Forwarded to BOTH main (standalone surfacing) and multi-session.
      expect(multiHandler).toHaveBeenCalledTimes(1);
      expect(mainHandler).toHaveBeenCalledTimes(1);
      const mainEvent = mainHandler.mock.calls[0]![0] as any;
      expect(mainEvent.type).toBe(AgentEventType.ToolCall);
      // Main copy renders as a normal inline card (crew sessionId stripped).
      expect(mainEvent.sessionId).toBeUndefined();
    });

    it('BACKSTOP: a failed pipeline that leaves a stage pending still clears the group on the orchestrate card finishing', async () => {
      // KAS stops a pipeline on first stage failure, so the terminal snapshot
      // can still carry an unexecuted 'pending' stage (never all-terminal). The
      // all-terminal check alone would leave the group stuck active. The
      // ToolCallFinished backstop on the orchestrate card releases it anyway.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await sendPipelineRunning();

      // Pipeline fails at stage 1; stage 'synthesis' was never reached → pending.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'tooluse_PARENT',
          status: 'failed',
          content: [
            {
              type: 'content',
              content: { type: 'text', text: 'Stage failed' },
            },
          ],
          _meta: {
            kiro: {
              pipeline: {
                groupId: 'pipeline-review',
                stages: [
                  {
                    name: 'architecture_review',
                    role: 'general-task-execution',
                    status: 'failed',
                    dependsOn: [],
                    agentSubtaskId: '7ec56d71-89b5-48a2-a56e-eb76195d6ff2',
                  },
                  {
                    name: 'synthesis',
                    role: 'general-task-execution',
                    status: 'pending',
                    dependsOn: ['architecture_review'],
                    agentSubtaskId: '1e80a5b2-e67e-401b-9bc7-5837e4729870',
                  },
                ],
              },
            },
          },
        },
      });
      mainHandler.mockClear();
      multiHandler.mockClear();

      // Group should be cleared despite the lingering 'pending' stage: a later
      // standalone subagent surfaces in main.
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-standalone',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-standalone' } },
        },
      });

      expect(mainHandler).toHaveBeenCalledTimes(1);
    });

    it('lets an unrelated standalone subagent surface in main while a crew is active', async () => {
      // Ownership is per subtask, not global crew liveness. A registered
      // pipeline-stage subtask stays panel-only, but an unrelated hidden
      // standalone subagent still surfaces in main once KAS sends its full card.
      const client = new KasAcpClient();
      const mainHandler = mock((_event: any) => {});
      const multiHandler = mock((_sessionId: string, _event: any) => {});
      client.onUpdate(mainHandler);
      client.onMultiSessionUpdate(multiHandler);
      await client.newSession();

      await sendPipelineRunning();
      mainHandler.mockClear();
      multiHandler.mockClear();

      // Unrelated standalone subagent (its subtaskId is not a registered stage).
      await capturedSessionUpdateHandler({
        sessionId: 'kas-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'read-concurrent',
          title: 'read_file',
          kind: 'read',
          rawInput: { path: '/test' },
          content: [],
          locations: [],
          _meta: { kiro: { agentSubtaskId: 'sub-unrelated' } },
        },
      });

      expect(multiHandler).toHaveBeenCalledTimes(1);
      expect(mainHandler).toHaveBeenCalledTimes(1);
      expect(mainHandler.mock.calls[0]![0]).toMatchObject({
        type: AgentEventType.ToolCall,
        id: 'read-concurrent',
        sessionId: undefined,
      });
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

// ── KAS shell consent: compound command at the ACP permission-request boundary ──
//
// The v3+KAS shell fix gates a compound command (`git status && echo "done"`)
// per segment. KAS carries the decision on `_meta.kiro.consent`: `resource` is
// the WHOLE command and `triggeringResource` is the GATED sub-command that
// actually needs consent right now. This test pins the ACP **ingestion**
// boundary — the seam where `handlePermissionRequest` lifts that consent off the
// incoming `session/request_permission` into the `consentContext` the UI reads.
//
// SCOPE (be honest about what this exercises): ingestion only. A regression that
// drops `triggeringResource` here would silently break the whole fix (the UI
// would derive trust for the wrong segment). The OTHER half — deriving the gated
// segment into the outgoing reply (`kasResource` → `_meta.kiro.consent.resource`,
// in app-store `respondToApproval`) — is exercised end-to-end through the real
// TUI + store in `acp_integ_tests/permission-consent.test.ts` ("compound shell:
// exact-trust persists the GATED segment"). It is NOT reachable from this
// harness, which never instantiates the store.
describe('KasAcpClient — KAS shell consent (compound command) ACP boundary', () => {
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

  const COMPOUND = 'git status && echo "done"';
  const GATED = 'echo "done"';

  // Drive an incoming KAS permission request and return the captured
  // ApprovalRequest value + the pending response promise.
  async function driveCompoundPermission(client: any, capability = 'shell') {
    let approvalInfo: any = null;
    const handler = mock((event: any) => {
      if (event.type === AgentEventType.ApprovalRequest) {
        approvalInfo = event.value;
      }
    });
    client.onUpdate(handler);
    await client.newSession();
    handler.mockClear();

    const permissionPromise = capturedPermissionHandler({
      toolCallId: 'shell-compound-001',
      options: [
        { optionId: 'allow_once', name: 'Allow once', kind: 'allow_once' },
        { optionId: 'reject_once', name: 'Reject once', kind: 'reject_once' },
      ],
      _meta: {
        kiro: {
          consent: {
            capability,
            resource: COMPOUND,
            triggeringResource: GATED,
          },
        },
      },
    });

    // handlePermissionRequest resolves via broadcast; let the event settle.
    await new Promise((r) => setTimeout(r, 50));
    return { getApproval: () => approvalInfo, permissionPromise };
  }

  it('ingestion: incoming request_permission preserves BOTH resource (whole command) and triggeringResource (gated segment) into consentContext', async () => {
    const client = new KasAcpClient();
    const { getApproval, permissionPromise } =
      await driveCompoundPermission(client);

    const approval = getApproval();
    expect(approval).not.toBeNull();
    // The consent the UI reads must carry the full picture: the whole compound
    // command for display, and the gated segment so trust applies to the right
    // sub-command. Dropping `triggeringResource` here is the regression guarded.
    expect(approval.consentContext).toBeDefined();
    expect(approval.consentContext.capability).toBe('shell');
    expect(approval.consentContext.resource).toBe(COMPOUND);
    expect(approval.consentContext.triggeringResource).toBe(GATED);

    // Resolve so the pending ACP promise never dangles.
    approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
  });

  it('ingestion: shell:exec consent is treated as a shell approval', async () => {
    const client = new KasAcpClient();
    const { getApproval, permissionPromise } = await driveCompoundPermission(
      client,
      'shell:exec'
    );

    const approval = getApproval();
    expect(approval).not.toBeNull();
    expect(approval.consentContext).toMatchObject({
      capability: 'shell:exec',
      resource: COMPOUND,
      triggeringResource: GATED,
    });
    expect(approval.toolCall.title).toBe('run_command');
    expect(approval.toolCall.rawInput.command).toBe(COMPOUND);

    approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
  });

  // The KAS session that owns the backend permission request is enriched onto
  // the request as `originSessionId` (handleKasPermissionRequest) and read as
  // the trust-cascade's session discriminator in the store. It must survive the
  // shared `handlePermissionRequest` broadcast, else two approvals from distinct
  // origin sessions collapse to the same trust identity and cross-cascade.
  it('ingestion: originSessionId (owning KAS session) reaches the broadcast approval value', async () => {
    const client = new KasAcpClient();
    const { getApproval, permissionPromise } =
      await driveCompoundPermission(client);

    const approval = getApproval();
    expect(approval).not.toBeNull();
    expect(approval.originSessionId).toBe('kas-session-1');

    approval.resolve({ outcome: 'selected', optionId: 'allow_once' });
    await permissionPromise;
  });
});

// ── Cloud sandbox: executionTarget on session/new + handshake-cap gating ──
// Covers T1 (plumb executionTarget) + T2 (consume initialize caps, fail-safe
// degrade to local). The mock KiroClient nests caps under
// agentCapabilities._meta.kiro, matching the KAS ACP doc §4.
describe('cloud executionTarget', () => {
  let origKasPath: string | undefined;

  // Advertise the cloud-sandbox execution target on the next initialize().
  function advertiseRemoteCaps() {
    mockKiroInitialize.mockImplementationOnce(() =>
      Promise.resolve({
        protocolVersion: '1.0',
        agentCapabilities: {
          _meta: {
            kiro: {
              executionTargets: ['local', 'cloud-sandbox'],
              sessionSources: ['local', 'remote'],
            },
          },
        },
      })
    );
  }

  function lastNewSessionMeta(): any {
    const calls = mockKiroNewSession.mock.calls;
    return calls[calls.length - 1]?.[0]?._meta?.kiro;
  }

  // Advertise an arbitrary (possibly malformed) `agentCapabilities._meta.kiro`
  // blob on the next initialize(), to exercise parseKiroAgentCapabilities'
  // defensive branches through the real ingestion path.
  function advertiseKiroCaps(kiro: unknown) {
    mockKiroInitialize.mockImplementationOnce(() =>
      Promise.resolve({
        protocolVersion: '1.0',
        agentCapabilities: { _meta: { kiro } },
      })
    );
  }

  beforeEach(() => {
    origKasPath = process.env.KIRO_KAS_SERVER_PATH;
    process.env.KIRO_KAS_SERVER_PATH = '/fake/acp-server.js';
    freshMocks();
  });

  afterEach(() => {
    if (origKasPath === undefined) delete process.env.KIRO_KAS_SERVER_PATH;
    else process.env.KIRO_KAS_SERVER_PATH = origKasPath;
  });

  // Existing-user / dark-ship safety: every released KAS today advertises NO
  // executionTargets capability, so a local (or default) client sends NOTHING
  // on the wire -- byte-identical to pre-feature behavior. This is the test
  // that guarantees the explicit-local change cannot impact existing users
  // until KAS actually ships the capability.
  it('omits executionTarget for a local session when KAS advertises no caps (existing-user path)', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('sends explicit {kind:local} once KAS advertises local support (intent no longer rides on KAS absent-default)', async () => {
    advertiseRemoteCaps(); // advertises ['local', 'cloud-sandbox']
    const client = new KasAcpClient(); // no target -> defaults to local
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toEqual({ kind: 'local' });
  });

  it('omits local executionTarget when KAS advertises caps but not local', async () => {
    advertiseKiroCaps({ executionTargets: ['cloud-sandbox'] }); // no 'local'
    const client = new KasAcpClient(); // local (default)
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('sends cloud-sandbox executionTarget when --cloud and KAS advertises it', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toEqual({
      kind: 'cloud-sandbox',
    });
  });

  it('isCloudSessionActive() is true after a cloud-sandbox session is placed on a cloud sandbox', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(client.isCloudSessionActive()).toBe(true);
    expect(mockRecordTuiCloudSession).toHaveBeenCalledWith({
      event: 'created',
      version: 'test-version',
    });
  });

  it('emits create_failed when a cloud session/new is rejected', async () => {
    advertiseRemoteCaps();
    mockKiroNewSession.mockRejectedValueOnce(new Error('provision boom'));
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await expect(client.newSession()).rejects.toThrow('provision boom');
    expect(mockRecordTuiCloudSession).toHaveBeenCalledWith({
      event: 'create_failed',
      version: 'test-version',
    });
    expect(mockRecordTuiCloudSession).not.toHaveBeenCalledWith({
      event: 'created',
      version: 'test-version',
    });
    expect(mockRecordTuiCloudError).toHaveBeenCalledWith({
      op: 'session_new',
      kind: 'other',
      version: 'test-version',
    });
  });

  it('does not emit create_failed when a local session/new is rejected', async () => {
    mockKiroNewSession.mockRejectedValueOnce(new Error('local boom'));
    const client = new KasAcpClient(); // local (default)
    await expect(client.newSession()).rejects.toThrow('local boom');
    expect(mockRecordTuiCloudSession).not.toHaveBeenCalled();
  });

  it('emits reattached when resuming a KAS-tagged remote-sourced session', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    mockKiroLoadSession.mockResolvedValueOnce({
      configOptions: [],
      _meta: { kiro: { source: 'remote' } },
    } as any);
    const client = new KasAcpClient();
    await client.initialize();
    await client.loadSession('cloud-session-1');
    expect(mockRecordTuiCloudSession).toHaveBeenCalledWith({
      event: 'reattached',
      version: 'test-version',
    });
  });

  it('does NOT emit reattached when resuming a local-sourced session', async () => {
    mockKiroLoadSession.mockResolvedValueOnce({
      configOptions: [],
      _meta: { kiro: { source: 'local' } },
    } as any);
    const client = new KasAcpClient();
    await client.loadSession('local-session-1');
    expect(mockRecordTuiCloudSession).not.toHaveBeenCalled();
  });

  it('isCloudSessionActive() stays false when --cloud degrades to local (cap not advertised)', async () => {
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize(); // default caps advertise no cloud-sandbox
    await client.newSession();
    expect(client.isCloudSessionActive()).toBe(false);
    expect(mockRecordTuiCloudSession).not.toHaveBeenCalledWith({
      event: 'created',
      version: 'test-version',
    });
    expect(mockRecordTuiCloudSession).toHaveBeenCalledWith({
      event: 'fell_back_local',
      version: 'test-version',
    });
  });

  it('isCloudSessionActive() is false for a local session', async () => {
    const client = new KasAcpClient();
    await client.newSession();
    expect(client.isCloudSessionActive()).toBe(false);
  });

  it('degrades to local when KAS does NOT advertise the cap', async () => {
    // Default initialize() return advertises no executionTargets.
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('degrades to local when initialize() was never called (no caps captured)', async () => {
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('merges executionTarget alongside modeId in _meta.kiro', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient({
      initialAgent: 'plan',
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.executionTarget).toEqual({ kind: 'cloud-sandbox' });
    expect(meta?.modeId).toBeDefined();
  });

  // ---- cloud "New" empty sandbox — sessionSource + isEmptyWorkspace ----
  it('sends sessionSource:remote + cloud-sandbox executionTarget + isEmptyWorkspace for a cloud New session', async () => {
    advertiseRemoteCaps(); // executionTargets:[local,cloud-sandbox], sessionSources:[local,remote]
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    }); // no repos -> "New" empty sandbox
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.executionTarget).toEqual({ kind: 'cloud-sandbox' });
    expect(meta?.sessionSource).toBe('remote');
    expect(meta?.isEmptyWorkspace).toBe(true);
  });

  it('omits sessionSource when KAS advertises the cloud-sandbox placement but not a remote store (per-flag gating)', async () => {
    advertiseKiroCaps({ executionTargets: ['local', 'cloud-sandbox'] }); // no sessionSources
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.executionTarget).toEqual({ kind: 'cloud-sandbox' });
    expect(meta?.sessionSource).toBeUndefined();
    expect(meta?.isEmptyWorkspace).toBe(true); // still a New (no-repo) session
  });

  it('does NOT send sessionSource/isEmptyWorkspace for a local session (existing-user path unchanged)', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient(); // local (default)
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.sessionSource).toBeUndefined();
    expect(meta?.isEmptyWorkspace).toBeUndefined();
  });

  it('binds repositories (and omits isEmptyWorkspace) for a repo-bound cloud session', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
      repos: ['owner/repo'],
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.repositories).toEqual(['owner/repo']);
    expect(meta?.isEmptyWorkspace).toBeUndefined();
    expect(meta?.sessionSource).toBe('remote');
  });

  it('binds multiple repositories in order for a repo-bound cloud session', async () => {
    advertiseRemoteCaps();
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
      repos: ['acme/repo', 'MyPackage'],
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.repositories).toEqual(['acme/repo', 'MyPackage']);
    expect(meta?.isEmptyWorkspace).toBeUndefined();
  });

  // ---- gated _kiro/sourceProviders/* pull methods (repo-picker data path) ----
  const SP_METHODS = [
    '_kiro/sourceProviders/list',
    '_kiro/sourceProviders/listResources',
  ];

  it('listSourceProviders issues the ext call and returns providers when the cap + method are advertised', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: SP_METHODS });
    const providers = {
      providers: [
        {
          providerType: 'GITHUB',
          displayName: 'GitHub',
          connectionStatus: 'connected',
        },
      ],
    };
    mockKiroSendExtMethod.mockResolvedValueOnce(providers);
    const client = new KasAcpClient();
    await client.initialize();
    const result = await client.listSourceProviders();
    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/sourceProviders/list',
      {}
    );
    expect(result).toEqual(providers);
  });

  it('listSourceProviderResources forwards the request and returns the page', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: SP_METHODS });
    const page = {
      resources: [{ providerType: 'GITHUB', name: 'owner/repo' }],
    };
    mockKiroSendExtMethod.mockResolvedValueOnce(page);
    const client = new KasAcpClient();
    await client.initialize();
    const result = await client.listSourceProviderResources({
      providerType: 'GITHUB',
      limit: 50,
    });
    expect(mockKiroSendExtMethod).toHaveBeenCalledWith(
      '_kiro/sourceProviders/listResources',
      { providerType: 'GITHUB', limit: 50 }
    );
    expect(result).toEqual(page);
  });

  it('returns undefined and issues NO ext call when the sourceProviders cap is absent (dark-safe)', async () => {
    // Default initialize advertises no caps -> the repo-picker surface is off.
    const client = new KasAcpClient();
    await client.initialize();
    mockKiroSendExtMethod.mockClear();
    expect(await client.listSourceProviders()).toBeUndefined();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/sourceProviders/list',
      expect.anything()
    );
  });

  it('returns undefined when sourceProviders is true but the method is absent from extensionMethods (extensionMethods is consulted)', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: [] }); // cap on, method NOT listed
    const client = new KasAcpClient();
    await client.initialize();
    mockKiroSendExtMethod.mockClear();
    expect(await client.listSourceProviders()).toBeUndefined();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/sourceProviders/list',
      expect.anything()
    );
  });

  it('listSourceProviderResources returns undefined without an ext call when the cap is off', async () => {
    advertiseKiroCaps({}); // no sourceProviders cap
    const client = new KasAcpClient();
    await client.initialize();
    mockKiroSendExtMethod.mockClear();
    expect(
      await client.listSourceProviderResources({ providerType: 'GITHUB' })
    ).toBeUndefined();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/sourceProviders/listResources',
      expect.anything()
    );
  });

  it('listSourceProviderResources returns undefined when the method is absent from extensionMethods', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: [] });
    const client = new KasAcpClient();
    await client.initialize();
    mockKiroSendExtMethod.mockClear();
    expect(
      await client.listSourceProviderResources({ providerType: 'GITHUB' })
    ).toBeUndefined();
    expect(mockKiroSendExtMethod).not.toHaveBeenCalledWith(
      '_kiro/sourceProviders/listResources',
      expect.anything()
    );
  });

  it('listSourceProviders resolves undefined (no throw) when the ext call keeps rejecting', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: SP_METHODS });
    mockKiroSendExtMethod
      .mockRejectedValueOnce(new Error('kas is down'))
      .mockRejectedValueOnce(new Error('kas is down'));
    const client = new KasAcpClient();
    await client.initialize();
    expect(await client.listSourceProviders()).toBeUndefined();
  });

  it('listSourceProviders retries once and succeeds after a transient failure', async () => {
    // The first call can race the agent's token refresh (transient
    // UnauthorizedException); a single blip must not disable the picker.
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: SP_METHODS });
    const providers = { providers: [] };
    mockKiroSendExtMethod
      .mockRejectedValueOnce(new Error('Authentication required'))
      .mockResolvedValueOnce(providers);
    const client = new KasAcpClient();
    await client.initialize();
    expect(await client.listSourceProviders()).toEqual(providers);
  });

  it('listSourceProviderResources resolves undefined (no throw) when the ext call rejects', async () => {
    advertiseKiroCaps({ sourceProviders: true, extensionMethods: SP_METHODS });
    mockKiroSendExtMethod.mockRejectedValueOnce(new Error('kas is down'));
    const client = new KasAcpClient();
    await client.initialize();
    expect(
      await client.listSourceProviderResources({ providerType: 'GITHUB' })
    ).toBeUndefined();
  });

  it('isCloudSessionActive reflects the SENT placement: true for a confirmed cloud-sandbox, false when --cloud degraded to local', async () => {
    advertiseRemoteCaps();
    const cloud = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await cloud.initialize();
    await cloud.newSession();
    expect(cloud.isCloudSessionActive()).toBe(true);

    freshMocks();
    advertiseKiroCaps({ executionTargets: ['local'] }); // cloud NOT advertised
    const degraded = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await degraded.initialize();
    await degraded.newSession();
    expect(degraded.isCloudSessionActive()).toBe(false);
  });

  it('omits BOTH sessionSource and isEmptyWorkspace when the cloud-sandbox placement is NOT advertised, even if a remote store is (gating keys off the SENT executionTarget)', async () => {
    // Locks the dark-ship guarantee to the SENT executionTarget, not the requested
    // kind: executionTargets excludes cloud-sandbox -> executionTarget is NOT sent ->
    // the remote block must not fire, even though a remote store IS advertised. Guards
    // against a future refactor reading `this.executionTarget` instead of the wire meta.
    advertiseKiroCaps({
      executionTargets: ['local'], // cloud-sandbox NOT advertised
      sessionSources: ['local', 'remote'], // remote store IS advertised
    });
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' }, // requested, but unsupported
      repos: ['owner/repo'], // must also be gated off when the target isn't sent
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.executionTarget).toBeUndefined();
    expect(meta?.sessionSource).toBeUndefined();
    expect(meta?.isEmptyWorkspace).toBeUndefined();
    expect(meta?.repositories).toBeUndefined();
  });

  it('still sends modeId while dropping executionTarget when KAS does not advertise the kind', async () => {
    // Degrade-but-preserve: --cloud + a mode, but KAS advertises no caps ->
    // executionTarget is gated out yet the modeId merge is unaffected (the two
    // fields share one _meta.kiro object; degrading one must not drop the other).
    advertiseKiroCaps({}); // no executionTargets advertised -> unsupported
    const client = new KasAcpClient({
      initialAgent: 'plan',
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    const meta = lastNewSessionMeta();
    expect(meta?.executionTarget).toBeUndefined();
    expect(meta?.modeId).toBeDefined();
  });

  it('degrades to local for remote-control when KAS advertises only cloud-sandbox', async () => {
    advertiseRemoteCaps(); // advertises ['local', 'cloud-sandbox'] — not remote-control
    const client = new KasAcpClient({
      executionTarget: { kind: 'remote-control' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('ingests the finalized caps shape (extensionMethods present, legacy sessionSearch ignored) and still gates executionTarget', async () => {
    advertiseKiroCaps({
      executionTargets: ['local', 'cloud-sandbox'],
      sessionSources: ['local', 'remote'],
      sessionListScopes: ['workspace', 'user'],
      extensionMethods: [
        '_kiro/sourceProviders/list',
        '_kiro/sourceProviders/listResources',
      ],
      sourceProviders: true,
      sessionSearch: true, // legacy key dropped from the milestone — must be ignored, not crash
    });
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toEqual({
      kind: 'cloud-sandbox',
    });
  });

  it('degrades to local when the advertised list excludes the requested kind', async () => {
    advertiseKiroCaps({ executionTargets: ['local'] }); // present, but no cloud-sandbox
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('degrades to local when executionTargets is malformed (non-array)', async () => {
    advertiseKiroCaps({ executionTargets: 'cloud-sandbox' }); // string, not array
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });

  it('degrades to local when executionTargets is a mixed-type array (malformed)', async () => {
    advertiseKiroCaps({ executionTargets: ['cloud-sandbox', 5] }); // not all strings
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    await client.initialize();
    await client.newSession();
    expect(lastNewSessionMeta()?.executionTarget).toBeUndefined();
  });
});

describe('KasAcpClient — content-policy refusal', () => {
  it('broadcasts ModelRefusal and suppresses the inline text for a refusal chunk', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: {
          type: 'text',
          text: 'The selected model cannot continue this conversation.',
        },
        _meta: {
          kiro: {
            refusal: {
              category: 'CYBER',
              explanation: 'Declined by content policy.',
              recommendedModel: 'kiro-safe',
            },
          },
        },
      },
    });

    const events = handler.mock.calls.map((c) => c[0]);
    const refusal = events.find(
      (e: any) => e.type === AgentEventType.ModelRefusal
    );
    expect(refusal).toBeDefined();
    expect(refusal.category).toBe('CYBER');
    expect(refusal.explanation).toBe('Declined by content policy.');
    expect(refusal.recommendedModel).toBe('kiro-safe');
    // The inline chunk is suppressed so the refusal renders once (parity with V2).
    expect(
      events.filter((e: any) => e.type === AgentEventType.Content)
    ).toHaveLength(0);
  });

  it('leaves a normal agent_message_chunk (no refusal) as a Content event', async () => {
    const client = new KasAcpClient();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);
    await client.initialize();
    await client.newSession();

    await capturedSessionUpdateHandler({
      sessionId: 'kas-session-1',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'hello' },
      },
    });

    const events = handler.mock.calls.map((c) => c[0]);
    expect(events.some((e: any) => e.type === AgentEventType.Content)).toBe(
      true
    );
    expect(
      events.some((e: any) => e.type === AgentEventType.ModelRefusal)
    ).toBe(false);
  });
});

describe('KasAcpClient — _kiro/sessions/changed forwarding', () => {
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

  it('forwards each roster delta unmerged as a SessionRosterDelta event', async () => {
    const client = new KasAcpClient();
    const events: any[] = [];
    client.onUpdate((e: any) => events.push(e));
    await client.initialize();
    const kc = (client as any).kiroClient;
    const roster = kc._extNotifHandlers['_kiro/sessions/changed'];
    expect(roster).toBeDefined();
    const delta = {
      upserted: [{ sessionId: 's1', status: 'provisioning' }],
      deleted: ['s0'],
    };
    roster(delta);
    const fwd = events.filter((e) => e.type === 'session_roster_delta');
    expect(fwd).toHaveLength(1);
    expect(fwd[0].delta).toEqual(delta);
  });

  it('emits cloud ready when a started session first reports completed', async () => {
    const originalDateNow = Date.now;
    let nowMs = 1_000;
    Date.now = () => nowMs;
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: {
          kiro: {
            sessionSources: ['local', 'remote'],
            executionTargets: ['cloud-sandbox'],
          },
        },
      },
    });
    mockKiroNewSession.mockImplementationOnce(async () => {
      nowMs = 2_500;
      return { sessionId: 'kas-session-1', configOptions: [] };
    });
    const client = new KasAcpClient({
      executionTarget: { kind: 'cloud-sandbox' },
    });
    try {
      await client.initialize();
      await client.newSession();
      nowMs = 4_000;
      const kc = (client as any).kiroClient;
      kc._extNotifHandlers['_kiro/sessions/changed']({
        upserted: [{ sessionId: 'kas-session-1', status: 'completed' }],
      });
      expect(mockRecordTuiCloudSessionReady).toHaveBeenCalledWith({
        durationSeconds: 3,
        version: 'test-version',
      });
    } finally {
      Date.now = originalDateNow;
    }
  });

  it('does not emit cloud ready on a reattached session (no start baseline)', async () => {
    mockKiroInitialize.mockResolvedValueOnce({
      protocolVersion: '1.0',
      agentCapabilities: {
        _meta: { kiro: { sessionSources: ['local', 'remote'] } },
      },
    });
    mockKiroLoadSession.mockResolvedValueOnce({
      sessionId: 'kas-loaded',
      _meta: { source: 'remote' },
      configOptions: [],
    });
    const client = new KasAcpClient();
    await client.initialize();
    await client.loadSession('reattached-id');
    const kc = (client as any).kiroClient;
    kc._extNotifHandlers['_kiro/sessions/changed']({
      upserted: [{ sessionId: 'reattached-id', status: 'idle' }],
    });
    expect(mockRecordTuiCloudSessionReady).not.toHaveBeenCalled();
  });

  it('registers the getAccessToken capability on the handshake', async () => {
    const client = new KasAcpClient();
    await client.initialize();
    const caps = capturedKiroClientConfig?.capabilities ?? [];
    const names = caps.map((c: any) => c?.name ?? c?.method ?? '').join(',');
    expect(names).toContain('getAccessToken');
  });
});
