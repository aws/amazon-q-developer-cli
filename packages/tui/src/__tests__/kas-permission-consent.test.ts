/**
 * Tests for KAS granular permission consent wiring:
 * A. handlePermissionRequest extracts consentContext from _meta.kiro.consent
 * B. respondToApproval attaches KAS consent _meta with correct scope
 */
import { describe, it, expect, mock, afterAll } from 'bun:test';
import {
  createAppStore,
  MessageRole,
  ToolUseStatus,
} from '../stores/app-store';
import { AgentEventType, ApprovalOptionId } from '../types/agent-events';
import type {
  AgentStreamEvent,
  ApprovalRequestInfo,
} from '../types/agent-events';
import { Kiro } from '../kiro';

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

// ── A. handlePermissionRequest — consentContext extraction ──

describe('handlePermissionRequest — consentContext extraction', () => {
  function createTestStore() {
    return createAppStore({ kiro: new Kiro() });
  }

  function makeToolCallEvent(id: string): AgentStreamEvent {
    return {
      type: AgentEventType.ToolCall,
      id,
      name: 'fs_write',
      kind: 'edit' as any,
      args: { path: '/tmp/test' },
    } as AgentStreamEvent;
  }

  it('passes consentContext from _meta.kiro.consent to the approval event', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('tc-consent'));

    // Simulate an ApprovalRequest with consentContext (as emitted by handlePermissionRequest)
    const consentContext = {
      capability: 'fs:write',
      resource: '/workspace/src',
      workspaceRoot: '/workspace',
    };
    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'tc-consent' },
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
        consentContext,
        resolve: () => {},
      },
    } as AgentStreamEvent);

    const approval = store.getState().pendingApproval;
    expect(approval).toBeDefined();
    expect(approval!.consentContext).toEqual(consentContext);
    expect(approval!.consentContext!.capability).toBe('fs:write');
    expect(approval!.consentContext!.resource).toBe('/workspace/src');
    expect(approval!.consentContext!.workspaceRoot).toBe('/workspace');
  });

  it('works without consentContext (V2 mode — field absent)', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('tc-v2'));
    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'tc-v2' },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow Once',
            optionId: 'accept',
          },
        ],
        resolve: () => {},
      },
    } as AgentStreamEvent);

    const approval = store.getState().pendingApproval;
    expect(approval).toBeDefined();
    expect(approval!.consentContext).toBeUndefined();
  });

  it('passes trustOptions from _meta.trustOptions alongside consentContext', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeToolCallEvent('tc-both'));

    const trustOptions = [
      {
        label: 'Full command',
        display: 'npm install',
        setting_key: 'allowedCommands',
        patterns: ['npm install'],
      },
    ];
    const consentContext = { capability: 'shell:exec' };

    handler({
      type: AgentEventType.ApprovalRequest,
      value: {
        toolCall: { toolCallId: 'tc-both' },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowAlways,
            name: 'Always',
            optionId: 'always-accept',
          },
        ],
        trustOptions,
        consentContext,
        resolve: () => {},
      },
    } as AgentStreamEvent);

    const approval = store.getState().pendingApproval;
    expect(approval!.trustOptions).toEqual(trustOptions);
    expect(approval!.consentContext).toEqual(consentContext);
  });
});

// ── B. respondToApproval — KAS consent _meta ──

