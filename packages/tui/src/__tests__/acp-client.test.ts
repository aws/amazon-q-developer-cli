import { describe, it, expect, mock, beforeEach } from 'bun:test';
import { EventEmitter } from 'events';
import { AgentEventType, ContentType } from '../types/agent-events';
import type { SessionNotification } from '@agentclientprotocol/sdk';

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
  };
  return mockProcess;
});

mock.module('child_process', () => ({
  spawn: mockSpawn,
}));
mock.module('node:child_process', () => ({
  spawn: mockSpawn,
}));

// --- Mock @agentclientprotocol/sdk ---
const mockInitialize = mock((_params: any) =>
  Promise.resolve({ protocolVersion: '1.0' })
);
const mockNewSession = mock((_params: any) =>
  Promise.resolve({
    sessionId: 'test-session-123',
    models: null,
    modes: null,
  })
);
const mockLoadSession = mock((_params: any) =>
  Promise.resolve({
    sessionId: 'loaded-session',
    models: null,
    modes: null,
  })
);
const mockPrompt = mock((_params: any) => Promise.resolve());
const mockCancel = mock((_params: any) => Promise.resolve());
const mockExtMethod = mock((_method: string, _params: any) =>
  Promise.resolve({})
);
const mockSetSessionMode = mock((_params: any) => Promise.resolve());
const mockConnectionSignal = { aborted: false };

class MockClientSideConnection {
  signal = mockConnectionSignal;
  initialize = mockInitialize;
  newSession = mockNewSession;
  loadSession = mockLoadSession;
  prompt = mockPrompt;
  cancel = mockCancel;
  extMethod = mockExtMethod;
  setSessionMode = mockSetSessionMode;
  constructor(_clientFactory: any, _stream: any) {}
}

mock.module('@agentclientprotocol/sdk', () => ({
  ndJsonStream: (_writable: any, _readable: any) => ({
    readable: new ReadableStream(),
    writable: new WritableStream(),
  }),
  ClientSideConnection: MockClientSideConnection,
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

// Dynamic import after mocks
const { AcpClient } = await import('../acp-client');

describe('AcpClient', () => {
  beforeEach(() => {
    mockSpawn.mockClear();
    mockInitialize.mockClear();
    mockNewSession.mockClear();
    mockLoadSession.mockClear();
    mockPrompt.mockClear();
    mockCancel.mockClear();
    mockExtMethod.mockClear();
    mockSetSessionMode.mockClear();
    // Reset mockSpawn to create fresh process for each test
    mockSpawn.mockImplementation(
      (_cmd: string, _args: string[], _opts: any) => {
        mockProcess = {
          stdin: createMockStdin(),
          stdout: createMockStream(),
          stderr: createMockStream(),
          kill: mock(() => {}),
          pid: 12345,
          on: mock(() => {}),
        };
        return mockProcess;
      }
    );
  });

  it('constructor creates agent process via spawn with "acp" first arg', () => {
    const _client = new AcpClient('/path/to/agent', []);
    expect(mockSpawn).toHaveBeenCalled();
    const callArgs = mockSpawn.mock.calls[0]!;
    expect(callArgs[0]).toBe('/path/to/agent');
    expect(callArgs[1][0]).toBe('acp');
  });

  it('close() calls kill("SIGTERM") on the agent process', () => {
    const client = new AcpClient('/path/to/agent', []);
    client.close();
    expect(mockProcess.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('onUpdate registers a handler and returns unsubscribe function', () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    const unsubscribe = client.onUpdate(handler);
    expect(typeof unsubscribe).toBe('function');
    unsubscribe();
  });

  it('after unsubscribe, handler is no longer called', () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    const unsubscribe = client.onUpdate(handler);
    unsubscribe();
    // After unsubscribe, handler should not be called
    expect(handler).not.toHaveBeenCalled();
  });

  it('initialize() calls connection.initialize with correct params', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.initialize();
    expect(mockInitialize).toHaveBeenCalledTimes(1);
    const params = mockInitialize.mock.calls[0]![0];
    expect(params.clientInfo.name).toBe('kiro-tui');
    expect(params.protocolVersion).toBeDefined();
  });

  it('newSession() calls connection.newSession and returns sessionId', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const result = await client.newSession();
    expect(mockNewSession).toHaveBeenCalledTimes(1);
    expect(result.sessionId).toBe('test-session-123');
  });

  it('cancel() calls connection.cancel with sessionId', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    await client.cancel();
    expect(mockCancel).toHaveBeenCalledTimes(1);
    const params = mockCancel.mock.calls[0]![0];
    expect(params.sessionId).toBe('test-session-123');
  });

  it('prompt() throws when no session is active', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await expect(
      client.prompt([{ type: 'text', text: 'hello' } as any])
    ).rejects.toThrow('cannot send prompt without an active session');
  });

  it('executeCommand() calls connection.extMethod', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    mockExtMethod.mockResolvedValue({ success: true, message: 'ok' });
    const _result = await client.executeCommand({ command: 'test' } as any);
    expect(mockExtMethod).toHaveBeenCalled();
  });

  it('sessionUpdate for agent_message_chunk text content broadcasts Content event', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    const notification: SessionNotification = {
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'Hello world' },
      },
    };
    await client.sessionUpdate(notification);

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.Content);
    expect(event.content.type).toBe(ContentType.Text);
    expect(event.content.text).toBe('Hello world');
  });

  it('sessionUpdate for tool_call broadcasts ToolCall event', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    const notification: SessionNotification = {
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-1',
        title: 'fs_write',
        kind: 'edit',
        rawInput: { path: '/test' },
        content: [],
        locations: [],
      },
    };
    await client.sessionUpdate(notification);

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.ToolCall);
    expect(event.id).toBe('tc-1');
    expect(event.name).toBe('fs_write');
  });

  it('sessionUpdate for tool_call_update completed broadcasts ToolCallFinished', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    const notification: SessionNotification = {
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-1',
        status: 'completed',
        rawOutput: { result: 'done' },
      },
    };
    await client.sessionUpdate(notification);

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.ToolCallFinished);
    expect(event.id).toBe('tc-1');
    expect(event.result.status).toBe('success');
  });

  it('sessionUpdate for unrecognized type does not broadcast any event', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    // Force an unrecognized sessionUpdate type to exercise the default branch
    const notification = {
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'some_future_update_type',
      },
    } as unknown as SessionNotification;
    await client.sessionUpdate(notification);

    expect(handler).not.toHaveBeenCalled();
  });

  it('extNotification for commands_available broadcasts CommandsUpdate', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/commands/available', {
      commands: [{ name: 'help', description: 'Show help' }],
      prompts: [],
      tools: [],
      mcpServers: [],
    });

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.CommandsUpdate);
    expect(event.commands.length).toBe(1);
    expect(event.commands[0].name).toBe('help');
  });

  it('extNotification for compaction_status broadcasts CompactionStatus', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/compaction/status', {
      status: { type: 'started' },
    });

    expect(handler).toHaveBeenCalled();
    const event = handler.mock.calls[0]![0] as any;
    expect(event.type).toBe(AgentEventType.CompactionStatus);
    expect(event.status).toBe('started');
  });
});
