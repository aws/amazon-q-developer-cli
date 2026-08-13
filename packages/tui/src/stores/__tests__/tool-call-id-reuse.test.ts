/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
/**
 * Store-level regression tests for tool-call id reuse.
 *
 * Some serving paths (observed with GPT models) emit tool-call ids that are
 * only unique within a single request — call_0, call_1, … reset every turn —
 * so a multi-turn session reuses ids. The store keys tool rows by id; without
 * disambiguation a reused id silently rewrites the previous turn's finished
 * row (already flushed to static scrollback): the new tool never renders and
 * its approval prompt shows stale data. The store derives unique internal ids
 * (`wireId#N`) statelessly from the message list at event ingestion.
 */
import { describe, it, expect, mock, afterAll } from 'bun:test';
import { AgentEventType, ApprovalOptionId } from '../../types/agent-events';

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

const { createAppStore, MessageRole, allocateToolCallId, resolveToolCallId } =
  await import('../app-store');
const { Kiro } = await import('../../kiro');

function makeStore() {
  const store = createAppStore({ kiro: new Kiro() });
  store.setState({ isInitialized: true });
  return store;
}

const toolCall = (id: string, command: string) => ({
  type: AgentEventType.ToolCall,
  id,
  name: 'shell',
  args: { command },
});

const finished = (id: string) => ({
  type: AgentEventType.ToolCallFinished,
  id,
  result: { status: 'success', output: 'ok' },
});

const approval = (toolCallId: string) => ({
  type: AgentEventType.ApprovalRequest,
  value: {
    toolCall: { toolCallId, title: 'shell' },
    permissionOptions: [
      { kind: ApprovalOptionId.AllowOnce, name: 'Allow', optionId: 'accept' },
    ],
    resolve: () => {},
  },
});

const toolRows = (store) =>
  store.getState().messages.filter((m) => m.role === MessageRole.ToolUse);

const question = (toolCallId: string, text: string) => ({
  type: AgentEventType.QuestionRequest,
  value: {
    sessionId: 'session-1',
    toolCallId,
    question: text,
    options: [{ title: 'Yes' }],
    resolve: () => {},
  },
});

describe('tool-call id reuse across turns', () => {
  it('a wire id reused after finish creates a NEW row; unique ids pass through', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    handler(toolCall('call_0', 'echo hello'));
    handler(finished('call_0'));
    handler(toolCall('call_0', 'rm /tmp/foo.txt'));

    const rows = toolRows(store);
    expect(rows.map((m) => m.id)).toEqual(['call_0', 'call_0#1']);
    // The finished first row is untouched; the new row carries the new args.
    expect(rows[0].isFinished).toBe(true);
    expect(rows[0].content).toContain('echo hello');
    expect(rows[1].isFinished).toBeFalsy();
    expect(rows[1].content).toContain('rm /tmp/foo.txt');
  });

  it('follow-up finish and approval for a reused wire id target the newest row', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    handler(toolCall('call_0', 'echo hello'));
    handler(finished('call_0'));
    handler(toolCall('call_0', 'rm /tmp/foo.txt'));
    handler(approval('call_0'));

    expect(store.getState().pendingApproval?.toolCall?.toolCallId).toBe(
      'call_0#1'
    );

    handler(finished('call_0'));
    const rows = toolRows(store);
    expect(rows[1].id).toBe('call_0#1');
    expect(rows[1].isFinished).toBe(true);
  });

  it('a re-emitted tool_call for a still-active id merges instead of forking', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    // Streaming chunk preview followed by the full tool_call (same id, no
    // finish in between) must stay a single row.
    handler({
      type: AgentEventType.ToolCall,
      id: 'call_1',
      name: 'shell',
      args: {},
    });
    handler(toolCall('call_1', 'git status'));

    const rows = toolRows(store);
    expect(rows).toHaveLength(1);
    expect(rows[0].id).toBe('call_1');
    expect(rows[0].content).toContain('git status');
  });

  it('a synthesized ToolCall replay of a finished call resolves to the existing row (no ghost)', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    // A rejected tool: row exists and was finished locally (cancelled), then
    // the client synthesizes a ToolCall replay off the backend's failed
    // update. It must merge into the existing row, not fork a #1 ghost.
    handler(toolCall('tool-alpha', 'echo a'));
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-alpha',
      result: { status: 'cancelled' },
    });
    handler({ ...toolCall('tool-alpha', 'echo a'), synthesized: true });

    expect(toolRows(store).map((m) => m.id)).toEqual(['tool-alpha']);
  });

  it('a question for a reused wire id opens a new row instead of reviving a finished one', () => {
    const store = makeStore();
    store.setState({ sessionId: 'session-1' });
    const handler = store.getState().createStreamEventHandler();

    handler(toolCall('call_0', 'echo hello'));
    handler(finished('call_0'));
    handler(question('call_0', 'Proceed?'));

    const rows = toolRows(store);
    expect(rows.map((m) => m.id)).toEqual(['call_0', 'call_0#1']);
    expect(rows[0].isQuestion).toBeFalsy();
    expect(rows[0].isFinished).toBe(true);
    expect(rows[1].isQuestion).toBe(true);
    expect(store.getState().pendingQuestion?.toolCallId).toBe('call_0#1');
  });

  it('a question for a still-active call attaches to that row', () => {
    const store = makeStore();
    store.setState({ sessionId: 'session-1' });
    const handler = store.getState().createStreamEventHandler();

    handler(toolCall('call_0', 'echo hello'));
    handler(question('call_0', 'Proceed?'));

    const rows = toolRows(store);
    expect(rows).toHaveLength(1);
    expect(rows[0].id).toBe('call_0');
    expect(rows[0].isQuestion).toBe(true);
    expect(store.getState().pendingQuestion?.toolCallId).toBe('call_0');
  });

  it('generations keep incrementing on repeated reuse', () => {
    const store = makeStore();
    const handler = store.getState().createStreamEventHandler();

    for (const cmd of ['a', 'b', 'c']) {
      handler(toolCall('call_0', cmd));
      handler(finished('call_0'));
    }

    expect(toolRows(store).map((m) => m.id)).toEqual([
      'call_0',
      'call_0#1',
      'call_0#2',
    ]);
    expect(toolRows(store).every((m) => m.isFinished)).toBe(true);
  });
});

describe('allocateToolCallId / resolveToolCallId', () => {
  const row = (id: string, isFinished: boolean) => ({
    role: MessageRole.ToolUse,
    id,
    isFinished,
  });

  it('passes through ids with no matching row', () => {
    expect(allocateToolCallId([], 'call_0')).toBe('call_0');
    expect(resolveToolCallId([], 'call_0')).toBe('call_0');
  });

  it('does not cross-match ids sharing a prefix', () => {
    const messages = [row('call_01', true)];
    expect(allocateToolCallId(messages, 'call_0')).toBe('call_0');
    expect(resolveToolCallId(messages, 'call_0')).toBe('call_0');
  });

  it('allocates the next generation from the newest matching row', () => {
    const messages = [row('call_0', true), row('call_0#1', true)];
    expect(allocateToolCallId(messages, 'call_0')).toBe('call_0#2');
    expect(resolveToolCallId(messages, 'call_0')).toBe('call_0#1');
  });
});
