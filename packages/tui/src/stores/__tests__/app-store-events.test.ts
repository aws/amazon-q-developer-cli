/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
import { describe, it, expect, mock, afterAll } from 'bun:test';
import { KAS_DEFAULT_AGENT_ID } from '../../constants/agents.js';
import { AgentEventType, ContentType } from '../../types/agent-events';
import { SessionLifecycleOwner } from '../../types/multi-session.js';
import { CommandHistory } from '../../utils/command-history';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../../kiro']);

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

const { createAppStore, MessageRole, ToolUseStatus } =
  await import('../app-store');
const { Kiro } = await import('../../kiro');

function makeStore() {
  const store = createAppStore({ kiro: new Kiro() });
  store.setState({ isInitialized: true });
  return store;
}

describe('Stream event handler — ToolCall subagent stamping (Bug 1)', () => {
  // Regression guard for the flush/spinner-stall bug: a subagent stage's
  // tool call must be stamped with the STAGE agentName (so static-flush's
  // later isInnerSubagentTool helper can hide it), not the main agent. The
  // acp-client fix attaches the notification sessionId to the ToolCall event
  // before broadcasting it to the main store; this test pins the store side
  // of that contract — given a sessionId for a registered subagent session,
  // the message's agentName resolves to the stage name.
  it('stamps the stage name (inner) when the ToolCall carries a subagent sessionId', async () => {
    const store = makeStore();
    store.setState({ sessionId: 'main-session' });
    store.getState().addSubagentSession({
      sessionId: 'stage-session',
      agentName: 'scan',
      status: 'working',
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-sub',
      name: 'grep',
      args: { pattern: 'x' },
      sessionId: 'stage-session',
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-sub');
    expect(msg).toBeDefined();
    expect(msg!.agentName).toBe('scan');
  });

  it('stamps the main agent name (visible) when no sessionId is present', async () => {
    const store = makeStore();
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'main-agent' },
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-main',
      name: 'grep',
      args: { pattern: 'x' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-main');
    expect(msg).toBeDefined();
    expect(msg!.agentName).toBe('main-agent');
  });

  it('a lone ToolCallFinished (converter-suppressed orphan read) adds no tool row', async () => {
    // End-to-end guard for the subagent-read bleed: the converter now
    // suppresses the synthesized ToolCall for a title-less failed read on the
    // main session, so the store only ever sees the ToolCallFinished. With no
    // matching ToolUse message, that is a no-op — nothing renders in main.
    const store = makeStore();
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'main-agent' },
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'read-orphan',
      result: { status: 'error', error: 'ENOENT: README.md' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const toolRows = store
      .getState()
      .messages.filter((m: any) => m.role === MessageRole.ToolUse);
    expect(toolRows.length).toBe(0);
  });
});

describe('Stream event handler — ToolCall', () => {
  it('adds a new tool call message', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-1',
      name: 'fs_write',
      kind: 'edit',
      args: { path: '/tmp/test.ts', content: 'hello' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msgs = store.getState().messages;
    expect(
      msgs.some((m: any) => m.id === 'tc-1' && m.role === MessageRole.ToolUse)
    ).toBe(true);
  });

  // Resumed history rows carry no persisted duration, so the handler must NOT
  // stamp Date.now() timestamps (which would render a bogus ~0ms elapsed chip);
  // live rows must stamp them so the elapsed chip renders.
  it.each([
    ['fromHistory', { fromHistory: true }, 'hist-tc', undefined],
    ['live (non-history)', undefined, 'live-tc', 'positive'],
  ] as const)(
    'timestamps: %s ToolCall',
    async (_label, opts, id, expectation) => {
      const store = makeStore();
      const handler = store.getState().createStreamEventHandler(opts as never);
      handler({
        type: AgentEventType.ToolCall,
        id,
        name: 'fs_read',
        kind: 'read',
        args: { path: '/tmp/x.ts' },
      });
      handler({
        type: AgentEventType.ToolCallFinished,
        id,
        result: { status: 'success', output: { text: 'ok' } },
      } as never);
      await new Promise((r) => setTimeout(r, 50));
      const msg: any = store.getState().messages.find((m: any) => m.id === id);
      expect(msg).toBeDefined();
      if (expectation === 'positive') {
        expect(msg.startTime).toBeGreaterThan(0);
        expect(msg.finishTime).toBeGreaterThan(0);
      } else {
        expect(msg.startTime).toBeUndefined();
        expect(msg.finishTime).toBeUndefined();
      }
    }
  );

  it('switches tool timing from replay to live delivery', async () => {
    const store = makeStore();
    const handler = store
      .getState()
      .createStreamEventHandler({ fromHistory: true });

    for (const id of ['history-tool', 'live-tool']) {
      handler({
        type: AgentEventType.ToolCall,
        id,
        name: 'fs_read',
        kind: 'read',
        args: { path: '/tmp/x.ts' },
      });
      handler({
        type: AgentEventType.ToolCallFinished,
        id,
        result: { status: 'success', output: { text: 'ok' } },
      } as never);
      if (id === 'history-tool') handler.setHistoryReplay(false);
    }

    await new Promise((resolve) => setTimeout(resolve, 50));
    const history = store
      .getState()
      .messages.find((m) => m.id === 'history-tool');
    const live = store.getState().messages.find((m) => m.id === 'live-tool');
    expect(history?.role).toBe(MessageRole.ToolUse);
    expect(live?.role).toBe(MessageRole.ToolUse);
    if (
      history?.role === MessageRole.ToolUse &&
      live?.role === MessageRole.ToolUse
    ) {
      expect(history.startTime).toBeUndefined();
      expect(history.finishTime).toBeUndefined();
      expect(live.startTime).toBeGreaterThan(0);
      expect(live.finishTime).toBeGreaterThan(0);
    }
  });

  it('preserves MCP provenance and raw args on an edit-kind name collision', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-mcp-collision',
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: '@server/fs_write',
      kind: 'edit',
      args: { operation: 'custom', payload: 'keep me' },
    });
    await new Promise((r) => setTimeout(r, 50));

    const msg = store
      .getState()
      .messages.find((candidate: any) => candidate.id === 'tc-mcp-collision');
    expect(msg).toMatchObject({
      origin: 'mcp',
      originalTitle: '@server/fs_write',
    });
    expect(JSON.parse(msg!.content)).toEqual({
      operation: 'custom',
      payload: 'keep me',
    });
    expect(msg!.diff).toBeUndefined();
  });

  it('renders a tool card for a standalone-subagent ToolCall forwarded to main', async () => {
    // A hidden/standalone subagent's tool call is forwarded to the main stream
    // by KasAcpClient with sessionId stripped to undefined. Verify the main
    // store/render path actually appends a tool card for it — proving the
    // surfaced event becomes a real inline card, not merely a routed event.
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'standalone-tc',
      name: 'fs_write',
      kind: 'edit',
      sessionId: undefined,
      args: { path: '/spec/requirements.md', content: 'hi' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msgs = store.getState().messages;
    expect(
      msgs.some(
        (m: any) => m.id === 'standalone-tc' && m.role === MessageRole.ToolUse
      )
    ).toBe(true);
  });

  it('preserves sessions owned by another active orchestration parent', () => {
    const store = makeStore();
    const activeSession = {
      id: 'session-a',
      name: 'worker',
      status: 'busy',
      type: 'ephemeral',
      group: 'group-a',
      created: new Date(),
      lastActivity: new Date(),
    };
    const incomingSession = {
      ...activeSession,
      id: 'session-b',
      group: 'group-b',
    };
    const staleSession = {
      ...activeSession,
      id: 'stale-session',
      group: 'old-group',
      status: 'terminated',
    };
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'main-agent' },
      sessions: new Map([
        [activeSession.id, activeSession],
        [incomingSession.id, incomingSession],
        [staleSession.id, staleSession],
      ]),
      sessionEventBuffer: {
        [activeSession.id]: [],
        [incomingSession.id]: [],
        [staleSession.id]: [],
      },
      messages: [
        {
          id: 'parent-a',
          role: MessageRole.ToolUse,
          name: 'orchestrate_subagent',
          pipelineGroupId: 'group-a',
          content: '{}',
        },
        {
          id: 'child-a',
          role: MessageRole.ToolUse,
          name: 'read',
          sessionId: activeSession.id,
          pipelineGroupId: 'group-a',
          agentName: 'worker',
          content: '{}',
          isSubagentTool: true,
        },
        {
          id: 'stale-child',
          role: MessageRole.ToolUse,
          name: 'read',
          sessionId: staleSession.id,
          pipelineGroupId: 'old-group',
          agentName: 'worker',
          content: '{}',
          isSubagentTool: true,
        },
      ],
    });

    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'parent-b',
      name: 'orchestrate_subagent',
      args: {},
      meta: {
        kiro: {
          pipeline: {
            groupId: 'group-b',
            stages: [],
          },
        },
      },
    });

    const state = store.getState();
    expect(state.sessions.has(activeSession.id)).toBe(true);
    expect(state.sessions.has(incomingSession.id)).toBe(true);
    expect(state.sessions.has(staleSession.id)).toBe(false);
    expect(state.sessionEventBuffer[activeSession.id]).toBeDefined();
    expect(state.sessionEventBuffer[incomingSession.id]).toBeDefined();
    expect(state.sessionEventBuffer[staleSession.id]).toBeUndefined();
    expect(
      state.messages.some((message: any) => message.id === 'child-a')
    ).toBe(true);
    expect(
      state.messages.some((message: any) => message.id === 'stale-child')
    ).toBe(false);
    expect(
      state.messages.find((message: any) => message.id === 'parent-b')
        ?.pipelineGroupId
    ).toBe('group-b');
  });

  it('updates existing tool call with new content', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-2',
      name: 'fs_write',
      kind: 'edit',
      args: { path: '/tmp/a.ts', content: 'v1' },
    });
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-2',
      name: 'fs_write',
      kind: 'edit',
      args: { path: '/tmp/a.ts', oldStr: 'v1', newStr: 'v2' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-2');
    expect(msg).toBeDefined();
    if (msg!.role === MessageRole.ToolUse) {
      // After the second call, the diff is synthesized from edit args
      // (oldStr/newStr/path), not stuffed into a JSON-encoded `command` blob.
      expect(msg!.diff).toEqual({
        path: '/tmp/a.ts',
        newText: 'v2',
        oldText: 'v1',
      });
    }
  });

  it('handles toolContent diff in ToolCall', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-3',
      name: 'fs_write',
      kind: 'edit',
      args: {},
      toolContent: [
        { type: 'diff', path: '/tmp/x.ts', oldText: 'old', newText: 'new' },
      ],
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-3');
    if (msg!.role === MessageRole.ToolUse) {
      expect(msg!.diff).toEqual({
        path: '/tmp/x.ts',
        newText: 'new',
        oldText: 'old',
      });
    }
  });

  it('captures __tool_use_purpose on the typed sibling field for edit-kind tools', async () => {
    // The lite renderer reads `msg.purpose` to surface the model's "why"
    // string in scrollback. The synthesis below rebuilds `content` from a
    // fixed field list and drops `__tool_use_purpose` from the JSON, so
    // without the typed sibling lite has no way to recover the field.
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    // Stream-style first chunk: empty args, no purpose yet.
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-purpose-1',
      name: 'fs_write',
      kind: 'edit',
      args: {},
    });
    // Full tool_call event with rawInput including the model's purpose.
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-purpose-1',
      name: 'fs_write',
      kind: 'edit',
      args: {
        path: '/tmp/foo.ts',
        content: 'hi',
        __tool_use_purpose: 'hello',
      },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store
      .getState()
      .messages.find((m: any) => m.id === 'tc-purpose-1');
    expect(msg).toBeDefined();
    expect(msg!.purpose).toBe('hello');
    // Sanity: the synthesized content does NOT carry the purpose, so the
    // typed field is the only surface that has it.
    const parsed = JSON.parse(msg!.content);
    expect(parsed.__tool_use_purpose).toBeUndefined();
  });

  it('preserves typed purpose across ToolCallFinished', async () => {
    // ToolCallFinished previously rebuilt the message via an explicit
    // field list and dropped `purpose`, wiping the lite scrollback's
    // purple reasoning the moment Rust finished executing the tool.
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-finish-purpose',
      name: 'fs_write',
      kind: 'edit',
      args: {
        path: '/tmp/x.ts',
        content: 'hi',
        __tool_use_purpose: 'hello',
      },
    });
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-finish-purpose',
      result: { status: 'success', output: 'ok' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store
      .getState()
      .messages.find((m: any) => m.id === 'tc-finish-purpose');
    expect(msg).toBeDefined();
    expect(msg!.isFinished).toBe(true);
    expect(msg!.purpose).toBe('hello');
  });

  it('handles insert command detection via insertLine arg', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-4',
      name: 'fs_write',
      kind: 'edit',
      args: { path: '/tmp/b.ts', insertLine: 5, text: 'inserted' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-4');
    if (msg!.role === MessageRole.ToolUse) {
      // Edit-kind without explicit toolContent: derive a diff from args.
      // `text` becomes newText; oldText is undefined for an insert.
      expect(msg!.diff).toEqual({
        path: '/tmp/b.ts',
        newText: 'inserted',
        oldText: undefined,
      });
    }
  });
});

