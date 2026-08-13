import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';
import { EventEmitter } from 'events';
import { AgentEventType, ContentType } from '../types/agent-events';
import { getCliVersion } from '../utils/version';
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

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  'child_process',
  'node:child_process',
  '@agentclientprotocol/sdk',
  '../utils/logger',
  '../acp-client',
]);

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

afterAll(() => {
  mock.restore();
});

// Dynamic import after mocks — query-string specifier bypasses stale
// mock.module('../acp-client') that other test files may have registered.
// @ts-expect-error — bun-specific query-string import
const { AcpClient } = await import('../acp-client?real');

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

  it('close() removes every registered event handler', () => {
    const client = new AcpClient('/path/to/agent', []);
    const update = mock(() => {});
    const multiSession = mock(() => {});
    const sessionEvent = mock(() => {});
    const subagentList = mock(() => {});
    client.onUpdate(update);
    client.onMultiSessionUpdate(multiSession);
    client.onSessionEvent(sessionEvent);
    client.onSubagentListUpdate(subagentList);

    client.close();
    (client as any).broadcastStreamEvent({ type: AgentEventType.Content });
    (client as any).broadcastMultiSession('session-1', {
      type: AgentEventType.Content,
    });
    (client as any).broadcastSessionEvent({
      type: 'session_terminated',
      sessionId: 'session-1',
    });
    (client as any).broadcastSubagentList([], []);

    expect(update).not.toHaveBeenCalled();
    expect(multiSession).not.toHaveBeenCalled();
    expect(sessionEvent).not.toHaveBeenCalled();
    expect(subagentList).not.toHaveBeenCalled();
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

  it('initialize() reports the injected version in clientInfo', async () => {
    // The launcher forwards the real release version via KIRO_VERSION_OVERRIDE;
    // injecting it through the constructor lets us assert the handshake carries
    // it without re-importing the module to bust a cached constant. The
    // negative assertion guards the original fix: the `99.99.99-dev` dev
    // fallback must never leak when a real version was forwarded.
    const client = new AcpClient('/path/to/agent', [], '3.1.4-test');
    await client.initialize();
    const params = mockInitialize.mock.calls[0]![0];
    expect(params.clientInfo.version).toBe('3.1.4-test');
    expect(params.clientInfo.version).not.toBe('99.99.99-dev');
  });

  it('initialize() defaults clientInfo version to getCliVersion()', async () => {
    // No-regression guard: with no injected version, production behavior is
    // unchanged — clientInfo reports the launcher-forwarded CLI version.
    const client = new AcpClient('/path/to/agent', []);
    await client.initialize();
    const params = mockInitialize.mock.calls[0]![0];
    expect(params.clientInfo.version).toBe(getCliVersion());
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

  it('uses canonical V2 metadata for MCP identity and retains the display title', async () => {
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.sessionUpdate({
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-mcp',
        title: 'Running: @weather/get_forecast',
        kind: 'other',
        rawInput: { city: 'Seattle' },
        content: [],
        locations: [],
        _meta: { kiro: { toolName: '@weather/get_forecast' } },
      },
    } as unknown as SessionNotification);

    expect(handler.mock.calls[0]![0]).toMatchObject({
      type: AgentEventType.ToolCall,
      id: 'tc-mcp',
      name: 'get_forecast',
      origin: 'mcp',
      originalTitle: 'Running: @weather/get_forecast',
      args: { city: 'Seattle' },
      meta: { kiro: { toolName: '@weather/get_forecast' } },
    });
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

  it('Failed tool_call_update for a subagent stage stamps the synthesized ToolCall with the stage sessionId', async () => {
    // Bug 3 (kill-race / rejected-before-exec leak): when a Failed
    // tool_call_update carrying rawInput is the FIRST event the store sees for
    // a toolCallId (no prior `tool_call` was sent — parse error /
    // permission-denied / hook-rejected), the converter synthesizes a ToolCall
    // and broadcasts it inline. For a subagent stage that synthesized event
    // MUST carry the stage sessionId, or the store falls back to the main
    // agent and lite leaks the stage tool into the main scrollback.
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession(); // this.sessionId = 'test-session-123'
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    const notification: SessionNotification = {
      sessionId: 'stage-session',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-x',
        status: 'failed',
        title: 'grep',
        kind: 'search',
        rawInput: { pattern: 'x' },
      },
    } as unknown as SessionNotification;
    await client.sessionUpdate(notification);

    // The FIRST broadcast is the synthesized ToolCall, stamped with the stage
    // session so the store resolves the stage's agentName (not main).
    const synth = handler.mock.calls[0]![0] as any;
    expect(synth.type).toBe(AgentEventType.ToolCall);
    expect(synth.id).toBe('tc-x');
    expect(synth.sessionId).toBe('stage-session');
    // A ToolCallFinished for the same id is also broadcast (it's matched by id
    // in the store, so it doesn't need the stamp).
    const finished = handler.mock.calls.find(
      (c) => (c[0] as any).type === AgentEventType.ToolCallFinished
    );
    expect(finished).toBeDefined();
    expect((finished![0] as any).id).toBe('tc-x');
  });

  it('Failed tool_call_update for the MAIN agent leaves the synthesized ToolCall unstamped (regression guard)', async () => {
    // Genuine main-agent rejected-before-exec tools must still render in the
    // main view: notifSessionId === this.sessionId → no stamp.
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession(); // this.sessionId = 'test-session-123'
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    const notification: SessionNotification = {
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-main',
        status: 'failed',
        title: 'grep',
        kind: 'search',
        rawInput: { pattern: 'x' },
      },
    } as unknown as SessionNotification;
    await client.sessionUpdate(notification);

    const synth = handler.mock.calls[0]![0] as any;
    expect(synth.type).toBe(AgentEventType.ToolCall);
    expect(synth.id).toBe('tc-main');
    expect(synth.sessionId).toBeUndefined();
  });

  it('title-less Failed update on the main session does NOT synthesize a leaking ToolCall', async () => {
    // KAS's orchestrate-subagent re-reads files a finished subagent referenced,
    // emitting those reads at the PARENT (main) executionId with no title and
    // no subagent stamp. A missing file yields a title-less Failed
    // tool_call_update on the main session. Synthesizing a ToolCall for it
    // attributes it to the main agent and leaks it into the main transcript
    // (the subagent-tool "bleed"). Only failures leaked, since the success
    // branch synthesizes solely on rawInput.response — matching the observed
    // failures-only asymmetry. We now suppress synthesis for this case, so no
    // ToolCall is broadcast and the (unmatched) ToolCallFinished is a store
    // no-op → nothing renders in main.
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession(); // this.sessionId = 'test-session-123'
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    // Real wire: KAS's toolCallUpdate passthrough carries neither title nor
    // kind for an instant-fail read (only the Running-path emit adds them).
    await client.sessionUpdate({
      sessionId: 'test-session-123', // MAIN session — no subagent stamp
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-orphan-read',
        status: 'failed',
        rawInput: { path: 'README.md' },
      },
    } as unknown as SessionNotification);

    const events = handler.mock.calls.map((c) => c[0] as any);
    expect(events.some((e) => e.type === AgentEventType.ToolCall)).toBe(false);
  });

  it('title-less Failed update for a SUBAGENT session still synthesizes (routed to its surface)', async () => {
    // Suppression is scoped to the main session. A title-less failed read that
    // carries a differing (subagent) sessionId is stamped and routed to the
    // subagent surface, so it must still synthesize.
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.sessionUpdate({
      sessionId: 'stage-session',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-read',
        status: 'failed',
        rawInput: { path: 'README.md' },
      },
    } as unknown as SessionNotification);

    const synth = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.ToolCall);
    expect(synth).toBeDefined();
    expect(synth.sessionId).toBe('stage-session'); // routed to subagent surface
  });

  it('titled Failed update still synthesizes (genuine rejected-before-exec tool)', async () => {
    // Regression guard: the synthesis exists for parse-error /
    // permission-denied / hook-rejected tools, which ALWAYS carry a title.
    // Suppression must not touch them.
    const client = new AcpClient('/path/to/agent', []);
    await client.newSession();
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.sessionUpdate({
      sessionId: 'test-session-123',
      update: {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-rejected',
        status: 'failed',
        title: 'grep',
        kind: 'search',
        rawInput: { pattern: 'x' },
      },
    } as unknown as SessionNotification);

    const synth = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.ToolCall);
    expect(synth).toBeDefined();
    expect(synth.name).toBe('grep');
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

  it('extNotification for commands_available partitions wire prompts into PromptsUpdate + SkillsUpdate', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    // V2 wire shape: prompts and skills both arrive in the `prompts`
    // array. Skills are tagged via a "skill:" prefix on `serverName`.
    await client.extNotification!('kiro.dev/commands/available', {
      commands: [],
      prompts: [
        {
          name: 'research',
          description: 'Deep research',
          arguments: [{ name: 'topic', required: true }],
          serverName: 'core-mcp',
        },
        {
          name: 'project-setup',
          description: 'Set up project',
          arguments: [],
          serverName: 'local',
        },
        {
          name: 'pair-program',
          description: 'Skill prompt',
          arguments: [],
          serverName: 'skill:config',
        },
      ],
      tools: [],
      mcpServers: [],
    });

    const promptsEvent = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.PromptsUpdate);
    const skillsEvent = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.SkillsUpdate);

    expect(promptsEvent).toBeDefined();
    expect(promptsEvent.prompts).toHaveLength(2);
    expect(promptsEvent.prompts[0]).toMatchObject({
      name: 'research',
      arguments: [{ name: 'topic', required: true }],
      source: { kind: 'mcp', serverName: 'core-mcp' },
    });
    expect(promptsEvent.prompts[1]).toMatchObject({
      name: 'project-setup',
      source: { kind: 'workspace' },
    });

    expect(skillsEvent).toBeDefined();
    expect(skillsEvent.skills).toHaveLength(1);
    expect(skillsEvent.skills[0]).toMatchObject({
      name: 'pair-program',
      source: { kind: 'agent-config' },
    });
    // V2 wire does NOT carry the resolved file path; partition leaves
    // `path` undefined rather than emitting the bogus literal "config".
    expect(skillsEvent.skills[0].source.path).toBeUndefined();
  });

  it("extNotification for commands_available maps serverName 'global' / 'local' to typed PromptSource kinds", async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/commands/available', {
      commands: [],
      prompts: [
        {
          name: 'global-p',
          description: '',
          arguments: [],
          serverName: 'global',
        },
        {
          name: 'local-p',
          description: '',
          arguments: [],
          serverName: 'local',
        },
      ],
      tools: [],
      mcpServers: [],
    });

    const promptsEvent = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.PromptsUpdate);
    expect(promptsEvent.prompts[0].source).toEqual({ kind: 'global' });
    expect(promptsEvent.prompts[1].source).toEqual({ kind: 'workspace' });
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

  it('metadata notification with a refusal broadcasts ModelRefusal', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/metadata', {
      stopReason: 'REFUSAL',
      refusal: {
        category: 'CYBER',
        explanation: 'Declined by content policy.',
        recommendedModel: 'kiro-safe',
      },
    });

    const refusal = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.ModelRefusal);
    expect(refusal).toBeDefined();
    expect(refusal.explanation).toBe('Declined by content policy.');
    expect(refusal.category).toBe('CYBER');
    expect(refusal.recommendedModel).toBe('kiro-safe');
  });

  it('metadata notification with CONTENT_FILTERED stop reason broadcasts ModelRefusal', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/metadata', {
      stopReason: 'CONTENT_FILTERED',
    });

    const refusal = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.ModelRefusal);
    expect(refusal).toBeDefined();
    expect(refusal.stopReason).toBe('CONTENT_FILTERED');
  });

  it('metadata notification without effort does not broadcast EffortUpdate', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    // A refusal-only notification omits effort and must not clear the chip.
    await client.extNotification!('kiro.dev/metadata', {
      refusal: { explanation: 'nope' },
    });

    const effort = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.EffortUpdate);
    expect(effort).toBeUndefined();
  });

  it('metadata notification with effort broadcasts EffortUpdate', async () => {
    const client = new AcpClient('/path/to/agent', []);
    const handler = mock((_event: any) => {});
    client.onUpdate(handler);

    await client.extNotification!('kiro.dev/metadata', { effort: 'high' });

    const effort = handler.mock.calls
      .map((c) => c[0] as any)
      .find((e) => e.type === AgentEventType.EffortUpdate);
    expect(effort).toBeDefined();
    expect(effort.effort).toBe('high');
  });
});
