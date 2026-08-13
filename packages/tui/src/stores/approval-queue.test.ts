import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole, ToolUseStatus } from './app-store';
import { AgentEventType, ApprovalOptionId } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';
import { Kiro } from '../kiro';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../kiro']);

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function makeToolCallEvent(
  id: string,
  name: string,
  command: string
): AgentStreamEvent {
  return {
    type: AgentEventType.ToolCall,
    id,
    name,
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

  it('marks ALL queued tool messages as finished on cancel', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'git log'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'git branch'));

    handler(makeApprovalEvent('t1'));
    handler(makeApprovalEvent('t2'));
    handler(makeApprovalEvent('t3'));

    store.getState().cancelApproval();

    const toolMsgs = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.ToolUse);

    for (const msg of toolMsgs) {
      if (msg.role === MessageRole.ToolUse) {
        expect(msg.isFinished).toBe(true);
        expect(msg.result).toEqual({ status: 'cancelled' });
      }
    }
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

describe('Approval and input preservation', () => {
  it('respondToApproval does not clear commandInputValue', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));

    // Simulate user typing while approval is pending
    store.getState().setCommandInput('hello world');
    expect(store.getState().commandInputValue).toBe('hello world');

    // Respond to approval
    store.getState().respondToApproval('allow_once');

    // Input should be preserved
    expect(store.getState().commandInputValue).toBe('hello world');
  });

  it('cancelApproval does not clear commandInputValue', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'git status'));
    handler(makeApprovalEvent('t1'));

    store.getState().setCommandInput('draft message');

    store.getState().cancelApproval();

    // Input should be preserved after cancel too
    expect(store.getState().commandInputValue).toBe('draft message');
  });
});

function makeApprovalEventWithTrustOptions(
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
          kind: ApprovalOptionId.AllowAlways,
          name: 'Always',
          optionId: 'allow_always',
        },
        {
          kind: ApprovalOptionId.RejectOnce,
          name: 'Reject Once',
          optionId: 'reject_once',
        },
      ],
      trustOptions: [
        {
          label: 'Full command',
          display: 'df -h',
          setting_key: 'allowedCommands',
          patterns: ['df -h'],
        },
        {
          label: 'Base command',
          display: 'df *',
          setting_key: 'allowedCommands',
          patterns: ['df( .*)?'],
        },
      ],
      resolve: resolve ?? (() => {}),
    },
  } as AgentStreamEvent;
}

describe('Trust options (_meta.trustOptions)', () => {
  it('stores trustOptions from approval event', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'df -h'));
    handler(makeApprovalEventWithTrustOptions('t1'));

    const approval = store.getState().pendingApproval;
    expect(approval).toBeDefined();
    expect(approval!.trustOptions).toHaveLength(2);
    expect(approval!.trustOptions![0]!.label).toBe('Full command');
    expect(approval!.trustOptions![1]!.display).toBe('df *');
  });

  it('respondToApproval passes _meta through to resolve', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    let resolved: any = null;

    handler(makeToolCallEvent('t1', 'execute_bash', 'df -h'));
    handler(makeApprovalEventWithTrustOptions('t1', (r) => (resolved = r)));

    const trustOption = {
      label: 'Full command',
      display: 'df -h',
      setting_key: 'allowedCommands',
      patterns: ['df -h'],
    };

    store.getState().respondToApproval('allow_always', undefined, {
      trustOption,
    });

    expect(resolved).toBeDefined();
    expect(resolved.outcome).toBe('selected');
    expect(resolved.optionId).toBe('allow_always');
    expect(resolved._meta).toEqual({ trustOption });
  });

  it('respondToApproval without _meta does not include it in resolve', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    let resolved: any = null;

    handler(makeToolCallEvent('t1', 'execute_bash', 'df -h'));
    handler(makeApprovalEvent('t1', (r) => (resolved = r)));

    store.getState().respondToApproval('allow_once');

    expect(resolved).toBeDefined();
    expect(resolved.outcome).toBe('selected');
    expect(resolved.optionId).toBe('allow_once');
    expect(resolved._meta).toBeUndefined();
  });
});