describe('Stream event handler — ToolCallUpdate', () => {
  it('buffers tool output and flushes to liveOutputs', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-live',
      name: 'shell',
      args: { command: 'echo hi' },
    });
    handler({
      type: AgentEventType.ToolCallUpdate,
      id: 'tc-live',
      content: { type: ContentType.Text, text: 'line1\nline2\n' },
    });
    await new Promise((r) => setTimeout(r, 100));
    const live = store.getState().liveOutputs.get('tc-live');
    expect(live).toBeDefined();
    expect(live!.length).toBeGreaterThan(0);
  });
});

describe('Stream event handler — ToolCallFinished', () => {
  it('marks tool call as finished with result', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-fin',
      name: 'shell',
      args: { command: 'ls' },
    });
    await new Promise((r) => setTimeout(r, 50));
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-fin',
      result: { status: 'success', output: 'done' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-fin');
    expect(msg!.isFinished).toBe(true);
    expect(msg!.result).toEqual({ status: 'success', output: 'done' });
  });

  it('clears liveOutputs for finished tool', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-clear',
      name: 'shell',
      args: {},
    });
    handler({
      type: AgentEventType.ToolCallUpdate,
      id: 'tc-clear',
      content: { type: ContentType.Text, text: 'output\n' },
    });
    await new Promise((r) => setTimeout(r, 100));
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-clear',
      result: { status: 'success' },
    });
    await new Promise((r) => setTimeout(r, 50));
    expect(store.getState().liveOutputs.has('tc-clear')).toBe(false);
  });
});

describe('Stream event handler — ApprovalRequest', () => {
  it('adds approval to queue and sets pendingApproval', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock(() => {});
    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'ap-1', name: 'shell' },
        permissionOptions: [{ optionId: 'allow_once', label: 'Allow' }],
        resolve,
      },
    });
    expect(store.getState().pendingApproval).not.toBeNull();
    expect(store.getState().approvalQueue).toHaveLength(1);
  });

  it('auto-approves crew tools when autoApproveCrewTools is true', () => {
    const store = makeStore();
    store.setState({ autoApproveCrewTools: true, sessionId: 'main-session' });
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock(() => {});
    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'ap-crew', name: 'shell' },
        permissionOptions: [
          { optionId: 'allow_once', kind: 'allow_once', label: 'Allow' },
        ],
        resolve,
        sessionId: 'sub-session',
      },
    });
    expect(resolve).toHaveBeenCalledWith({
      outcome: 'selected',
      optionId: 'allow_once',
    });
  });

  it('auto-approves crew tools matching by kind (KAS-shaped option)', () => {
    // KAS uses descriptive optionIds (e.g. 'accept') with kind='allow_once'.
    // The auto-approve path must match by `kind` so it works for both engines.
    const store = makeStore();
    store.setState({ autoApproveCrewTools: true, sessionId: 'main-session' });
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock(() => {});
    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'ap-crew-kas', name: 'invoke_sub_agent' },
        permissionOptions: [
          { optionId: 'accept', kind: 'allow_once', label: 'Allow' },
        ],
        resolve,
        sessionId: 'sub-session',
      },
    });
    expect(resolve).toHaveBeenCalledWith({
      outcome: 'selected',
      optionId: 'accept',
    });
  });
});

describe('Stream event handler — QuestionRequest', () => {
  const request = (
    toolCallId: string,
    resolve: ReturnType<typeof mock>,
    sessionId = 'main-session'
  ) => ({
    type: AgentEventType.QuestionRequest as const,
    value: {
      sessionId,
      toolCallId,
      question: 'Which path?',
      options: [{ title: 'Yes' }, { title: 'No' }],
      resolve,
    },
  });

  it('queues, answers, and cancels questions outside approval state', () => {
    const store = makeStore();
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'spec' },
      sessions: new Map([
        [
          'subagent-session',
          { name: 'requirements', type: 'ephemeral', status: 'running' },
        ],
      ]),
    });
    const handler = store.getState().createStreamEventHandler();
    const firstResolve = mock(() => {});
    const secondResolve = mock(() => {});

    handler({
      type: AgentEventType.ToolCall,
      id: 'replayed-question',
      name: 'Which path?',
      args: {},
    });
    handler({
      type: AgentEventType.ToolCall,
      id: 'replayed-question',
      name: 'Which path?',
      args: {},
      meta: { kiro: { toolId: 'user_input' } },
    });
    expect(
      store
        .getState()
        .messages.find((message) => message.id === 'replayed-question')
    ).toMatchObject({ isQuestion: true });

    handler(request('question-1', firstResolve, 'subagent-session'));
    handler(request('question-2', secondResolve));

    expect(store.getState().pendingApproval).toBeNull();
    expect(store.getState().pendingQuestion?.toolCallId).toBe('question-1');
    expect(store.getState().questionQueue).toHaveLength(2);

    store
      .getState()
      .respondToQuestion(
        '1 but add context',
        store.getState().pendingQuestion!,
        'Yes but add context'
      );

    expect(firstResolve).toHaveBeenCalledWith({
      action: 'answered',
      answer: 'Yes but add context',
    });
    expect(
      store
        .getState()
        .messages.filter((message) => message.role === MessageRole.User)
        .at(-1)
    ).toMatchObject({
      content: '1 but add context',
      agentName: 'requirements',
      questionToolCallId: 'question-1',
    });
    expect(store.getState().pendingQuestion?.toolCallId).toBe('question-2');

    store.getState().cancelQuestion();
    expect(secondResolve).toHaveBeenCalledWith({ action: 'dismissed' });
    expect(store.getState().pendingQuestion).toBeNull();
  });

  it('attributes main-session answers and dismisses duplicate active ids', () => {
    const store = makeStore();
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'spec' },
    });
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock(() => {});
    const duplicateResolve = mock(() => {});

    handler(request('question-1', resolve));
    handler(request('question-1', duplicateResolve));

    expect(store.getState().questionQueue).toHaveLength(1);
    expect(duplicateResolve).toHaveBeenCalledWith({ action: 'dismissed' });

    store
      .getState()
      .respondToQuestion('Yes', store.getState().pendingQuestion!);

    expect(resolve).toHaveBeenCalledWith({
      action: 'answered',
      answer: 'Yes',
    });
    expect(
      store
        .getState()
        .messages.filter((message) => message.role === MessageRole.User)
        .at(-1)
    ).toMatchObject({
      content: 'Yes',
      agentName: 'spec',
      questionToolCallId: 'question-1',
    });
  });
});

describe('Stream event handler — CompactionStatus', () => {
  it('sets isCompacting on started', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({ type: AgentEventType.CompactionStatus, status: 'started' });
    expect(store.getState().isCompacting).toBe(true);
  });

  it('clears isCompacting on completed', () => {
    const store = makeStore();
    store.setState({ isCompacting: true });
    const handler = store.getState().createStreamEventHandler();
    handler({ type: AgentEventType.CompactionStatus, status: 'completed' });
    expect(store.getState().isCompacting).toBe(false);
  });

  it('shows alert on failed', () => {
    const store = makeStore();
    store.setState({ isCompacting: true });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      error: 'out of memory',
    });
    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().transientAlert?.message).toContain(
      'Compaction failed'
    );
  });
});