describe('respondToApproval — KAS consent _meta', () => {
  function createTestStore() {
    return createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  }

  function setupPendingApproval(
    store: ReturnType<typeof createTestStore>,
    opts: { consentContext?: ApprovalRequestInfo['consentContext'] } = {}
  ) {
    const resolve = mock((_response: any) => {});
    const approval: ApprovalRequestInfo = {
      toolCall: { toolCallId: 'tc-kas' },
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
      consentContext: opts.consentContext,
      resolve,
    };
    store.setState({
      pendingApproval: approval,
      approvalQueue: [approval],
      messages: [
        {
          id: 'tc-kas',
          role: MessageRole.ToolUse,
          name: 'fs_write',
          content: '{}',
          status: ToolUseStatus.Pending,
        },
      ],
    });
    return { resolve, approval };
  }

  it('allow_always default scope is "session" when consentContext present', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: { capability: 'fs:write', resource: '/workspace' },
    });

    store.getState().respondToApproval('always-accept');

    expect(resolve).toHaveBeenCalledTimes(1);
    const call = resolve.mock.calls[0]![0];
    expect(call.outcome).toBe('selected');
    expect(call.optionId).toBe('always-accept');
    expect(call._meta?.kiro?.consent?.capability).toBe('fs:write');
    expect(call._meta?.kiro?.consent?.scope).toBe('session');
    expect(call._meta?.kiro?.consent?.resource).toBeUndefined();
  });

  it('allow_once scope is "invocation"', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: { capability: 'shell:exec' },
    });

    store.getState().respondToApproval('accept');

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent?.capability).toBe('shell:exec');
    expect(call._meta?.kiro?.consent?.scope).toBe('invocation');
  });

  it('allow_always with explicit kasScope overrides default', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: { capability: 'fs:write' },
    });

    store.getState().respondToApproval('always-accept', undefined, {
      kasScope: 'workspace',
    });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent?.capability).toBe('fs:write');
    expect(call._meta?.kiro?.consent?.scope).toBe('workspace');
    expect(call._meta?.kiro?.consent?.resource).toBeUndefined();
  });

  it('entire tool trust sends the KAS wildcard resource for whole capability persistence', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: {
        capability: 'shell',
        resource: 'echo hello',
        workspaceRoot: '/workspace',
      },
    });

    store.getState().respondToApproval('always-accept', undefined, {
      kasWholeCapability: true,
    });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent?.capability).toBe('shell');
    expect(call._meta?.kiro?.consent?.resource).toBe('*');
    expect(call._meta?.kiro?.consent?.workspaceRoot).toBe('/workspace');
  });

  it('shell:exec entire tool trust sends the KAS wildcard resource', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: {
        capability: 'shell:exec',
        resource: 'echo hello',
      },
    });

    store.getState().respondToApproval('always-accept', undefined, {
      kasWholeCapability: true,
    });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent).toEqual({
      capability: 'shell:exec',
      scope: 'session',
      resource: '*',
    });
  });

  it('fs_write entire tool trust sends the KAS wildcard resource', () => {
    // Regression: the wildcard was gated to shell capabilities, so pressing
    // "trust whole tool" on a write approval sent scope but no resource — KAS
    // then persisted only the narrow path, re-asking every OTHER path. A
    // non-shell capability must send resource:'*' just like shell does.
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: {
        capability: 'fs_write',
        resource: '/workspace/src/a.ts',
        workspaceRoot: '/workspace',
      },
    });

    store.getState().respondToApproval('always-accept', undefined, {
      kasWholeCapability: true,
    });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent?.capability).toBe('fs_write');
    expect(call._meta?.kiro?.consent?.resource).toBe('*');
    expect(call._meta?.kiro?.consent?.workspaceRoot).toBe('/workspace');
  });

  it('bare shell allow_always does not imply whole-capability wildcard trust', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: {
        capability: 'shell',
        resource: 'echo hello',
        triggeringResource: 'hello',
      },
    });

    store.getState().respondToApproval('always-accept');

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent).toEqual({
      capability: 'shell',
      scope: 'session',
    });
  });

  it('explicit kasResource is included in response', () => {
    const store = createTestStore();
    const { resolve } = setupPendingApproval(store, {
      consentContext: {
        capability: 'shell',
        resource: 'git commit -m test',
        workspaceRoot: '/workspace',
      },
    });

    store.getState().respondToApproval('always-accept', undefined, {
      kasScope: 'workspace',
      kasResource: 'git *',
    });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent?.capability).toBe('shell');
    expect(call._meta?.kiro?.consent?.resource).toBe('git *');
    expect(call._meta?.kiro?.consent?.scope).toBe('workspace');
  });

  it('V2 mode (no consentContext): _meta does NOT contain kiro.consent', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { resolve } = setupPendingApproval(store, {
      consentContext: undefined,
    });

    store.getState().respondToApproval('accept');

    const call = resolve.mock.calls[0]![0];
    expect(call._meta?.kiro?.consent).toBeUndefined();
  });

  it('V2 mode with trustOption: _meta contains only trustOption, no kiro.consent', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { resolve } = setupPendingApproval(store, {
      consentContext: undefined,
    });

    const trustOption = {
      label: 'Full command',
      setting_key: 'allowedCommands',
      patterns: ['df -h'],
    };
    store
      .getState()
      .respondToApproval('allow_always', undefined, { trustOption });

    const call = resolve.mock.calls[0]![0];
    expect(call._meta).toEqual({ trustOption });
    expect(call._meta?.kiro).toBeUndefined();
  });
});
