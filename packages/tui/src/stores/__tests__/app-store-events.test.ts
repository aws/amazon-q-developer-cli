/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
import { describe, it, expect, mock, afterAll } from 'bun:test';
import { AgentEventType, ContentType } from '../../types/agent-events';

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
    const parsed = JSON.parse(msg!.content);
    expect(parsed.command).toBe('strReplace');
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
      toolContent: [{ path: '/tmp/x.ts', oldText: 'old', newText: 'new' }],
    });
    await new Promise((r) => setTimeout(r, 50));
    const msg = store.getState().messages.find((m: any) => m.id === 'tc-3');
    const parsed = JSON.parse(msg!.content);
    expect(parsed.path).toBe('/tmp/x.ts');
    expect(parsed.oldStr).toBe('old');
    expect(parsed.newStr).toBe('new');
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
    const parsed = JSON.parse(msg!.content);
    expect(parsed.command).toBe('insert');
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
        permissionOptions: [{ optionId: 'allow_once', label: 'Allow' }],
        resolve,
        sessionId: 'sub-session',
      },
    });
    expect(resolve).toHaveBeenCalledWith({
      outcome: 'selected',
      optionId: 'allow_once',
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
});

describe('Stream event handler — RateLimitError', () => {
  it('shows transient alert', () => {
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
  });
});

describe('Stream event handler — ContextUsage', () => {
  it('sets contextUsagePercent', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({ type: AgentEventType.ContextUsage, percent: 85 });
    expect(store.getState().contextUsagePercent).toBe(85);
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
});

describe('Stream event handler — AgentNotFound', () => {
  it('adds to initErrors', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.AgentNotFound,
      requestedAgent: 'missing-agent',
      fallbackAgent: 'default',
    });
    expect(store.getState().initErrors).toHaveLength(1);
    expect(store.getState().initErrors[0].type).toBe('agent_not_found');
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
    expect(store.getState().isProcessing).toBe(true);
  });

  it('handles completed with summary', async () => {
    const store = makeStore();
    store.setState({ isCompacting: true, isProcessing: true });
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
      'Compaction failed'
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

  it('forwards EffortUpdate events', async () => {
    const store = makeStore();
    await store.getState().handleCompactionEvent({
      type: AgentEventType.EffortUpdate,
      effort: 'low',
    });
    expect(store.getState().currentEffort).toBe('low');
  });
});

describe('handleTurnSummaryEvent', () => {
  it('aggregates metering usage and stores summary', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hi' }],
    });
    store.getState().handleTurnSummaryEvent({
      type: AgentEventType.TurnSummary,
      meteringUsage: [
        { unitPlural: 'credits', value: 1.5 },
        { unitPlural: 'credits', value: 0.5 },
      ],
      turnDurationMs: 5000,
    });
    const summaries = store.getState().turnSummaries;
    expect(summaries.get('u1')).toContain('Credits: 2.00');
    expect(summaries.get('u1')).toContain('Time: 5s');
  });

  it('formats time as minutes when >= 60s', () => {
    const store = makeStore();
    store.setState({
      messages: [{ id: 'u2', role: MessageRole.User, content: 'hi' }],
    });
    store.getState().handleTurnSummaryEvent({
      type: AgentEventType.TurnSummary,
      meteringUsage: [{ unitPlural: 'credits', value: 3.0 }],
      turnDurationMs: 125000,
    });
    const summaries = store.getState().turnSummaries;
    expect(summaries.get('u2')).toContain('2m 5s');
  });

  it('does nothing when no user message exists', () => {
    const store = makeStore();
    store.setState({ messages: [] });
    store.getState().handleTurnSummaryEvent({
      type: AgentEventType.TurnSummary,
      meteringUsage: [{ unitPlural: 'credits', value: 1 }],
    });
    expect(store.getState().turnSummaries.size).toBe(0);
  });
});

describe('queueMessage and processQueue', () => {
  it('queues a message', () => {
    const store = makeStore();
    store.getState().queueMessage('hello');
    expect(store.getState().queuedMessages).toEqual(['hello']);
  });

  it('ignores empty messages', () => {
    const store = makeStore();
    store.getState().queueMessage('   ');
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('clearQueue empties the queue', () => {
    const store = makeStore();
    store.getState().queueMessage('a');
    store.getState().queueMessage('b');
    store.getState().clearQueue();
    expect(store.getState().queuedMessages).toEqual([]);
  });
});

describe('removeQueuedMessage', () => {
  it('removes message at index', () => {
    const store = makeStore();
    store.setState({ queuedMessages: ['a', 'b', 'c'] });
    store.getState().removeQueuedMessage(1);
    expect(store.getState().queuedMessages).toEqual(['a', 'c']);
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

  it('addMessage adds to sessionMessages', () => {
    const store = makeStore();
    store.getState().addMessage('s1', { role: 'user', content: 'hi' } as any);
    const msgs = store.getState().sessionMessages.get('s1');
    expect(msgs).toHaveLength(1);
  });

  it('toggleCrewMonitor toggles visibility', () => {
    const store = makeStore();
    expect(store.getState().crewMonitorVisible).toBe(false);
    store.getState().toggleCrewMonitor();
    expect(store.getState().crewMonitorVisible).toBe(true);
  });
});

describe('navigateHistory', () => {
  it('returns a value from history', () => {
    const store = makeStore();
    const result = store.getState().navigateHistory('up');
    // Just verify it doesn't throw — history state depends on other tests
    expect(typeof result === 'string' || result === undefined).toBe(true);
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

describe('setExtensionCommands', () => {
  it('sets extension commands', () => {
    const store = makeStore();
    store
      .getState()
      .setExtensionCommands([
        { name: '/ext', description: 'Extension', source: 'backend' as any },
      ]);
    expect(store.getState().extensionCommands).toHaveLength(1);
  });
});

describe('setPrompts', () => {
  it('sets prompts', () => {
    const store = makeStore();
    store
      .getState()
      .setPrompts([{ name: 'test', arguments: [], serverName: 'srv' }] as any);
    expect(store.getState().prompts).toHaveLength(1);
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