// handleCompactionEvent is the handler wired to the GLOBAL onUpdate subscriber
// (index.tsx) and is the ONLY handler that fires for a user-typed /compact in
// both V2 and KAS (the createStreamEventHandler path above only runs mid-turn).
// Regression guards for the /compact UX bug: it used to push a phantom empty
// User row and set isProcessing without a label (generic "thinking" spinner).
describe('handleCompactionEvent (live /compact path)', () => {
  it('started: sets the Compacting label, no phantom empty User message', async () => {
    const store = makeStore();
    const before = store.getState().messages.length;
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
    });
    const s = store.getState();
    expect(s.isCompacting).toBe(true);
    // Compaction is its own busy state; lite must not show generic Thinking.
    expect(s.isProcessing).toBe(false);
    expect(s.loadingMessage).toBe('Compacting conversation...');
    // No empty user row appended.
    expect(s.messages.length).toBe(before);
  });

  it('completed: clears the label and appends the summary', async () => {
    const store = makeStore();
    store.setState({
      isCompacting: true,
      isProcessing: true,
      loadingMessage: 'Compacting conversation...',
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      summary: 'short recap',
    });
    const s = store.getState();
    expect(s.isCompacting).toBe(false);
    expect(s.loadingMessage).toBeNull();
    const last = s.messages[s.messages.length - 1];
    expect(last?.role).toBe(MessageRole.Model);
    expect(last?.content).toBe('short recap');
    expect((last as any)?.standalone).toBe(true);
  });

  it('failed: clears the label and surfaces a transient error', async () => {
    const store = makeStore();
    store.setState({
      isCompacting: true,
      isProcessing: true,
      loadingMessage: 'Compacting conversation...',
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      error: 'boom',
    });
    const s = store.getState();
    expect(s.isCompacting).toBe(false);
    expect(s.loadingMessage).toBeNull();
    expect(s.transientAlert?.message).toContain('boom');
  });
});

describe('Stream event handler — AuthError', () => {
  it('sets agentError and stops processing', () => {
    const store = makeStore();
    store.setState({ isProcessing: true });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AuthError,
      message: 'Token expired',
      errorType: 'expired_token',
    });
    expect(store.getState().agentError).toBe('Token expired');
    expect(store.getState().isProcessing).toBe(false);
  });
});

describe('Stream event handler — SessionError', () => {
  it('sets agentError with guidance', () => {
    const store = makeStore();
    store.setState({ isProcessing: true });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionError,
      message: 'Session crashed',
      errorType: 'process_died',
      pid: 12345,
    });
    expect(store.getState().agentError).toBe('Session crashed');
    expect(store.getState().isProcessing).toBe(false);
  });
});

describe('Stream event handler — cancel mid-reasoning', () => {
  it('finalizes thinkingMs on dispose when cancelled while still thinking', async () => {
    // Reasoning streamed, but no answer text or tool call has ended the
    // thinking phase yet — so thinkingMs is never computed on the message.
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.Thought,
      id: 't-1',
      content: { type: ContentType.Text, text: 'Reasoning about the task.' },
    });
    await new Promise((r) => setTimeout(r, 50));

    const beforeCancel = store
      .getState()
      .messages.find((m: any) => m.role === MessageRole.Model && m.thinking);
    expect(beforeCancel).toBeDefined();
    expect((beforeCancel as any).thinking).toBe('Reasoning about the task.');
    expect((beforeCancel as any).thinkingMs).toBeUndefined();

    // Cancel/error path: sendMessage disposes the handler on AbortError.
    handler.dispose();

    const afterCancel = store
      .getState()
      .messages.find((m: any) => m.role === MessageRole.Model && m.thinking);
    expect(afterCancel).toBeDefined();
    expect(typeof (afterCancel as any).thinkingMs).toBe('number');
    expect((afterCancel as any).thinkingMs).toBeGreaterThan(0);
  });

  it('does not stamp thinkingMs when there was no in-flight reasoning', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.Content,
      id: 'c-1',
      content: { type: ContentType.Text, text: 'Plain answer, no thinking.' },
    });
    await new Promise((r) => setTimeout(r, 50));

    handler.dispose();

    const modelMsg = store
      .getState()
      .messages.find((m: any) => m.role === MessageRole.Model);
    expect(modelMsg).toBeDefined();
    expect((modelMsg as any).thinking).toBeUndefined();
    expect((modelMsg as any).thinkingMs).toBeUndefined();
  });
});

describe('Stream event handler — history-only cancellation placeholders', () => {
  it.each([
    ['response', ['Response was ', 'interrupted by the user']],
    [
      'tool uses',
      ['Tool uses were interrupted, ', 'waiting for the next user prompt'],
    ],
  ])('does not render the %s sentinel', async (_name, chunks) => {
    for (const suffix of ['', '\n', '\r\n']) {
      const store = makeStore();
      const handler = store.getState().createStreamEventHandler();
      for (const [index, text] of chunks.entries()) {
        handler({
          type: AgentEventType.Content,
          id: 'sentinel',
          content: {
            type: ContentType.Text,
            text: index === chunks.length - 1 ? `${text}${suffix}` : text,
          },
        });
        await new Promise((r) => setTimeout(r, 25));
        expect(
          store
            .getState()
            .messages.some((message) => message.role === MessageRole.Model)
        ).toBe(false);
      }
      handler.flush();
      expect(
        store
          .getState()
          .messages.some((message) => message.role === MessageRole.Model)
      ).toBe(false);
    }
  });
});

describe('Stream event handler — SteeringConsumed thinking reset', () => {
  it('does not re-attach turn-1 thinking to the reply-to-steer Model row', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    // Turn 1: think, then speak — thinking attaches to the first Model row.
    handler({
      type: AgentEventType.Thought,
      id: 't-1',
      content: { type: ContentType.Text, text: 'Pondering turn one.' },
    });
    handler({
      type: AgentEventType.Content,
      id: 'c-1',
      content: { type: ContentType.Text, text: 'Answer one.' },
    });
    await new Promise((r) => setTimeout(r, 50));
    // Mid-turn steer, then the reply to it.
    handler({ type: AgentEventType.SteeringConsumed, content: 'do X instead' });
    handler({
      type: AgentEventType.Content,
      id: 'c-2',
      content: { type: ContentType.Text, text: 'Reply to steer.' },
    });
    await new Promise((r) => setTimeout(r, 50));
    handler.dispose();

    const thinkingRows = store
      .getState()
      .messages.filter((m: any) => m.role === MessageRole.Model && m.thinking);
    // Exactly ONE Model row carries thinking (the pre-steer one), not two.
    expect(thinkingRows.length).toBe(1);
    expect((thinkingRows[0] as any).thinking).toBe('Pondering turn one.');
  });
});

describe('Stream event handler — McpServerInitFailure', () => {
  it('adds to initErrors and shows alert', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'my-mcp',
      error: 'connection refused',
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].serverName).toBe('my-mcp');
    expect(store.getState().transientAlert).not.toBeNull();
  });

  it('deduplicates by server name', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'srv',
      error: 'err1',
    });
    handler({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'srv',
      error: 'err2',
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].error).toBe('err2');
  });
});

describe('Stream event handler — McpOauthRequest', () => {
  it('adds to pendingOAuthServers', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpOauthRequest,
      serverName: 'oauth-srv',
      oauthUrl: 'https://auth.example.com',
    });
    expect(store.getState().pendingOAuthServers.get('oauth-srv')).toBe(
      'https://auth.example.com'
    );
  });
});

describe('Stream event handler — McpServerInitialized', () => {
  it('removes from pendingOAuthServers', () => {
    const store = makeStore();
    store.setState({
      pendingOAuthServers: new Map([['oauth-srv', 'https://auth.example.com']]),
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpServerInitialized,
      serverName: 'oauth-srv',
    });
    expect(store.getState().pendingOAuthServers.has('oauth-srv')).toBe(false);
  });

  it('clears the authenticating flag on the server (forced re-auth resolved)', () => {
    const store = makeStore();
    store.setState({
      mcpServers: [
        { name: 'srv', status: 'running', toolCount: 2, authenticating: true },
        {
          name: 'other',
          status: 'running',
          toolCount: 1,
          authenticating: true,
        },
      ],
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpServerInitialized,
      serverName: 'srv',
    });
    const servers = store.getState().mcpServers;
    expect(servers.find((s) => s.name === 'srv')?.authenticating).toBe(false);
    // Unrelated servers are untouched.
    expect(servers.find((s) => s.name === 'other')?.authenticating).toBe(true);
  });
});

describe('Stream event handler — McpServerInitFailure', () => {
  it('clears the authenticating flag on the server when forced auth fails', () => {
    const store = makeStore();
    store.setState({
      mcpServers: [
        { name: 'srv', status: 'running', toolCount: 0, authenticating: true },
      ],
    });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpServerInitFailure,
      serverName: 'srv',
      error: 'auth failed',
    });
    expect(
      store.getState().mcpServers.find((s) => s.name === 'srv')?.authenticating
    ).toBe(false);
  });
});

describe('Stream event handler — RateLimitError', () => {
  it('shows a transient alert without adding a scrollback row', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.RateLimitError,
      message: 'Rate limited, try again in 30s',
    });
    expect(store.getState().transientAlert?.message).toBe(
      'Rate limited, try again in 30s'
    );
    expect(store.getState().transientAlert?.status).toBe('error');
    expect(store.getState().messages).toHaveLength(0);
  });
});

describe('sendMessage — non-blocking error persistence', () => {
  it('leaves a scrollback row when the prompt fails with a throttle error', async () => {
    const store = makeStore();
    store.getState().kiro.streamMessage = mock(() =>
      Promise.reject(new Error('The request was throttled by the service'))
    );
    await store.getState().sendMessage('hello');

    const rows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.System);
    expect(rows).toHaveLength(1);
    expect(rows[0]!.content).toBe('The request was throttled by the service');
    expect(rows[0]!.success).toBe(false);
    expect(rows[0]!.turnOwned).toBe(true);
    expect(store.getState().transientAlert?.message).toBe(
      'The request was throttled by the service'
    );
    expect(store.getState().isProcessing).toBe(false);
  });

  it('keeps blocking auth errors out of scrollback', async () => {
    const store = makeStore();
    store.getState().kiro.streamMessage = mock(() =>
      Promise.reject(new Error('token expired'))
    );
    await store.getState().sendMessage('hello');

    expect(
      store.getState().messages.filter((m) => m.role === MessageRole.System)
    ).toHaveLength(0);
    expect(store.getState().agentError).toBe('Session expired');
  });
});