describe('Trust cascade — allow_always auto-resolves same-tool approvals', () => {
  it('cascades trust to all queued approvals of the same tool', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolved: any[] = [];

    // 5 tool calls of the same tool
    handler(makeToolCallEvent('t1', 'execute_bash', 'cmd1'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'cmd2'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'cmd3'));
    handler(makeToolCallEvent('t4', 'execute_bash', 'cmd4'));
    handler(makeToolCallEvent('t5', 'execute_bash', 'cmd5'));

    handler(makeApprovalEventWithTrustOptions('t1', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t2', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t3', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t4', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t5', (r) => resolved.push(r)));

    expect(store.getState().approvalQueue).toHaveLength(5);

    // Trust the first one (full tool trust, no trustOption in _meta)
    store.getState().respondToApproval('allow_always');

    // All should be resolved, queue empty
    expect(store.getState().approvalQueue).toHaveLength(0);
    expect(store.getState().pendingApproval).toBeNull();
    expect(resolved).toHaveLength(5);

    // First resolved with allow_always
    expect(resolved[0].optionId).toBe('allow_always');
    // Remaining resolved with allow_once (cascaded)
    for (let i = 1; i < 5; i++) {
      expect(resolved[i].optionId).toBe('allow_once');
    }
  });

  it('does NOT cascade when using path-specific trust (trustOption in _meta)', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolved: any[] = [];

    handler(makeToolCallEvent('t1', 'execute_bash', 'cmd1'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'cmd2'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'cmd3'));

    handler(makeApprovalEventWithTrustOptions('t1', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t2', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t3', (r) => resolved.push(r)));

    // Trust with a specific trustOption (path-level, not full tool trust)
    store.getState().respondToApproval('allow_always', undefined, {
      trustOption: {
        label: 'Full command',
        setting_key: 'allowedCommands',
        patterns: ['cmd1'],
      },
    });

    // Only the first should be resolved; others remain queued
    expect(store.getState().approvalQueue).toHaveLength(2);
    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe('t2');
    expect(resolved).toHaveLength(1);
    expect(resolved[0].optionId).toBe('allow_always');
  });

  it('does NOT cascade to approvals of a different tool', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolved: any[] = [];

    // Mix of tool types
    handler(makeToolCallEvent('t1', 'execute_bash', 'cmd1'));
    handler(makeToolCallEvent('t2', 'fs_write', 'write something'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'cmd3'));

    handler(makeApprovalEventWithTrustOptions('t1', (r) => resolved.push(r)));
    handler(makeApprovalEvent('t2', (r) => resolved.push(r)));
    handler(makeApprovalEventWithTrustOptions('t3', (r) => resolved.push(r)));

    // Trust execute_bash
    store.getState().respondToApproval('allow_always');

    // t1 and t3 resolved (same tool), t2 remains (different tool)
    expect(store.getState().approvalQueue).toHaveLength(1);
    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe('t2');
    expect(resolved).toHaveLength(2);
    expect(resolved[0].optionId).toBe('allow_always'); // t1
    expect(resolved[1].optionId).toBe('allow_once'); // t3 cascaded
  });

  it('marks all cascaded tool messages as Approved', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'cmd1'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'cmd2'));
    handler(makeToolCallEvent('t3', 'execute_bash', 'cmd3'));

    handler(makeApprovalEventWithTrustOptions('t1'));
    handler(makeApprovalEventWithTrustOptions('t2'));
    handler(makeApprovalEventWithTrustOptions('t3'));

    store.getState().respondToApproval('allow_always');

    const toolMsgs = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.ToolUse);

    for (const msg of toolMsgs) {
      if (msg.role === MessageRole.ToolUse) {
        expect(msg.status).toBe(ToolUseStatus.Approved);
      }
    }
  });

  it('does NOT cascade on allow_once', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('t1', 'execute_bash', 'cmd1'));
    handler(makeToolCallEvent('t2', 'execute_bash', 'cmd2'));

    handler(makeApprovalEventWithTrustOptions('t1'));
    handler(makeApprovalEventWithTrustOptions('t2'));

    store.getState().respondToApproval('allow_once');

    // Only t1 resolved, t2 still pending
    expect(store.getState().approvalQueue).toHaveLength(1);
    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe('t2');
  });
});

describe('KAS trust cascade — originSessionId discriminates cross-agent approvals', () => {
  function makeKasApproval(
    toolCallId: string,
    originSessionId: string | undefined,
    resolve: (r: any) => void
  ) {
    return {
      toolCall: { toolCallId },
      originSessionId,
      permissionOptions: [
        {
          kind: ApprovalOptionId.AllowOnce,
          name: 'Allow Once',
          optionId: 'accept',
        },
        {
          kind: ApprovalOptionId.AllowAlways,
          name: 'Always',
          optionId: 'always-accept',
        },
      ],
      consentContext: {
        capability: 'shell',
        resource: 'echo hi',
        workspaceRoot: '/ws',
      },
      resolve,
    };
  }

  function setupTwoApprovals(originA: string | undefined, originB: string) {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const resolveA = mock((_r: any) => {});
    const resolveB = mock((_r: any) => {});
    const a = makeKasApproval('parent-shell', originA, resolveA);
    const b = makeKasApproval('subagent-shell', originB, resolveB);
    store.setState({
      pendingApproval: a as any,
      approvalQueue: [a as any, b as any],
    });
    return { store, resolveA, resolveB, b };
  }

  it('does NOT cascade a whole-capability trust to a sibling from a different origin session', () => {
    const { store, resolveB } = setupTwoApprovals(
      'parent-session',
      'subagent-session'
    );

    store.getState().respondToApproval('always-accept', undefined, {
      kasWholeCapability: true,
    });

    // The visible parent approval is trusted; the hidden-subagent sibling — same
    // capability + workspaceRoot but a distinct origin session — must survive.
    expect(resolveB).not.toHaveBeenCalled();
    expect(store.getState().approvalQueue).toHaveLength(1);
    expect(store.getState().pendingApproval?.toolCall.toolCallId).toBe(
      'subagent-shell'
    );
  });

  it('still cascades to a sibling that shares the same origin session', () => {
    const { store, resolveB } = setupTwoApprovals(
      'same-session',
      'same-session'
    );

    store.getState().respondToApproval('always-accept', undefined, {
      kasWholeCapability: true,
    });

    expect(resolveB).toHaveBeenCalledTimes(1);
    expect(store.getState().approvalQueue).toHaveLength(0);
  });
});

