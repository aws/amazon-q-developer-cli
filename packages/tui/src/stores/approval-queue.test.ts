import { describe, it, expect, mock } from 'bun:test';
import { createAppStore, MessageRole, ToolUseStatus } from './app-store';
import { AgentEventType, ApprovalOptionId } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

function makeToolCallEvent(
  id: string,
  name: string,
  command: string
): AgentStreamEvent {
  return {
    type: AgentEventType.ToolCall,
    id,
    name: 'execute_bash',
    kind: 'shell' as any,
    args: { command },
  } as AgentStreamEvent;
}

function makeApprovalEvent(
  toolCallId: string,
  resolve?: (r: any) => void
): AgentStreamEvent {
  return {
    type: AgentEventType.ApprovalRequest,
    value: {
      toolCall: { toolCallId },
      permissionOptions: [
        {
          kind: ApprovalOptionId.AllowOnce,
          name: 'Allow Once',
          optionId: 'allow_once',
        },
        {
          kind: ApprovalOptionId.RejectOnce,
          name: 'Reject Once',
          optionId: 'reject_once',
        },
      ],
      resolve: resolve ?? (() => {}),
    },
  } as AgentStreamEvent;
}

function makeToolFinishedEvent(id: string): AgentStreamEvent {
  return {
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'success', output: 'ok' },
  } as AgentStreamEvent;
}

function createTestStore() {
  const mockKiro = new Kiro();
  return createAppStore({ kiro: mockKiro });
}

describe('Approval queue', () => {
  it('queues multiple approval requests and serves them one at a time', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'git log'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'git branch'));

    handler(makeApprovalEvent('t1'));
    handler(makeApprovalEvent('t2'));
    handler(makeApprovalEvent('t3'));

    const state = store.getState();
    expect(state.approvalQueue).toHaveLength(3);
    expect(state.pendingApproval?.toolCall.toolCallId).toBe('t1');
  });

  it('advances to next approval after responding', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolves: Array<(r: any) => void> = [];

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'git log'));

    handler(makeApprovalEvent('t1', (r) => resolves.push(r)));
    handler(makeApprovalEvent('t2', (r) => resolves.push(r)));

    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe('t1');

    store.getState().respondToApproval('allow_once');

    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe('t2');
    expect(store.getState().approvalQueue).toHaveLength(1);
  });

  it('cancels all queued approvals at once', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolved: any[] = [];

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'git log'));

    handler(makeApprovalEvent('t1', (r) => resolved.push(r)));
    handler(makeApprovalEvent('t2', (r) => resolved.push(r)));

    store.getState().cancelApproval();

    expect(store.getState().pendingApproval).toBeNull();
    expect(store.getState().approvalQueue).toHaveLength(0);
    expect(resolved).toHaveLength(2);
    expect(resolved[0].outcome).toBe('cancelled');
    expect(resolved[1].outcome).toBe('cancelled');
  });
});

describe('Tool approval status tracking', () => {
  it('sets Pending status when approval is requested', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));

    const toolMsg = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    expect(toolMsg).toBeDefined();
    if (toolMsg?.role === MessageRole.ToolUse) {
      expect(toolMsg.status).toBe(ToolUseStatus.Pending);
    }
  });

  it('sets Approved status when user approves', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));

    store.getState().respondToApproval('allow_once');

    const toolMsg = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    if (toolMsg?.role === MessageRole.ToolUse) {
      expect(toolMsg.status).toBe(ToolUseStatus.Approved);
    }
  });

  it('sets Rejected status when user rejects', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));

    store.getState().respondToApproval('reject_once');

    const toolMsg = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    if (toolMsg?.role === MessageRole.ToolUse) {
      expect(toolMsg.status).toBe(ToolUseStatus.Rejected);
      expect(toolMsg.isFinished).toBe(true);
    }
  });

  it('tool stays visually unfinished while pending even if ToolCallFinished arrives', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));
    // ToolCallFinished arrives while still pending
    handler(makeToolFinishedEvent('t1'));

    const toolMsg = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    if (toolMsg?.role === MessageRole.ToolUse) {
      // Store has isFinished true and status Pending
      expect(toolMsg.isFinished).toBe(true);
      expect(toolMsg.status).toBe(ToolUseStatus.Pending);
      // Display logic: effectiveFinished = isFinished && status !== Pending = false
      const effectiveFinished =
        toolMsg.isFinished && toolMsg.status !== ToolUseStatus.Pending;
      expect(effectiveFinished).toBe(false);
    }
  });

  it('tool becomes visually finished after approval clears Pending status', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));
    handler(makeToolFinishedEvent('t1'));

    // Approve the tool
    store.getState().respondToApproval('allow_once');

    const toolMsg = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    if (toolMsg?.role === MessageRole.ToolUse) {
      expect(toolMsg.isFinished).toBe(true);
      expect(toolMsg.status).toBe(ToolUseStatus.Approved);
      const effectiveFinished =
        toolMsg.isFinished && toolMsg.status !== ToolUseStatus.Pending;
      expect(effectiveFinished).toBe(true);
    }
  });

  it('new ToolCall does not auto-finish previous pending tools', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));
    // Second tool call arrives
    handler(makeToolCallEvent('t2', 'execute_bash', 'git log'));

    const t1 = store
      .getState()
      .messages.find((m) => m.role === MessageRole.ToolUse && m.id === 't1');
    if (t1?.role === MessageRole.ToolUse) {
      // t1 should NOT be auto-finished by t2 arriving
      expect(t1.isFinished).toBeFalsy();
      expect(t1.status).toBe(ToolUseStatus.Pending);
    }
  });
});