describe('Stream event handler — ModelRefusal', () => {
  it('leaves a scrollback copy without pinning a toast above the prompt bar', () => {
    const store = makeStore();
    store.setState({ isProcessing: true });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ModelRefusal,
      stopReason: 'REFUSAL',
      category: 'CYBER',
      explanation: 'This request was declined by content policy.',
      recommendedModel: 'kiro-safe',
    });
    expect(store.getState().transientAlert).toBeNull();

    const scrollback = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.System);
    expect(scrollback).toHaveLength(1);
    expect(scrollback[0]!.content).toBe(
      'This request was declined by content policy.'
    );
    expect(scrollback[0]!.success).toBe(false);
    expect(scrollback[0]!.turnOwned).toBe(true);
  });

  it('falls back to the default guidance when no explanation is given', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ModelRefusal,
      stopReason: 'CONTENT_FILTERED',
    });
    const scrollback = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.System);
    expect(scrollback).toHaveLength(1);
    expect(scrollback[0]!.content).toBe(
      "The selected model couldn't process this request. Try a different model with /model, rewind with /rewind, or start a new session with /chat new."
    );
  });

  it('surfaces only the first refusal per turn', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ModelRefusal,
      stopReason: 'CONTENT_FILTERED',
      explanation: 'First refusal.',
    });
    handler({
      type: AgentEventType.ModelRefusal,
      stopReason: 'CONTENT_FILTERED',
      explanation: 'Second refusal.',
    });
    const scrollback = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.System);
    expect(scrollback).toHaveLength(1);
    expect(scrollback[0]!.content).toBe('First refusal.');
  });
});

describe('Stream event handler — ContextUsage', () => {
  it('sets contextUsagePercent', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({ type: AgentEventType.ContextUsage, percent: 85 });
    expect(store.getState().contextUsagePercent).toBe(85);
  });

  it('stores context breakdown snapshots independently of panel state', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const breakdown = {
      contextFiles: { tokens: 100, percent: 5 },
      tools: { tokens: 20, percent: 1 },
      kiroResponses: { tokens: 30, percent: 2 },
      yourPrompts: { tokens: 40, percent: 2 },
    };

    handler({
      type: AgentEventType.ContextBreakdownUpdate,
      breakdown,
    });

    expect(store.getState().contextBreakdownCache).toEqual(breakdown);
    expect(store.getState().contextBreakdown).toBeNull();
  });
});

describe('Stream event handler — MCP registry snapshot', () => {
  it('stores the latest registry independently of the open panel', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const registryServers = [
      {
        name: 'memory',
        status: 'disabled' as const,
        toolCount: 0,
        enabled: false,
      },
    ];

    handler({
      type: AgentEventType.McpRegistrySnapshot,
      registryServers,
    });

    expect(store.getState().mcpRegistryCache).toEqual(registryServers);
    expect(store.getState().mcpRegistryServers).toEqual([]);
  });
});

describe('Stream event handler — MCP server snapshot', () => {
  it('stores the latest snapshot without live-updating an open panel', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const panelServers = [
      { name: 'old', status: 'running' as const, toolCount: 1 },
    ];
    const latestServers = [
      { name: 'new', status: 'loading' as const, toolCount: 0 },
    ];
    store.getState().setShowMcpPanel(true, panelServers, 'list');

    handler({
      type: AgentEventType.McpServerSnapshot,
      servers: latestServers,
    });

    expect(store.getState().mcpServerCache).toEqual(latestServers);
    expect(store.getState().mcpServers).toEqual(panelServers);
  });
});

describe('Stream event handler — EffortUpdate', () => {
  it('sets currentEffort', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({ type: AgentEventType.EffortUpdate, effort: 'high' });
    expect(store.getState().currentEffort).toBe('high');
  });
});

describe('Stream event handler — Metadata', () => {
  it('sets lastTurnTokens', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.Metadata,
      inputTokens: 1000,
      outputTokens: 500,
      cachedTokens: 200,
    });
    expect(store.getState().lastTurnTokens).toEqual({
      input: 1000,
      output: 500,
      cached: 200,
    });
  });
});

describe('Stream event handler — AgentSwitched', () => {
  it('sets currentAgent and previousAgentName', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'kiro_planner',
      previousAgentName: 'kiro_coder',
      welcomeMessage: 'Planning mode',
    });
    expect(store.getState().currentAgent?.name).toBe('kiro_planner');
    expect(store.getState().previousAgentName).toBe('kiro_coder');
  });

  it('sets model when provided', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'agent',
      model: 'claude-sonnet',
    });
    expect(store.getState().currentModel).toEqual({
      id: 'claude-sonnet',
      name: 'claude-sonnet',
    });
  });

  it('tracks a server-pushed switch into and out of autonomous mode', () => {
    // A `current_mode_update` / `config_option_update` push arrives here as
    // AgentSwitched; the autonomous chip derives from currentAgent, so this
    // pins both directions of the sync.
    const store = makeStore();
    store.setState({ currentAgent: { name: KAS_DEFAULT_AGENT_ID } });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'autonomous',
    });
    expect(store.getState().currentAgent?.name).toBe('autonomous');
    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'spec',
    });
    expect(store.getState().currentAgent?.name).toBe('spec');
  });
});

describe('Stream event handler — SystemNotice', () => {
  it('shows a transient error banner and does NOT persist a chat message', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SystemNotice,
      message:
        "Autonomous mode was turned off — this cloud session doesn't support changing modes yet.",
      success: false,
    });
    // Transient banner (like the cloud-only command refusals), not chat history.
    expect(store.getState().transientAlert).toEqual({
      message:
        "Autonomous mode was turned off — this cloud session doesn't support changing modes yet.",
      status: 'error',
      autoHideMs: 5000,
    });
    expect(
      store
        .getState()
        .messages.filter((m: any) => m.role === MessageRole.System)
    ).toHaveLength(0);
  });

  it('maps a success notice to a success-status banner', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SystemNotice,
      message: 'x',
      success: true,
    });
    expect(store.getState().transientAlert?.status).toBe('success');
  });

  it('adds a persistent notice as one selectable system row', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const message =
      'Clipboard copy failed. Open this session-specific OAuth URL manually; do not share it:\nhttps://example.com/oauth?state=sensitive';

    handler({
      type: AgentEventType.SystemNotice,
      message,
      success: false,
      persistent: true,
    });

    expect(store.getState().transientAlert).toBeNull();
    expect(
      store
        .getState()
        .messages.filter((item: any) => item.role === MessageRole.System)
    ).toEqual([expect.objectContaining({ content: message, success: false })]);
  });
});

describe('Stream event handler — AgentNotFound', () => {
  it('adds to initErrors', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentNotFound,
      requestedAgent: 'missing-agent',
      fallbackAgent: KAS_DEFAULT_AGENT_ID,
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].type).toBe('agent_not_found');
  });

  it('carries the rejected file through to the alert the user reads', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentNotFound,
      requestedAgent: 'thunder-agent',
      fallbackAgent: KAS_DEFAULT_AGENT_ID,
      skipped: {
        path: '/home/u/.kiro/agents/thunder-agent.json',
        reasonCode: 'cli_only_agent',
        error: 'uses fields this agent engine does not support: allowedTools',
      },
    });
    expect(store.getState().initErrors[0].skipped?.reasonCode).toBe(
      'cli_only_agent'
    );
    // The whole point of the detail is that it reaches the user, not just the store.
    expect(store.getState().transientAlert?.message).toBe(
      `agent "thunder-agent" needs upgrading for this agent engine, using "${KAS_DEFAULT_AGENT_ID}" — run /upgrade-agent to convert thunder-agent.json`
    );
  });

  it('leaves the message as plain not-found when no file claimed the id', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentNotFound,
      requestedAgent: 'really-not-here',
      fallbackAgent: KAS_DEFAULT_AGENT_ID,
    });
    expect(store.getState().initErrors[0].skipped).toBeUndefined();
    expect(store.getState().transientAlert?.message).toBe(
      `agent "really-not-here" not found, using "${KAS_DEFAULT_AGENT_ID}"`
    );
  });
});

describe('Stream event handler — AgentConfigError', () => {
  it('adds to initErrors', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentConfigError,
      path: '/agents/broken.yml',
      error: 'invalid yaml',
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].type).toBe('agent_config_error');
  });
});

describe('Stream event handler — UserMessage', () => {
  it('adds historical user message', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.UserMessage,
      id: 'um-1',
      content: { type: 'text', text: 'hello from history' },
    });
    await new Promise((r) => setTimeout(r, 50));
    const msgs = store.getState().messages;
    expect(
      msgs.some(
        (m: any) => m.id === 'um-1' && m.content === 'hello from history'
      )
    ).toBe(true);
  });
});

describe('Stream event handler — McpGovernanceDisabled', () => {
  it('adds to initErrors', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.McpGovernanceDisabled,
      apiFailure: true,
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].type).toBe('mcp_governance_disabled');
  });
});

describe('Stream event handler — WebToolsGovernanceDisabled', () => {
  it('adds to initErrors', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.WebToolsGovernanceDisabled,
      apiFailure: false,
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].type).toBe(
      'web_tools_governance_disabled'
    );
  });

  it('deduplicates repeated notifications', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.WebToolsGovernanceDisabled,
      apiFailure: false,
    });
    handler({
      type: AgentEventType.WebToolsGovernanceDisabled,
      apiFailure: false,
    });
    expect(
      store
        .getState()
        .initErrors.filter((e) => e.type === 'web_tools_governance_disabled')
    ).toHaveLength(1);
  });
});