describe('--trust-all-tools auto-approval', () => {
  function createTrustAllStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro, trustAllTools: true });
    // Simulate user confirming the trust gate
    store.getState().confirmTrustAllTools();
    return store;
  }

  function makeApprovalWithAlways(
    toolCallId: string,
    resolve?: (r: any) => void,
    options: { toolId?: string; consentContext?: Record<string, unknown> } = {}
  ): AgentStreamEvent {
    return {
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId },
        ...(options.toolId ? { toolId: options.toolId } : {}),
        ...(options.consentContext
          ? { consentContext: options.consentContext }
          : {}),
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow',
            optionId: 'accept',
          },
          {
            kind: ApprovalOptionId.AllowAlways,
            name: 'Always',
            optionId: 'always-accept',
          },
          {
            kind: ApprovalOptionId.RejectOnce,
            name: 'Deny',
            optionId: 'reject',
          },
        ],
        resolve: resolve ?? (() => {}),
      },
    } as AgentStreamEvent;
  }

  it('prefers allow_always when available (V2 parity)', () => {
    const store = createTrustAllStore();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    handler(makeToolCallEvent('tc1', 'shell', 'npm test'));
    handler(makeApprovalWithAlways('tc1', resolve));

    expect(resolve).toHaveBeenCalledTimes(1);
    expect(resolve.mock.calls[0]![0]).toEqual({
      outcome: 'selected',
      optionId: 'always-accept',
    });
    expect(store.getState().pendingApproval).toBeNull();
  });

  it('in KAS mode, auto-approve attaches capability and scope', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({
      kiro: mockKiro,
      trustAllTools: true,
      agentEngine: 'kas',
    });
    store.getState().confirmTrustAllTools();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    handler(makeToolCallEvent('tc-kas', 'shell', 'npm test'));
    handler(
      makeApprovalWithAlways('tc-kas', resolve, {
        toolId: 'shell',
        consentContext: { capability: 'shell', resource: 'npm test' },
      })
    );

    expect(resolve).toHaveBeenCalledTimes(1);
    const call = resolve.mock.calls[0]![0];
    expect(call.optionId).toBe('always-accept');
    expect(call._meta?.kiro?.consent).toEqual({
      capability: 'shell',
      scope: 'session',
      resource: '*',
    });
  });

  it('falls back to allow_once when allow_always not offered', () => {
    const store = createTrustAllStore();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    handler(makeToolCallEvent('tc2', 'shell', 'npm test'));
    handler(makeApprovalEvent('tc2', resolve));

    expect(resolve).toHaveBeenCalledTimes(1);
    expect(resolve.mock.calls[0]![0]).toEqual({
      outcome: 'selected',
      optionId: 'allow_once',
    });
    expect(store.getState().pendingApproval).toBeNull();
  });

  it('falls through to UI if neither allow_always nor allow_once available', () => {
    const store = createTrustAllStore();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    const rejectOnlyEvent = {
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'tc-edge' },
        permissionOptions: [
          {
            kind: ApprovalOptionId.RejectOnce,
            name: 'Deny',
            optionId: 'reject',
          },
        ],
        resolve,
      },
    } as AgentStreamEvent;

    handler(makeToolCallEvent('tc-edge', 'shell', 'danger'));
    handler(rejectOnlyEvent);

    // No suitable option — falls through to show UI
    expect(resolve).not.toHaveBeenCalled();
    expect(store.getState().pendingApproval).not.toBeNull();
  });

  it('does NOT auto-approve when trustAllToolsConfirmed is false', () => {
    const store = createAppStore({ kiro: new Kiro(), trustAllTools: true });
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    handler(makeToolCallEvent('tc3', 'shell', 'rm -rf /'));
    handler(makeApprovalEvent('tc3', resolve));

    expect(resolve).not.toHaveBeenCalled();
    expect(store.getState().pendingApproval).not.toBeNull();
  });

  it('does NOT auto-approve when --trust-all-tools was not requested', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();
    const resolve = mock((_r: any) => {});

    handler(makeToolCallEvent('tc4', 'shell', 'ls'));
    handler(makeApprovalEvent('tc4', resolve));

    expect(resolve).not.toHaveBeenCalled();
    expect(store.getState().pendingApproval).not.toBeNull();
  });
});