describe('respondToApproval', () => {
  it('resolves approval and updates tool status to approved', () => {
    const store = makeStore();
    const resolve = mock(() => {});
    const approval = {
      toolCall: { toolCallId: 'tc-ap', name: 'shell' },
      permissionOptions: [{ optionId: 'allow_once', label: 'Allow' }],
      resolve,
    };
    store.setState({
      pendingApproval: approval,
      approvalQueue: [approval],
      messages: [
        {
          id: 'tc-ap',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{}',
          status: ToolUseStatus.Pending,
        },
      ],
    });
    store.getState().respondToApproval('allow_once');
    expect(resolve).toHaveBeenCalledWith({
      outcome: 'selected',
      optionId: 'allow_once',
      _meta: undefined,
    });
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-ap');
    expect(msg!.status).toBe(ToolUseStatus.Approved);
  });

  it('marks tool as rejected and finished', () => {
    const store = makeStore();
    const resolve = mock(() => {});
    const approval = {
      toolCall: { toolCallId: 'tc-rej', name: 'shell' },
      permissionOptions: [],
      resolve,
    };
    store.setState({
      pendingApproval: approval,
      approvalQueue: [approval],
      messages: [
        {
          id: 'tc-rej',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{}',
          status: ToolUseStatus.Pending,
        },
      ],
    });
    store.getState().respondToApproval('reject_once');
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-rej');
    expect(msg!.status).toBe(ToolUseStatus.Rejected);
    expect(msg!.isFinished).toBe(true);
  });
});

describe('cancelApproval', () => {
  it('cancels all queued approvals', () => {
    const store = makeStore();
    const resolve1 = mock(() => {});
    const resolve2 = mock(() => {});
    const ap1 = {
      toolCall: { toolCallId: 'a1' },
      permissionOptions: [],
      resolve: resolve1,
    };
    const ap2 = {
      toolCall: { toolCallId: 'a2' },
      permissionOptions: [],
      resolve: resolve2,
    };
    store.setState({
      pendingApproval: ap1,
      approvalQueue: [ap1, ap2],
      messages: [
        { id: 'a1', role: MessageRole.ToolUse, name: 'x', content: '{}' },
        { id: 'a2', role: MessageRole.ToolUse, name: 'y', content: '{}' },
      ],
    });
    store.getState().cancelApproval();
    expect(resolve1).toHaveBeenCalledWith({ outcome: 'cancelled' });
    expect(resolve2).toHaveBeenCalledWith({ outcome: 'cancelled' });
    expect(store.getState().pendingApproval).toBeNull();
    expect(store.getState().approvalQueue).toHaveLength(0);
  });
});

describe('handleCompactionEvent', () => {
  it('handles started status', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
    });
    expect(store.getState().isCompacting).toBe(true);
    expect(store.getState().isProcessing).toBe(false);
  });

  it('handles completed with summary', async () => {
    const store = makeStore();
    store.setState({ isCompacting: true, isProcessing: false });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      summary: 'Context compacted successfully',
    });
    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().isProcessing).toBe(false);
    const msgs = store.getState().messages;
    expect(
      msgs.some((m: any) => m.content === 'Context compacted successfully')
    ).toBe(true);
    expect(
      msgs.some(
        (m: any) =>
          m.content === 'Context compacted successfully' &&
          m.standalone === true
      )
    ).toBe(true);
  });

  it('treats duplicate started events as idempotent for one compact', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
      attemptId: 1,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
      summary: 'Compaction report',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().loadingMessage).toBeNull();
    expect(store.getState().activeCompactionAttemptKey).toBeNull();
    expect(
      store
        .getState()
        .messages.some((m: any) => m.content === 'Compaction report')
    ).toBe(true);
  });

  it('ignores stale terminal events from an older compaction attempt', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
      attemptId: 1,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
      attemptId: 2,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
      summary: 'stale report',
    });

    expect(store.getState().isCompacting).toBe(true);
    expect(store.getState().loadingMessage).toBe('Compacting conversation...');
    expect(store.getState().activeCompactionAttemptKey).toBe(2);
    expect(
      store.getState().messages.some((m: any) => m.content === 'stale report')
    ).toBe(false);

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 2,
      summary: 'current report',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(
      store.getState().messages.some((m: any) => m.content === 'current report')
    ).toBe(true);
  });

  it('ignores orphan terminal events when no compaction is active', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
      summary: 'orphan report',
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      attemptId: 1,
      error: 'orphan failure',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().transientAlert).toBeNull();
    expect(
      store.getState().messages.some((m: any) => m.content === 'orphan report')
    ).toBe(false);
  });

  it('handles completed summaries without clearing an active KAS turn', async () => {
    const store = makeStore();
    store.setState({
      isCompacting: true,
      isProcessing: true,
      loadingMessage: 'Compacting conversation...',
      messages: [
        { id: 'active-user', role: MessageRole.User, content: 'next prompt' },
      ],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      summary: 'Compaction report',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().isProcessing).toBe(true);
    const report = store
      .getState()
      .messages.find((m: any) => m.content === 'Compaction report');
    expect(report).toBeDefined();
    expect((report as any).standalone).toBe(true);
  });

  it('appends the report before draining queued input', async () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['next prompt'],
      messages: [
        { id: 'before', role: MessageRole.Model, content: 'Before compact' },
      ],
    });
    const sendMessage = mock(async (content: string) => {
      store.setState((state) => ({
        isProcessing: true,
        messages: [
          ...state.messages,
          { id: 'queued-user', role: MessageRole.User, content },
          {
            id: 'streaming-model',
            role: MessageRole.Model,
            content: 'partial answer',
          },
        ],
      }));
    });
    store.setState({ sendMessage: sendMessage as any });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
      attemptId: 1,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
      summary: 'Compact report',
    });

    expect(store.getState().messages.map((m: any) => m.content)).toEqual([
      'Before compact',
      'Compact report',
      'next prompt',
      'partial answer',
    ]);
    expect(sendMessage).toHaveBeenCalledWith(
      'next prompt',
      undefined,
      'next prompt'
    );
  });

  it('inserts a late report at the compact boundary after fallback drains queued input', async () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['next prompt'],
      messages: [
        { id: 'before', role: MessageRole.Model, content: 'Before compact' },
      ],
    });
    const sendMessage = mock(async (content: string) => {
      store.setState((state) => ({
        isProcessing: true,
        messages: [
          ...state.messages,
          { id: 'queued-user', role: MessageRole.User, content },
          {
            id: 'streaming-model',
            role: MessageRole.Model,
            content: 'partial answer',
          },
        ],
      }));
    });
    store.setState({ sendMessage: sendMessage as any });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
      attemptId: 1,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
    });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
      attemptId: 1,
      summary: 'Compact report',
    });

    expect(store.getState().messages.map((m: any) => m.content)).toEqual([
      'Before compact',
      'Compact report',
      'next prompt',
      'partial answer',
    ]);
    expect(sendMessage).toHaveBeenCalledWith(
      'next prompt',
      undefined,
      'next prompt'
    );
  });

  it('handles failed status', async () => {
    const store = makeStore();
    store.setState({ isCompacting: true, isProcessing: true });
    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      error: 'timeout',
    });
    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().transientAlert?.message).toContain(
      'Compaction failed: timeout'
    );
  });

  it('forwards ContextUsage events', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.ContextUsage,
      percent: 42,
    });
    expect(store.getState().contextUsagePercent).toBe(42);
  });

  it('forwards ContextBreakdownUpdate events', async () => {
    const store = makeStore();
    const breakdown = {
      contextFiles: { tokens: 100, percent: 5 },
      tools: { tokens: 20, percent: 1 },
      kiroResponses: { tokens: 30, percent: 2 },
      yourPrompts: { tokens: 40, percent: 2 },
    };

    await store.getState().handleCompactionEvent({
      type: AgentEventType.ContextBreakdownUpdate,
      breakdown,
    });

    expect(store.getState().contextBreakdownCache).toEqual(breakdown);
  });

  it('forwards EffortUpdate events', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.EffortUpdate,
      effort: 'low',
    });
    expect(store.getState().currentEffort).toBe('low');
  });
});

describe('TurnSummary events', () => {
  it('retains each goal iteration summary when later metadata arrives', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: '/goal test' }],
    });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Content,
      id: 'response-1',
      content: { type: ContentType.Text, text: 'First response' },
    });
    handler({
      type: AgentEventType.TurnSummary,
      meteringUsage: [
        { unitPlural: 'credits', value: 1.5 },
        { unitPlural: 'credits', value: 0.5 },
      ],
      turnDurationMs: 5000,
    });
    handler({
      type: AgentEventType.ToolCall,
      id: 'goal-iter-1',
      name: 'Goal iteration 2/2',
      kind: 'other',
      args: {},
    });
    handler({
      type: AgentEventType.Content,
      id: 'response-2',
      content: { type: ContentType.Text, text: 'Second response' },
    });
    handler({
      type: AgentEventType.TurnSummary,
      meteringUsage: [{ unitPlural: 'credits', value: 0.25 }],
      turnDurationMs: 9000,
    });

    expect(
      store.getState().messages.map((message) => ({
        role: message.role,
        content: message.content,
        kind: 'kind' in message ? message.kind : undefined,
      }))
    ).toEqual([
      { role: MessageRole.User, content: '/goal test', kind: undefined },
      { role: MessageRole.Model, content: 'First response', kind: undefined },
      {
        role: MessageRole.System,
        content: 'Credits: 2.00 • Time: 5s',
        kind: 'turn-usage',
      },
      { role: MessageRole.ToolUse, content: '{}', kind: 'other' },
      { role: MessageRole.Model, content: 'Second response', kind: undefined },
      {
        role: MessageRole.System,
        content: 'Credits: 0.25 • Time: 9s',
        kind: 'turn-usage',
      },
    ]);
  });

  it('formats time as minutes when >= 60s', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'u2', role: MessageRole.User, content: 'hi' }],
    });
    store.getState().createStreamEventHandler()({
      type: AgentEventType.TurnSummary,
      meteringUsage: [{ unitPlural: 'credits', value: 3.0 }],
      turnDurationMs: 125000,
    });
    expect(store.getState().messages.at(-1)?.content).toContain('2m 5s');
  });

  it('does nothing when no user message exists', () => {
    const store = makeStore();
    store.setState({ messages: [] });
    store.getState().createStreamEventHandler()({
      type: AgentEventType.TurnSummary,
      meteringUsage: [{ unitPlural: 'credits', value: 1 }],
    });
    expect(store.getState().messages).toEqual([]);
  });
});

describe('queueMessage and processQueue', () => {
  it('queues a message', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      sessionId: 'test-session',
    });
    store.getState().queueMessage('hello');
    expect(store.getState().queuedMessages).toEqual(['hello']);
  });

  it('ignores empty messages', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      sessionId: 'test-session',
    });
    store.getState().queueMessage('   ');
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('clearQueue empties the queue', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      sessionId: 'test-session',
    });
    store.getState().queueMessage('a');
    store.getState().queueMessage('b');
    store.getState().clearQueue();
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('clearQueue resets editingQueueIndex', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
      editingQueueIndex: 1,
    });
    store.getState().clearQueue();
    expect(store.getState().queuedMessages).toEqual([]);
    expect(store.getState().editingQueueIndex).toBeNull();
  });
});

describe('removeQueuedMessage', () => {
  it('removes the item at the given index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
    });
    store.getState().removeQueuedMessage(1);
    expect(store.getState().queuedMessages).toEqual(['a', 'c']);
  });

  it('does nothing for negative index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
    });
    store.getState().removeQueuedMessage(-1);
    expect(store.getState().queuedMessages).toEqual(['a', 'b']);
  });

  it('does nothing for index >= length', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
    });
    store.getState().removeQueuedMessage(2);
    expect(store.getState().queuedMessages).toEqual(['a', 'b']);
  });

  it('sets editingQueueIndex to null when removing the edited index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
      editingQueueIndex: 1,
    });
    store.getState().removeQueuedMessage(1);
    expect(store.getState().editingQueueIndex).toBeNull();
  });

  it('decrements editingQueueIndex when removing before edited index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
      editingQueueIndex: 2,
    });
    store.getState().removeQueuedMessage(0);
    expect(store.getState().editingQueueIndex).toBe(1);
  });

  it('leaves editingQueueIndex unchanged when removing after edited index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
      editingQueueIndex: 0,
    });
    store.getState().removeQueuedMessage(2);
    expect(store.getState().editingQueueIndex).toBe(0);
  });
});

describe('replaceQueuedMessage', () => {
  it('replaces message at valid index with trimmed content', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
    });
    store.getState().replaceQueuedMessage(1, '  updated  ');
    expect(store.getState().queuedMessages).toEqual(['a', 'updated', 'c']);
  });

  it('does nothing for out-of-bounds index (negative)', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
    });
    store.getState().replaceQueuedMessage(-1, 'x');
    expect(store.getState().queuedMessages).toEqual(['a', 'b']);
  });

  it('does nothing for out-of-bounds index (>= length)', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
    });
    store.getState().replaceQueuedMessage(5, 'x');
    expect(store.getState().queuedMessages).toEqual(['a', 'b']);
  });

  it('does nothing when content is empty after trimming', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
    });
    store.getState().replaceQueuedMessage(0, '   ');
    expect(store.getState().queuedMessages).toEqual(['a', 'b']);
  });
});

describe('startEditingQueue', () => {
  it('sets editingQueueIndex to valid index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b', 'c'],
    });
    store.getState().startEditingQueue(1);
    expect(store.getState().editingQueueIndex).toBe(1);
  });

  it('does nothing for negative index', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
      editingQueueIndex: null,
    });
    store.getState().startEditingQueue(-1);
    expect(store.getState().editingQueueIndex).toBeNull();
  });

  it('does nothing for index >= length', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
      editingQueueIndex: null,
    });
    store.getState().startEditingQueue(2);
    expect(store.getState().editingQueueIndex).toBeNull();
  });
});

describe('cancelEditingQueue', () => {
  it('resets editingQueueIndex to null', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['a', 'b'],
      editingQueueIndex: 1,
    });
    store.getState().cancelEditingQueue();
    expect(store.getState().editingQueueIndex).toBeNull();
  });
});

describe('setSlashCommands', () => {
  it('preserves local commands and adds new ones', () => {
    const store = makeStore();
    const _initial = store.getState().slashCommands.length;
    store
      .getState()
      .setSlashCommands([
        { name: '/test', description: 'Test cmd', source: 'backend' as any },
      ]);
    const cmds = store.getState().slashCommands;
    expect(cmds.some((c: any) => c.name === '/test')).toBe(true);
    // Local commands preserved
    expect(cmds.some((c: any) => c.name === '/editor')).toBe(true);
  });

  it('gates backend voice commands on local or remote availability', () => {
    const original = process.env.KIRO_VOICE_SUPPORTED;
    const originalServerUrl = process.env.KIRO_VOICE_SERVER_URL;
    process.env.KIRO_VOICE_SUPPORTED = '0';
    delete process.env.KIRO_VOICE_SERVER_URL;

    try {
      const store = makeStore();
      const voiceCommand = {
        name: '/voice',
        description: 'Voice',
        source: 'backend' as any,
      };
      store
        .getState()
        .setSlashCommands([
          voiceCommand,
          { name: '/test', description: 'Test', source: 'backend' as any },
        ]);

      expect(
        store.getState().slashCommands.some((c: any) => c.name === '/voice')
      ).toBe(false);
      expect(
        store.getState().slashCommands.some((c: any) => c.name === '/test')
      ).toBe(true);

      process.env.KIRO_VOICE_SERVER_URL = 'http://127.0.0.1:19876';
      store.getState().setSlashCommands([voiceCommand]);
      expect(
        store.getState().slashCommands.some((c: any) => c.name === '/voice')
      ).toBe(true);
    } finally {
      if (original === undefined) delete process.env.KIRO_VOICE_SUPPORTED;
      else process.env.KIRO_VOICE_SUPPORTED = original;
      if (originalServerUrl === undefined)
        delete process.env.KIRO_VOICE_SERVER_URL;
      else process.env.KIRO_VOICE_SERVER_URL = originalServerUrl;
    }
  });
});

describe('resetMessages', () => {
  it('clears all messages', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'x', role: MessageRole.User, content: 'hi' }],
    });
    store.getState().resetMessages();
    expect(store.getState().messages).toEqual([]);
  });

  it('clears tasks and collapses the activity tray on /chat new', () => {
    // Bug: starting a new chat from within a session left the prior turn's
    // todo list in `tasks`, so activity surfaces kept rendering stale state.
    const store = makeStore();
    store.setState({
      messages: [{ id: 'x', role: MessageRole.User, content: 'hi' }],
      tasks: [
        { id: '1', subject: 'Old task', status: 'pending' as const },
        { id: '2', subject: 'Done', status: 'completed' as const },
      ],
      activityTrayExpanded: true,
    });
    store.getState().resetMessages();
    expect(store.getState().tasks).toEqual([]);
    expect(store.getState().activityTrayExpanded).toBe(false);
  });

  it('bumps lite.scrollbackClearToken and resets the lite skip bookmark', () => {
    // Locks in the rest of the atomic-reset contract documented above the
    // resetMessages set() call so a future split (e.g. only clearing one
    // half) trips this test. The bump is gated behind uiMode==='lite'
    // (only lite mode tracks scrollback clear tokens), so set lite first.
    const store = makeStore();
    store.setState({ uiMode: 'lite' });
    const startToken = store.getState().lite.scrollbackClearToken;
    store.setState((state) => ({
      lite: {
        ...state.lite,
        staticSkipBefore: 42,
        welcomeEmitted: true,
      },
    }));
    store.getState().resetMessages();
    expect(store.getState().lite.scrollbackClearToken).toBe(startToken + 1);
    expect(store.getState().lite.staticSkipBefore).toBe(0);
    expect(store.getState().lite.welcomeEmitted).toBe(false);
  });
});

describe('setCurrentAgent', () => {
  it('adds welcome message when provided', () => {
    const store = makeStore();
    store
      .getState()
      .setCurrentAgent({ name: 'planner', welcomeMessage: 'Hello!' });
    expect(store.getState().currentAgent?.name).toBe('planner');
    const msgs = store.getState().messages;
    expect(msgs.some((m: any) => m.content === 'Hello!' && m.standalone)).toBe(
      true
    );
  });

  it('suppresses welcome message when option set', () => {
    const store = makeStore();
    store
      .getState()
      .setCurrentAgent(
        { name: 'planner', welcomeMessage: 'Hello!' },
        { suppressWelcome: true }
      );
    expect(store.getState().messages).toHaveLength(0);
  });

  it('does not re-add the welcome when the same agent is re-asserted', () => {
    const store = makeStore();
    store
      .getState()
      .setCurrentAgent({ name: 'planner', welcomeMessage: 'Hello!' });
    store
      .getState()
      .setCurrentAgent({ name: 'planner', welcomeMessage: 'Hello!' });
    const banners = store
      .getState()
      .messages.filter((m: any) => m.content === 'Hello!' && m.standalone);
    expect(banners).toHaveLength(1);
  });

  it('adds the welcome again when switching to a different agent', () => {
    const store = makeStore();
    store.getState().setCurrentAgent({ name: 'planner', welcomeMessage: 'P!' });
    store.getState().setCurrentAgent({ name: 'coder', welcomeMessage: 'C!' });
    const banners = store
      .getState()
      .messages.filter((m: any) => m.standalone)
      .map((m: any) => m.content);
    expect(banners).toEqual(['P!', 'C!']);
  });

  it('sets null agent', () => {
    const store = makeStore();
    store.getState().setCurrentAgent(null);
    expect(store.getState().currentAgent).toBeNull();
  });
});

describe('Input buffer — backspace at line start merges lines', () => {
  it('merges current line with previous', () => {
    const store = makeStore();
    store.getState().insert('a');
    store.getState().newline();
    store.getState().insert('b');
    // Cursor is at row 1, col 1. Move to col 0
    store.setState({
      input: { ...store.getState().input, cursorCol: 0, preferredCursorCol: 0 },
    });
    store.getState().backspace();
    expect(store.getState().input.lines).toEqual(['ab']);
    expect(store.getState().input.cursorRow).toBe(0);
    expect(store.getState().input.cursorCol).toBe(1);
  });
});

describe('Input buffer — setViewport', () => {
  it('sets viewport dimensions', () => {
    const store = makeStore();
    store.getState().setViewport(120, 40);
    expect(store.getState().input.viewportWidth).toBe(120);
    expect(store.getState().input.viewportHeight).toBe(40);
  });

  it('is a no-op when dimensions unchanged', () => {
    const store = makeStore();
    store.getState().setViewport(80, 24);
    const state1 = store.getState().input;
    store.getState().setViewport(80, 24);
    const state2 = store.getState().input;
    expect(state1).toBe(state2); // same reference
  });
});

describe('Session management', () => {
  it('cancels only approvals owned by the released session', () => {
    const store = makeStore();
    const resolveReleased = mock();
    const resolveOther = mock();
    const releasedApproval = {
      originSessionId: 'workflow-child',
      toolCall: { toolCallId: 'tool-released' },
      permissionOptions: [],
      resolve: resolveReleased,
    };
    const otherApproval = {
      sessionId: 'other-child',
      toolCall: { toolCallId: 'tool-other' },
      permissionOptions: [],
      resolve: resolveOther,
    };
    store.setState({
      approvalQueue: [releasedApproval, otherApproval],
      pendingApproval: releasedApproval,
    });

    store.getState().cancelSessionApprovals('workflow-child');

    expect(resolveReleased).toHaveBeenCalledWith({ outcome: 'cancelled' });
    expect(resolveReleased).toHaveBeenCalledTimes(1);
    expect(resolveOther).not.toHaveBeenCalled();
    expect(store.getState().approvalQueue).toEqual([otherApproval]);
    expect(store.getState().pendingApproval).toBe(otherApproval);
  });

  it('addSubagentSession adds a session', () => {
    const store = makeStore();
    store.getState().addSubagentSession({
      sessionId: 'sub-1',
      agentName: 'worker',
      status: 'working',
    });
    expect(store.getState().sessions.get('sub-1')).toBeDefined();
    expect(store.getState().sessions.get('sub-1')!.status).toBe('busy');
  });

  it('updateSubagentSession updates status', () => {
    const store = makeStore();
    store.getState().addSubagentSession({
      sessionId: 'sub-2',
      agentName: 'worker',
      status: 'working',
    });
    store.getState().updateSubagentSession('sub-2', 'idle');
    expect(store.getState().sessions.get('sub-2')!.status).toBe('idle');
  });

  it('pushSessionEvent adds event to buffer', () => {
    const store = makeStore();
    store.getState().pushSessionEvent('s1', { type: 'content', text: 'hi' });
    expect(store.getState().sessionEventBuffer['s1']).toHaveLength(1);
  });

  it('addSession backfills placeholder agentName once the session arrives', async () => {
    // Reproduces the race where a stage's first tool call lands BEFORE the
    // subagent_list_update for that session — the resolver in app-store
    // stamps the message with the raw sessionId as a placeholder, then the
    // backfill in addSession rewrites it to the human-readable stage name.
    // Without the backfill, the lite render's subagentSummariesById walk
    // misses that stage's summaries and the final block is missing the row.
    const store = makeStore();
    store.setState({
      sessionId: 'main-session',
      currentAgent: { name: 'main' },
    });
    const handler = store.getState().createStreamEventHandler();
    // Stage tool arrives before its session is registered → resolver stamps
    // agentName = sessionId (the placeholder fallback).
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-1',
      name: 'summary',
      kind: 'other',
      args: { taskResult: 'stage A done' },
      sessionId: 'sub-session-uuid',
    });
    await new Promise((r) => setTimeout(r, 50));
    const before = store.getState().messages.find((m: any) => m.id === 'tc-1');
    expect(before?.agentName).toBe('sub-session-uuid');
    // Now register the session — the backfill should rewrite the placeholder
    // to the real stage name.
    store.getState().addSession({
      id: 'sub-session-uuid',
      name: 'stage-a',
      status: 'busy',
      type: 'ephemeral',
      created: new Date(),
      lastActivity: new Date(),
    } as any);
    const after = store.getState().messages.find((m: any) => m.id === 'tc-1');
    expect(after?.agentName).toBe('stage-a');
  });

  it('addSession clears terminated sessions when new active arrives', () => {
    const store = makeStore();
    store.setState({
      sessions: new Map([
        [
          'old',
          {
            id: 'old',
            name: 'old',
            status: 'terminated',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
          },
        ],
      ]) as any,
    });
    store.getState().addSession({
      id: 'new',
      name: 'new',
      status: 'busy',
      type: 'ephemeral',
      created: new Date(),
      lastActivity: new Date(),
    } as any);
    expect(store.getState().sessions.has('old')).toBe(false);
    expect(store.getState().sessions.has('new')).toBe(true);
  });

  it('addSession preserves terminated workflow sessions', () => {
    const store = makeStore();
    store.setState({
      sessions: new Map([
        [
          'workflow-child',
          {
            id: 'workflow-child',
            name: 'completed-step',
            status: 'terminated',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
            lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
          },
        ],
      ]) as any,
    });

    store.getState().addSession({
      id: 'new',
      name: 'new',
      status: 'busy',
      type: 'ephemeral',
      created: new Date(),
      lastActivity: new Date(),
    } as any);

    expect(store.getState().sessions.has('workflow-child')).toBe(true);
    expect(store.getState().sessions.has('new')).toBe(true);
  });

  it('session tools preserve workflow-owned sessions and transcripts', async () => {
    const store = makeStore();
    const workflowSession = {
      id: 'workflow-child',
      name: 'completed-step',
      status: 'terminated',
      type: 'ephemeral',
      created: new Date(),
      lastActivity: new Date(),
      lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
    };
    store.setState({
      sessionId: 'main-session',
      sessions: new Map([
        ['workflow-child', workflowSession],
        [
          'stale-subagent',
          {
            id: 'stale-subagent',
            name: 'stale',
            status: 'terminated',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
          },
        ],
      ]) as any,
      sessionEventBuffer: {
        'workflow-child': [{ type: AgentEventType.Content, text: 'kept' }],
        'stale-subagent': [{ type: AgentEventType.Content, text: 'removed' }],
      },
    });

    store.getState().createStreamEventHandler()({
      type: AgentEventType.ToolCall,
      id: 'new-crew',
      name: 'subagent',
      args: {},
    });
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(store.getState().sessions.has('workflow-child')).toBe(true);
    expect(store.getState().sessionEventBuffer['workflow-child']).toHaveLength(
      1
    );
    expect(store.getState().sessions.has('stale-subagent')).toBe(false);
    expect(
      store.getState().sessionEventBuffer['stale-subagent']
    ).toBeUndefined();
  });

  it('updateSession updates existing session', () => {
    const store = makeStore();
    store.setState({
      sessions: new Map([
        [
          's1',
          {
            id: 's1',
            name: 'test',
            status: 'idle',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
          },
        ],
      ]) as any,
    });
    store.getState().updateSession('s1', { status: 'busy' });
    expect(store.getState().sessions.get('s1')!.status).toBe('busy');
  });

  it('removeSession removes session and cleans up', () => {
    const store = makeStore();
    store.setState({
      sessions: new Map([
        [
          's1',
          {
            id: 's1',
            name: 'test',
            status: 'idle',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
          },
        ],
      ]) as any,
      activeSessionId: 's1',
    });
    store.getState().removeSession('s1');
    expect(store.getState().sessions.has('s1')).toBe(false);
    expect(store.getState().activeSessionId).toBe('');
  });

  it('setActiveSession sets the active session', () => {
    const store = makeStore();
    store.getState().setActiveSession('s1');
    expect(store.getState().activeSessionId).toBe('s1');
  });

  it('toggleCrewMonitor toggles visibility', () => {
    const store = makeStore();
    expect(store.getState().crewMonitorVisible).toBe(false);
    store.getState().toggleCrewMonitor();
    expect(store.getState().crewMonitorVisible).toBe(true);
  });
});

describe('navigateHistory', () => {
  it('returns the most recent command from history', () => {
    // Seed the shared CommandHistory singleton so this test does not depend on
    // ambient state (the on-disk history file, or whatever HOME a previously
    // run test file pointed at in the same process).
    const history = CommandHistory.getInstance();
    history.clear();
    history.add('/help');
    const store = makeStore();
    expect(store.getState().navigateHistory('up')).toBe('/help');
  });
});

describe('setMode', () => {
  it('sets the mode', () => {
    const store = makeStore();
    store.getState().setMode('session-view');
    expect(store.getState().mode).toBe('session-view');
  });
});

describe('Input buffer — newline', () => {
  it('splits line at cursor position', () => {
    const store = makeStore();
    store.getState().insert('hello');
    store.setState({
      input: { ...store.getState().input, cursorCol: 2, preferredCursorCol: 2 },
    });
    store.getState().newline();
    expect(store.getState().input.lines).toEqual(['he', 'llo']);
    expect(store.getState().input.cursorRow).toBe(1);
    expect(store.getState().input.cursorCol).toBe(0);
  });
});

describe('Input buffer — clearWord and clearLine', () => {
  it('clearWord is a no-op (todo)', () => {
    const store = makeStore();
    store.getState().insert('hello world');
    store.getState().clearWord();
    // Currently a no-op
    expect(store.getState().input.lines[0]).toBe('hello world');
  });

  it('clearLine is a no-op (todo)', () => {
    const store = makeStore();
    store.getState().insert('hello');
    store.getState().clearLine();
    expect(store.getState().input.lines[0]).toBe('hello');
  });
});

describe('Input buffer — moveCursor', () => {
  it('moveCursor is a no-op (todo)', () => {
    const store = makeStore();
    store.getState().insert('hi');
    store.getState().moveCursor('left');
    // Currently a no-op
    expect(store.getState().input.cursorCol).toBe(2);
  });
});

describe('cleanupTerminatedSession', () => {
  it('cancels approvals for terminated session', () => {
    const store = makeStore();
    const resolve = mock(() => {});
    store.setState({
      sessions: new Map([
        [
          's1',
          {
            id: 's1',
            name: 'worker',
            status: 'busy',
            type: 'ephemeral',
            created: new Date(),
            lastActivity: new Date(),
          },
        ],
      ]) as any,
      approvalQueue: [
        { sessionId: 's1', toolCall: { toolCallId: 'tc1' }, resolve },
      ],
      pendingApproval: {
        sessionId: 's1',
        toolCall: { toolCallId: 'tc1' },
        resolve,
      },
      messages: [
        {
          id: 'tc1',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{}',
          agentName: 'worker',
        },
      ],
    });
    store.getState().cleanupTerminatedSession('s1');
    expect(resolve).toHaveBeenCalledWith({ outcome: 'cancelled' });
    expect(store.getState().pendingApproval).toBeNull();
    const msg = store.getState().messages.find((m: any) => m.id === 'tc1');
    expect(msg!.isFinished).toBe(true);
  });
});

describe('setKasCommands', () => {
  it('sets KAS commands', async () => {
    const { KasCommandName } = await import('../../kas-commands');
    const store = makeStore();
    store
      .getState()
      .setKasCommands([
        { name: KasCommandName.Help, description: 'Show help' },
      ]);
    expect(store.getState().kasCommands).toHaveLength(1);
  });
});

describe('setPrompts', () => {
  it('sets prompts', () => {
    const store = makeStore();
    store.getState().setPrompts([
      {
        name: 'test',
        arguments: [],
        source: { kind: 'mcp', serverName: 'srv' },
      },
    ]);
    expect(store.getState().prompts).toHaveLength(1);
    expect(store.getState().prompts[0]!.source).toEqual({
      kind: 'mcp',
      serverName: 'srv',
    });
  });
});

describe('setSkills', () => {
  it('sets skills', () => {
    const store = makeStore();
    store
      .getState()
      .setSkills([{ name: 'sop', source: { kind: 'agent-config' } }]);
    expect(store.getState().skills).toHaveLength(1);
    expect(store.getState().skills[0]!.source).toEqual({
      kind: 'agent-config',
    });
  });
});

describe('setSteering', () => {
  it('sets steering', () => {
    const store = makeStore();
    store
      .getState()
      .setSteering([
        { name: 'project-context', source: { kind: 'workspace' } },
      ]);
    expect(store.getState().steering).toHaveLength(1);
    expect(store.getState().steering[0]!.source).toEqual({
      kind: 'workspace',
    });
  });
});

describe('clearCommandInput', () => {
  it('clears all command input state', () => {
    const store = makeStore();
    store.setState({
      commandInputValue: 'test',
      activeTrigger: '/' as any,
      filePickerHasResults: true,
      promptHint: 'hint',
      commandShadowText: 'shadow',
    });
    store.getState().clearCommandInput();
    expect(store.getState().commandInputValue).toBe('');
    expect(store.getState().activeTrigger).toBeNull();
    expect(store.getState().filePickerHasResults).toBe(false);
    expect(store.getState().promptHint).toBeNull();
    expect(store.getState().commandShadowText).toBeNull();
  });
});

describe('Stream event handler — thinkingMs', () => {
  it('persists thinkingMs on the think→content first-flush path', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Thought,
      id: 'th-content',
      content: { type: ContentType.Text, text: 'reasoning' },
    });
    handler({
      type: AgentEventType.Content,
      id: 'content-after-thought',
      content: { type: ContentType.Text, text: 'answer' },
    });
    await new Promise((r) => setTimeout(r, 30));

    const model = store
      .getState()
      .messages.find((m: any) => m.role === MessageRole.Model);
    expect(model).toBeDefined();
    expect(model!.content).toBe('answer');
    expect((model as any).thinking).toBe('reasoning');
    expect(typeof (model as any).thinkingMs).toBe('number');
    expect((model as any).thinkingMs).toBeGreaterThanOrEqual(0);
  });

  it('persists thinkingMs on the think→tool-call path', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.Thought,
      id: 'th-1',
      content: { type: ContentType.Text, text: 'reasoning' },
    });
    // Let the 16ms thinking flush fire so pendingContentFlush is null,
    // mirroring the real think→tool-call timing.
    await new Promise((r) => setTimeout(r, 30));
    handler({
      type: AgentEventType.ToolCall,
      id: 'tc-think',
      name: 'fs_read',
      kind: 'read',
      args: { path: '/tmp/x.ts' },
    });
    await new Promise((r) => setTimeout(r, 30));

    const model = store
      .getState()
      .messages.find((m: any) => m.role === MessageRole.Model);
    expect(model).toBeDefined();
    expect(typeof (model as any).thinkingMs).toBe('number');
    expect((model as any).thinkingMs).toBeGreaterThanOrEqual(0);
  });
});

describe('Stream event handler — ToolsUpdate', () => {
  it('populates toolsList from the event', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    const tools = [
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: '@git/status', source: 'mcp', description: 'git status' },
    ];
    handler({ type: AgentEventType.ToolsUpdate, tools });
    await new Promise((r) => setTimeout(r, 20));
    expect(store.getState().toolsList).toEqual(tools);
  });

  it('replaces the previous toolsList wholesale (no merge)', async () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolsUpdate,
      tools: [{ name: 'read', source: 'builtin', description: 'a' }],
    });
    handler({
      type: AgentEventType.ToolsUpdate,
      tools: [{ name: 'write', source: 'builtin', description: 'b' }],
    });
    await new Promise((r) => setTimeout(r, 20));
    expect(store.getState().toolsList).toEqual([
      { name: 'write', source: 'builtin', description: 'b' },
    ]);
  });
});

describe('Stream event handler — SessionRosterDelta', () => {
  const attach = (store: ReturnType<typeof makeStore>) =>
    store.setState({ sessionId: 's1' } as never);

  it('derives cloudSessionStatus for the attached session from a roster delta', async () => {
    const store = makeStore();
    attach(store);
    expect(store.getState().cloudSessionStatus).toBeNull();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [{ sessionId: 's1', status: 'provisioning' }],
        deleted: [],
      },
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudSessionStatus).toBe('provisioning');
  });

  it('reflects a later status change and captures a provisioning failure', async () => {
    const store = makeStore();
    attach(store);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [{ sessionId: 's1', status: 'provisioning' }],
        deleted: [],
      },
    });
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [
          {
            sessionId: 's1',
            status: 'failed',
            provisioningFailure: { code: 'backend' },
          },
        ],
        deleted: [],
      },
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudSessionStatus).toBe('failed');
    expect(store.getState().cloudProvisioningFailure).toEqual({
      code: 'backend',
    });
  });

  it('resets the status to null when the attached session is retracted', async () => {
    const store = makeStore();
    attach(store);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [{ sessionId: 's1', status: 'in_progress' }],
        deleted: [],
      },
    });
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: { upserted: [], deleted: ['s1'] },
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudSessionStatus).toBeNull();
  });

  it('ignores status changes for sessions the client is not attached to', async () => {
    const store = makeStore();
    attach(store);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [{ sessionId: 'other', status: 'in_progress' }],
        deleted: [],
      },
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudSessionStatus).toBeNull();
    // The roster still tracks the other session for observers.
    expect(store.getState().sessionRoster.get('other')?.status).toBe(
      'in_progress'
    );
  });

  it('is also handled on the compaction-event dispatch path', async () => {
    const store = makeStore();
    attach(store);
    store.getState().handleCompactionEvent({
      type: AgentEventType.SessionRosterDelta,
      delta: {
        upserted: [{ sessionId: 's1', status: 'waiting_on_user' }],
        deleted: [],
      },
    } as never);
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudSessionStatus).toBe('waiting_on_user');
  });
});

describe('Stream event handler — SessionRepositoriesUpdate', () => {
  it('applies the pushed repo set to the footer when a cloud session is active', async () => {
    const store = makeStore();
    store.getState().setCloudSessionActive(true);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRepositoriesUpdate,
      repositories: [
        { name: 'acme/banana-service', branch: 'main' },
        { name: 'acme/second-repo' },
      ],
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudRepo).toBe('acme/banana-service');
    expect(store.getState().cloudBranch).toBe('main');
    expect(store.getState().cloudExtraRepos).toBe(1);
  });

  it('an empty pushed set clears the footer (detach-all)', async () => {
    const store = makeStore();
    store.getState().setCloudSessionActive(true);
    store.getState().applyRepoFooter(['acme/banana-service']);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRepositoriesUpdate,
      repositories: [],
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudRepo).toBeNull();
    expect(store.getState().cloudBranch).toBeNull();
  });

  it('ignores the push when no cloud session is active (local footer untouched)', async () => {
    const store = makeStore();
    expect(store.getState().cloudSessionActive).toBe(false);
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.SessionRepositoriesUpdate,
      repositories: [{ name: 'acme/banana-service' }],
    });
    await new Promise((r) => setTimeout(r, 10));
    expect(store.getState().cloudRepo).toBeNull();
  });
});

describe('cloudRepo slice (footer location)', () => {
  it('defaults to null and is updated by setCloudRepo', () => {
    const store = makeStore();
    expect(store.getState().cloudRepo).toBeNull();
    store.getState().setCloudRepo('acme/banana-service');
    expect(store.getState().cloudRepo).toBe('acme/banana-service');
    store.getState().setCloudRepo(null);
    expect(store.getState().cloudRepo).toBeNull();
  });
});

describe('app-store — showCloudQuitPrompt slice', () => {
  it('defaults closed and toggles via setShowCloudQuitPrompt', () => {
    const store = makeStore();
    expect(store.getState().showCloudQuitPrompt).toBe(false);
    store.getState().setShowCloudQuitPrompt(true);
    expect(store.getState().showCloudQuitPrompt).toBe(true);
    store.getState().setShowCloudQuitPrompt(false);
    expect(store.getState().showCloudQuitPrompt).toBe(false);
  });

  it('resumes a queue when the cloud quit prompt is cancelled', async () => {
    const store = makeStore();
    const processQueue = mock(async () => {});
    store.setState({
      showCloudQuitPrompt: true,
      queuedMessages: ['/tui'],
      processQueue,
    });

    store.getState().setShowCloudQuitPrompt(false);
    await new Promise((resolve) => queueMicrotask(resolve));

    expect(processQueue).toHaveBeenCalledTimes(1);
  });
});
