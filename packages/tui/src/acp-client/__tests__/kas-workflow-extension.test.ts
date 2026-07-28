import * as acp from '@agentclientprotocol/sdk';
import { describe, expect, it } from 'bun:test';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type AgentStreamEvent,
  type KiroMeta,
} from '../../types/agent-events.js';
import type { SessionEvent } from '../../types/multi-session.js';
import type {
  WorkflowEvent,
  WorkflowLoadResponse,
  WorkflowNodeSessionTarget,
} from '../../types/workflow.js';
import type { AcpSessionUpdate } from '../base.js';
import { KasWorkflowExtension } from '../kas-extensions/workflow/controller.js';
import type { WorkflowExtensionHost as KasWorkflowExtensionHost } from '../kas-extensions/workflow/ports.js';
import {
  WORKFLOW_LIFECYCLE_CONTRACTS,
  type WorkflowNotificationMethod,
} from '../kas-extensions/workflow/contracts.js';
import type { WorkflowExtensionEffect } from '../kas-extensions/workflow/effects.js';
import type { WorkflowSessionOwner } from '../kas-extensions/workflow/owner-registry.js';
import type {
  Disposable,
  KasExtensionRuntime as KasWorkflowTransport,
  NotificationContract,
  RpcContract,
  SessionLeaseHandlers,
} from '../kas-extensions/runtime.js';

type ContentEvent = Extract<AgentStreamEvent, { type: AgentEventType.Content }>;
type UserMessageEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.UserMessage }
>;
type ApprovalRequestEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.ApprovalRequest }
>;
type ToolCallEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.ToolCall }
>;
type NodeStartEvent = Extract<WorkflowEvent, { type: 'node_start' }>;

const PARENT_SESSION_ID = 'parent-session';
const OTHER_PARENT_SESSION_ID = 'other-parent-session';
const TARGET = {
  workflowId: 'workflow-1',
  parentSessionId: PARENT_SESSION_ID,
  nodeId: 'implement',
  nodePath: ['root', 'implement'],
  sessionId: 'node-session-1',
  iteration: 1,
  branchId: 'branch-a',
} as const satisfies WorkflowNodeSessionTarget;

interface Deferred<T> {
  promise: Promise<T>;
  resolve(value: T | PromiseLike<T>): void;
  reject(reason?: unknown): void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: Deferred<T>['resolve'];
  let reject!: Deferred<T>['reject'];
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function contentEvent(id: string, text = id): ContentEvent {
  return {
    type: AgentEventType.Content,
    id,
    content: { type: ContentType.Text, text },
  };
}

function userMessageEvent(id: string, text = id): UserMessageEvent {
  return {
    type: AgentEventType.UserMessage,
    id,
    content: { type: ContentType.Text, text },
  };
}

function toolCallEvent(id: string): ToolCallEvent {
  return {
    type: AgentEventType.ToolCall,
    id,
    name: 'read_files',
    kind: 'read',
    args: {},
  };
}

function agentUpdate(text: string): AcpSessionUpdate {
  return {
    sessionUpdate: 'agent_message_chunk',
    content: { type: 'text', text },
  };
}

function userUpdate(text: string): AcpSessionUpdate {
  return {
    sessionUpdate: 'user_message_chunk',
    content: { type: 'text', text },
  };
}

function permissionRequest(
  sessionId: string = TARGET.sessionId
): acp.RequestPermissionRequest {
  return {
    sessionId,
    options: [
      {
        kind: 'allow_once',
        name: 'Allow once',
        optionId: 'allow-once',
      },
    ],
    toolCall: {
      toolCallId: 'tool-call-1',
      title: 'Write file',
      kind: 'edit',
      rawInput: { path: 'result.txt' },
    },
  };
}

function nodeStartEvent(
  overrides: Partial<NodeStartEvent> = {}
): NodeStartEvent {
  return {
    type: 'node_start',
    workflowId: TARGET.workflowId,
    parentSessionId: TARGET.parentSessionId,
    nodeId: TARGET.nodeId,
    nodePath: TARGET.nodePath,
    nodeType: 'step',
    agentName: 'builder',
    sessionId: TARGET.sessionId,
    iteration: TARGET.iteration,
    branchId: TARGET.branchId,
    ...overrides,
  };
}

function loadedWorkflow(
  target: WorkflowNodeSessionTarget = TARGET,
  status: 'completed' | 'running' = 'completed'
): WorkflowLoadResponse {
  return {
    workflowId: target.workflowId,
    state: {
      workflowId: target.workflowId,
      workflowName: 'Test workflow',
      status,
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      parentSessionId: target.parentSessionId,
      root: {
        nodeId: 'root',
        type: 'sequence',
        status,
        children: [
          {
            nodeId: target.nodeId,
            type: 'step',
            status,
            agentName: 'builder',
            sessionId: target.sessionId,
            iteration: target.iteration,
            branchId: target.branchId,
          },
        ],
      },
    },
    stepSessions: [
      {
        nodeId: target.nodeId,
        nodePath: target.nodePath,
        sessionId: target.sessionId,
        iteration: target.iteration,
        branchId: target.branchId,
      },
    ],
  };
}

function textFromEvent(event: AgentStreamEvent): string | undefined {
  if (
    (event.type === AgentEventType.Content ||
      event.type === AgentEventType.UserMessage) &&
    event.content.type === ContentType.Text
  ) {
    return event.content.text;
  }
  return undefined;
}

class FakeKasWorkflowTransport implements KasWorkflowTransport {
  readonly requests: Array<{
    method: string;
    params: Readonly<Record<string, unknown>>;
  }> = [];
  readonly replayCalls: string[] = [];
  readonly promptCalls: Array<{ sessionId: string; content: string }> = [];
  readonly leaseCalls: string[] = [];
  readonly invalidNotifications: string[] = [];
  workflowLoadBehavior?: (workflowId: string) => Promise<WorkflowLoadResponse>;
  replayBehavior?: (sessionId: string) => Promise<void>;
  promptBehavior?: (sessionId: string, content: string) => Promise<void>;
  disposeCalls = 0;

  private readonly workflowLoads = new Map<string, WorkflowLoadResponse>();
  private readonly rpcResponses = new Map<string, unknown>();
  private readonly notificationListeners = new Map<
    string,
    Set<(payload: unknown) => void>
  >();
  private readonly updateListeners = new Map<
    string,
    SessionLeaseHandlers['onUpdate']
  >();
  private readonly permissionListeners = new Map<
    string,
    SessionLeaseHandlers['onPermission']
  >();

  setWorkflowLoad(response: WorkflowLoadResponse): void {
    this.workflowLoads.set(response.workflowId, response);
  }

  setRpcResponse(method: string, response: unknown): void {
    this.rpcResponses.set(method, response);
  }

  async request<Params, Response>(
    contract: RpcContract<Params, Response>,
    params: Params
  ): Promise<Response> {
    const encoded = contract.encode(params);
    this.requests.push({ method: contract.method, params: encoded });
    if (this.rpcResponses.has(contract.method)) {
      const response = contract.decode(this.rpcResponses.get(contract.method));
      if (response === null) {
        throw new Error(`Invalid fake response for ${contract.method}`);
      }
      return response;
    }
    const workflowId = encoded.workflowId;
    if (typeof workflowId !== 'string') {
      throw new Error(`Missing workflowId for ${contract.method}`);
    }
    const rawResponse = this.workflowLoadBehavior
      ? await this.workflowLoadBehavior(workflowId)
      : this.workflowLoads.get(workflowId);
    if (!rawResponse) {
      throw new Error(`No workflow response for ${workflowId}`);
    }
    const response = contract.decode(rawResponse);
    if (response === null) {
      throw new Error(`Invalid fake response for ${contract.method}`);
    }
    return response;
  }

  subscribe<Payload>(
    contract: NotificationContract<Payload>,
    handler: (payload: Payload) => void
  ): Disposable {
    const listener = (payload: unknown) => {
      const decoded = contract.decode(payload);
      if (decoded === null) {
        this.invalidNotifications.push(contract.method);
        return;
      }
      handler(decoded);
    };
    const listeners =
      this.notificationListeners.get(contract.method) ??
      new Set<(payload: unknown) => void>();
    listeners.add(listener);
    this.notificationListeners.set(contract.method, listeners);

    let active = true;
    return {
      dispose: () => {
        if (!active) return;
        active = false;
        listeners.delete(listener);
        if (listeners.size === 0) {
          this.notificationListeners.delete(contract.method);
        }
      },
    };
  }

  leaseSession(sessionId: string, handlers: SessionLeaseHandlers): Disposable {
    if (
      this.updateListeners.has(sessionId) ||
      this.permissionListeners.has(sessionId)
    ) {
      throw new Error(`Duplicate lease for ${sessionId}`);
    }
    this.leaseCalls.push(sessionId);
    this.updateListeners.set(sessionId, handlers.onUpdate);
    this.permissionListeners.set(sessionId, handlers.onPermission);

    let active = true;
    return {
      dispose: () => {
        if (!active) return;
        active = false;
        this.updateListeners.delete(sessionId);
        this.permissionListeners.delete(sessionId);
      },
    };
  }

  async replaySession(sessionId: string): Promise<void> {
    this.replayCalls.push(sessionId);
    await this.replayBehavior?.(sessionId);
  }

  async promptSession(sessionId: string, content: string): Promise<void> {
    this.promptCalls.push({ sessionId, content });
    await this.promptBehavior?.(sessionId, content);
  }

  dispose(): void {
    this.disposeCalls += 1;
    this.notificationListeners.clear();
    this.updateListeners.clear();
    this.permissionListeners.clear();
  }

  emitNotification(method: WorkflowNotificationMethod, payload: unknown): void {
    for (const listener of this.notificationListeners.get(method) ?? []) {
      listener(payload);
    }
  }

  async emitUpdate(
    sessionId: string,
    update: AcpSessionUpdate
  ): Promise<boolean> {
    const listener = this.updateListeners.get(sessionId);
    if (!listener) return false;
    await listener({ sessionId, update });
    return true;
  }

  async emitPermission(
    request: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse | undefined> {
    return this.permissionListeners.get(request.sessionId)?.(request);
  }

  hasUpdateListener(sessionId: string): boolean {
    return this.updateListeners.has(sessionId);
  }

  hasPermissionListener(sessionId: string): boolean {
    return this.permissionListeners.has(sessionId);
  }

  get activeNotificationCount(): number {
    let count = 0;
    for (const listeners of this.notificationListeners.values()) {
      count += listeners.size;
    }
    return count;
  }

  get activeNotificationMethods(): string[] {
    return [...this.notificationListeners.keys()].sort();
  }
}

class FakeKasWorkflowExtensionHost implements KasWorkflowExtensionHost {
  readonly effects: WorkflowExtensionEffect[] = [];
  readonly mainEvents: AgentStreamEvent[] = [];
  readonly childEvents: Array<{
    sessionId: string;
    event: AgentStreamEvent;
  }> = [];
  readonly approvalEvents: ApprovalRequestEvent[] = [];
  readonly sessionEvents: SessionEvent[] = [];
  readonly steerCalls: Array<{ sessionId: string; content: string }> = [];
  readonly permissionCalls: Array<{
    request: acp.RequestPermissionRequest;
    sessionId: string;
  }> = [];
  readonly clearedSessions: string[] = [];
  readonly timeline: string[] = [];
  autoResolvePermissions = true;

  private readonly conversations = new Map<string, AgentStreamEvent[]>();
  private nextEventId = 0;

  convertUpdate(
    update: AcpSessionUpdate,
    _sessionId: string,
    _sideEffectSink: (event: AgentStreamEvent) => void
  ): AgentStreamEvent | null {
    if (
      (update.sessionUpdate !== 'agent_message_chunk' &&
        update.sessionUpdate !== 'user_message_chunk') ||
      update.content.type !== 'text'
    ) {
      return null;
    }
    const id = `converted-${++this.nextEventId}`;
    return update.sessionUpdate === 'agent_message_chunk'
      ? contentEvent(id, update.content.text)
      : userMessageEvent(id, update.content.text);
  }

  routePermissionRequest(
    request: acp.RequestPermissionRequest,
    sessionId: string,
    eventSink: (event: ApprovalRequestEvent) => void
  ): Promise<acp.RequestPermissionResponse> {
    this.permissionCalls.push({ request, sessionId });
    let resolveRequest!: (response: acp.RequestPermissionResponse) => void;
    const response = new Promise<acp.RequestPermissionResponse>((resolve) => {
      resolveRequest = resolve;
    });
    const event: ApprovalRequestEvent = {
      type: AgentEventType.ApprovalRequest,
      value: {
        originSessionId: sessionId,
        sessionId,
        toolCall: {
          toolCallId: request.toolCall.toolCallId,
          title: request.toolCall.title ?? undefined,
          rawInput: request.toolCall.rawInput,
        },
        permissionOptions: [
          {
            kind: ApprovalOptionId.AllowOnce,
            name: 'Allow once',
            optionId: 'allow-once',
          },
        ],
        resolve: (userResponse) => {
          resolveRequest(
            userResponse.outcome === 'selected'
              ? {
                  outcome: {
                    outcome: 'selected',
                    optionId: userResponse.optionId,
                  },
                }
              : { outcome: { outcome: 'cancelled' } }
          );
        },
      },
    };
    eventSink(event);
    if (this.autoResolvePermissions) {
      this.approvalEvents.at(-1)?.value.resolve({ outcome: 'cancelled' });
    }
    return response;
  }

  async steerSession(sessionId: string, content: string): Promise<void> {
    this.steerCalls.push({ sessionId, content });
  }

  emitEffect(effect: WorkflowExtensionEffect): void {
    this.effects.push(effect);
    switch (effect.type) {
      case 'workflow_progress':
        this.emitMain({
          type: AgentEventType.WorkflowProgress,
          id: `workflow-event-${++this.nextEventId}`,
          event: effect.event,
        });
        break;
      case 'child_registered': {
        const now = new Date();
        const name =
          effect.owner.agentName ??
          effect.owner.nodeId ??
          effect.owner.sessionId.slice(0, 8);
        this.emitSession({
          type: 'session_created',
          session: {
            id: effect.owner.sessionId,
            name,
            agentName: effect.owner.agentName ?? name,
            status: this.workflowSessionStatus(effect.owner),
            type: 'ephemeral',
            group: 'workflow',
            parentSession: effect.owner.parentSessionId,
            created: now,
            lastActivity: now,
          },
        });
        break;
      }
      case 'child_removed':
        this.emitSession({
          type: 'session_removed',
          sessionId: effect.sessionId,
        });
        break;
      case 'child_status_restored':
        this.emitSession({
          type: 'session_status_changed',
          sessionId: effect.owner.sessionId,
          status: this.workflowSessionStatus(effect.owner),
        });
        break;
      case 'child_busy':
        this.emitSession({
          type: 'session_status_changed',
          sessionId: effect.sessionId,
          status: 'busy',
        });
        break;
      case 'child_conversation_reset':
        this.emitSession({
          type: 'session_conversation_reset',
          sessionId: effect.sessionId,
        });
        break;
      case 'child_turn_started':
        this.emitSession({
          type: 'session_turn_started',
          sessionId: effect.sessionId,
        });
        break;
      case 'child_event':
        this.emitChild(effect.sessionId, effect.event);
        break;
      case 'child_approval_requested':
        this.approvalEvents.push(effect.event);
        this.emitChild(effect.sessionId, effect.event);
        break;
      case 'child_approvals_cancelled':
        this.emitSession({
          type: 'session_approvals_cancelled',
          sessionId: effect.sessionId,
        });
        break;
    }
  }

  clearSessionState(sessionId: string): void {
    this.clearedSessions.push(sessionId);
  }

  seedConversation(sessionId: string, events: AgentStreamEvent[]): void {
    this.conversations.set(sessionId, [...events]);
  }

  conversationTexts(sessionId: string): Array<string | undefined> {
    return (this.conversations.get(sessionId) ?? []).map(textFromEvent);
  }

  resetCount(sessionId: string): number {
    return this.sessionEvents.filter(
      (event) =>
        event.type === 'session_conversation_reset' &&
        event.sessionId === sessionId
    ).length;
  }

  private workflowSessionStatus(
    owner: WorkflowSessionOwner
  ): 'busy' | 'pending' | 'failed' | 'idle' {
    if (owner.status === 'running') return 'busy';
    if (owner.status === 'pending') return 'pending';
    if (owner.status === 'failed' || owner.status === 'aborted')
      return 'failed';
    return 'idle';
  }

  private emitMain(event: AgentStreamEvent): void {
    this.mainEvents.push(event);
    this.timeline.push(`main:${event.type}`);
  }

  private emitChild(sessionId: string, event: AgentStreamEvent): void {
    this.childEvents.push({ sessionId, event });
    const conversation = this.conversations.get(sessionId) ?? [];
    conversation.push(event);
    this.conversations.set(sessionId, conversation);
    this.timeline.push(
      `child:${sessionId}:${textFromEvent(event) ?? event.type}`
    );
  }

  private emitSession(event: SessionEvent): void {
    this.sessionEvents.push(event);
    if (event.type === 'session_conversation_reset') {
      this.conversations.set(event.sessionId, []);
      this.timeline.push(`reset:${event.sessionId}`);
    }
  }
}

interface Fixture {
  transport: FakeKasWorkflowTransport;
  host: FakeKasWorkflowExtensionHost;
  extension: KasWorkflowExtension;
}

function createFixture(): Fixture {
  const transport = new FakeKasWorkflowTransport();
  const host = new FakeKasWorkflowExtensionHost();
  const extension = new KasWorkflowExtension(transport, host);
  extension.setParentSession(PARENT_SESSION_ID);
  extension.start();
  return { transport, host, extension };
}

function emitNodeStart(
  transport: FakeKasWorkflowTransport,
  event = nodeStartEvent()
): void {
  const { type: _eventType, nodeType, ...payload } = event;
  transport.emitNotification('_kiro/workflow/node_start', {
    ...payload,
    ...(nodeType ? { type: nodeType } : {}),
  });
}

describe('KasWorkflowExtension', () => {
  it('routes workflow history and controls through typed RPC contracts', async () => {
    const { transport, extension } = createFixture();
    const state = loadedWorkflow().state;
    const summary = {
      workflowId: TARGET.workflowId,
      name: state.workflowName,
      status: state.status,
      createdAt: '2026-07-19T10:00:00.000Z',
      updatedAt: '2026-07-19T10:01:00.000Z',
      parentSessionId: TARGET.parentSessionId,
    } as const;
    transport.setRpcResponse('_kiro/workflow/list', { runs: [summary] });
    transport.setRpcResponse('_kiro/workflow/inspect', {
      workflowId: TARGET.workflowId,
      state,
    });
    transport.setRpcResponse('_kiro/workflow/pause', { paused: true });
    transport.setRpcResponse('_kiro/workflow/resume', {
      workflowId: TARGET.workflowId,
      status: 'running',
    });
    transport.setRpcResponse('_kiro/workflow/cancel', {
      ok: true,
      previousStatus: 'running',
    });

    await expect(extension.listRuns(['/workspace'])).resolves.toEqual([
      summary,
    ]);
    await expect(extension.inspectRun(TARGET.workflowId)).resolves.toEqual({
      workflowId: TARGET.workflowId,
      state,
    });
    await expect(extension.pauseRun(TARGET.workflowId)).resolves.toEqual({
      paused: true,
    });
    await expect(extension.resumeRun(TARGET.workflowId)).resolves.toEqual({
      workflowId: TARGET.workflowId,
      status: 'running',
    });
    await expect(
      extension.cancelRun(TARGET.workflowId, 'completed')
    ).resolves.toEqual({
      ok: true,
      previousStatus: 'running',
    });
    expect(transport.requests).toEqual([
      {
        method: '_kiro/workflow/list',
        params: { workspacePaths: ['/workspace'] },
      },
      {
        method: '_kiro/workflow/inspect',
        params: { workflowId: TARGET.workflowId },
      },
      {
        method: '_kiro/workflow/pause',
        params: { workflowId: TARGET.workflowId },
      },
      {
        method: '_kiro/workflow/resume',
        params: { workflowId: TARGET.workflowId },
      },
      {
        method: '_kiro/workflow/cancel',
        params: {
          workflowId: TARGET.workflowId,
          targetStatus: 'completed',
        },
      },
    ]);
  });

  it('registers lifecycle nodes with dedicated child listeners and session events', () => {
    const { transport, host } = createFixture();

    expect(transport.activeNotificationMethods).toEqual(
      WORKFLOW_LIFECYCLE_CONTRACTS.map((contract) => contract.method).sort()
    );
    emitNodeStart(transport);

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);
    expect(host.sessionEvents).toHaveLength(2);
    expect(host.sessionEvents[0]).toMatchObject({
      type: 'session_created',
      session: {
        id: TARGET.sessionId,
        agentName: 'builder',
        group: 'workflow',
        parentSession: PARENT_SESSION_ID,
        status: 'busy',
        type: 'ephemeral',
      },
    });
    expect(host.sessionEvents[1]).toEqual({
      type: 'session_status_changed',
      sessionId: TARGET.sessionId,
      status: 'busy',
    });
    expect(host.mainEvents).toHaveLength(1);
    expect(host.mainEvents[0]).toMatchObject({
      type: AgentEventType.WorkflowProgress,
      event: nodeStartEvent(),
    });
  });

  it('treats duplicate node_start events as one child registration and lease', () => {
    const { transport, host } = createFixture();

    emitNodeStart(transport);
    emitNodeStart(transport);

    expect(
      host.effects.filter((effect) => effect.type === 'child_registered')
    ).toHaveLength(1);
    expect(
      host.sessionEvents.filter((event) => event.type === 'session_created')
    ).toHaveLength(1);
    expect(transport.leaseCalls).toEqual([TARGET.sessionId]);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);
  });

  it('routes child content and approvals through their typed sinks', async () => {
    const { transport, host } = createFixture();
    emitNodeStart(transport);
    const mainEventCount = host.mainEvents.length;

    expect(
      await transport.emitUpdate(
        TARGET.sessionId,
        agentUpdate('child response')
      )
    ).toBe(true);
    const permissionResponse =
      await transport.emitPermission(permissionRequest());

    expect(permissionResponse).toEqual({
      outcome: { outcome: 'cancelled' },
    });
    expect(host.mainEvents).toHaveLength(mainEventCount);
    expect(host.childEvents.map(({ sessionId }) => sessionId)).toEqual([
      TARGET.sessionId,
      TARGET.sessionId,
    ]);
    expect(host.childEvents.map(({ event }) => event.type)).toEqual([
      AgentEventType.Content,
      AgentEventType.ApprovalRequest,
    ]);
    expect(host.permissionCalls).toHaveLength(1);
    expect(host.approvalEvents).toHaveLength(1);
  });

  it('cancels pending child approvals before releasing session ownership', async () => {
    const { transport, host, extension } = createFixture();
    emitNodeStart(transport);
    host.autoResolvePermissions = false;

    const pendingResponse = transport.emitPermission(permissionRequest());
    await Promise.resolve();

    expect(host.approvalEvents).toHaveLength(1);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);

    extension.setParentSession(OTHER_PARENT_SESSION_ID);

    await expect(pendingResponse).resolves.toEqual({
      outcome: { outcome: 'cancelled' },
    });
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    expect(
      host.sessionEvents
        .filter(
          (event) =>
            event.type === 'session_approvals_cancelled' ||
            event.type === 'session_removed'
        )
        .map((event) => event.type)
    ).toEqual(['session_approvals_cancelled', 'session_removed']);
  });

  it('re-keys approval routing when a resumed child gets a new session', async () => {
    const { transport, host } = createFixture();
    emitNodeStart(transport);
    host.autoResolvePermissions = false;

    const oldApproval = transport.emitPermission(permissionRequest());
    await Promise.resolve();
    const replacementSessionId = 'replacement-node-session';
    emitNodeStart(
      transport,
      nodeStartEvent({ sessionId: replacementSessionId })
    );

    await expect(oldApproval).resolves.toEqual({
      outcome: { outcome: 'cancelled' },
    });
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(replacementSessionId)).toBe(true);
    expect(host.clearedSessions).toContain(TARGET.sessionId);

    const replacementApproval = transport.emitPermission(
      permissionRequest(replacementSessionId)
    );
    await Promise.resolve();
    expect(host.permissionCalls.at(-1)?.sessionId).toBe(replacementSessionId);
    host.approvalEvents.at(-1)?.value.resolve({
      outcome: 'selected',
      optionId: 'allow-once',
    });
    await expect(replacementApproval).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'allow-once' },
    });
  });

  it('suppresses workflow relays while preserving ordinary parent content', () => {
    const { extension } = createFixture();
    const workflowContent = contentEvent('workflow-relay');
    const ordinaryContent = contentEvent('ordinary-parent-content');
    const workflowMeta: KiroMeta = {
      workflow: {
        workflowId: TARGET.workflowId,
        nodeId: TARGET.nodeId,
        nodePath: TARGET.nodePath,
        iteration: TARGET.iteration,
        branchId: TARGET.branchId,
      },
    };

    expect(
      extension.interceptParentEvent(
        workflowContent,
        workflowMeta,
        PARENT_SESSION_ID
      )
    ).toBeNull();
    expect(
      extension.interceptParentEvent(
        ordinaryContent,
        undefined,
        PARENT_SESSION_ID
      )
    ).toBe(ordinaryContent);
  });

  it('suppresses a metadata-poor synthesized ToolCall after its tagged parent start', () => {
    const { extension } = createFixture();
    const toolCallId = 'workflow-tool-call';
    const workflowMeta: KiroMeta = {
      notification: {
        kind: 'workflow-progress',
        workflowId: TARGET.workflowId,
      },
    };

    expect(
      extension.shouldSuppressParentRelay(
        toolCallEvent(toolCallId),
        workflowMeta
      )
    ).toBe(true);
    expect(
      extension.shouldSuppressParentRelay(toolCallEvent(toolCallId), undefined)
    ).toBe(true);
  });

  it('steers running nodes instead of starting a prompt', async () => {
    const { transport, host, extension } = createFixture();
    emitNodeStart(transport);

    await expect(
      extension.sendMessage(TARGET, '  revise the implementation  ')
    ).resolves.toBeUndefined();

    expect(host.steerCalls).toEqual([
      {
        sessionId: TARGET.sessionId,
        content: 'revise the implementation',
      },
    ]);
    expect(transport.replayCalls).toEqual([]);
    expect(transport.promptCalls).toEqual([]);
  });

  it('atomically replays a completed node and exposes later prompt echoes', async () => {
    const { transport, host, extension } = createFixture();
    transport.setWorkflowLoad(loadedWorkflow());
    host.seedConversation(TARGET.sessionId, [
      contentEvent('existing', 'existing conversation'),
    ]);
    const replayStarted = deferred<void>();
    const releaseReplay = deferred<void>();
    transport.replayBehavior = async (sessionId) => {
      await transport.emitUpdate(
        sessionId,
        userUpdate('generated node prompt')
      );
      await transport.emitUpdate(sessionId, agentUpdate('persisted response'));
      replayStarted.resolve();
      await releaseReplay.promise;
    };
    transport.promptBehavior = async (sessionId, _content) => {
      await transport.emitUpdate(sessionId, agentUpdate('follow-up response'));
    };

    const message = extension.sendMessage(TARGET, 'normal follow-up');
    await replayStarted.promise;

    expect(host.resetCount(TARGET.sessionId)).toBe(0);
    expect(host.childEvents).toEqual([]);
    expect(host.conversationTexts(TARGET.sessionId)).toEqual([
      'existing conversation',
    ]);

    releaseReplay.resolve();
    await expect(message).resolves.toBeUndefined();

    expect(host.resetCount(TARGET.sessionId)).toBe(1);
    expect(transport.replayCalls).toEqual([TARGET.sessionId]);
    expect(transport.promptCalls).toEqual([
      { sessionId: TARGET.sessionId, content: 'normal follow-up' },
    ]);
    expect(host.conversationTexts(TARGET.sessionId)).toEqual([
      'persisted response',
      'follow-up response',
    ]);
  });

  it('opens one prompt and steers a concurrent completed-node message', async () => {
    const { transport, host, extension } = createFixture();
    transport.setWorkflowLoad(loadedWorkflow());
    const replayStarted = deferred<void>();
    const releaseReplay = deferred<void>();
    const promptStarted = deferred<void>();
    const releasePrompt = deferred<void>();
    transport.replayBehavior = async () => {
      replayStarted.resolve();
      await releaseReplay.promise;
    };
    transport.promptBehavior = async () => {
      promptStarted.resolve();
      await releasePrompt.promise;
    };

    const first = extension.sendMessage(TARGET, 'first follow-up');
    await replayStarted.promise;
    const second = extension.sendMessage(TARGET, 'second follow-up');
    releaseReplay.resolve();
    await promptStarted.promise;

    await expect(second).resolves.toBeUndefined();
    expect(transport.promptCalls).toEqual([
      { sessionId: TARGET.sessionId, content: 'first follow-up' },
    ]);
    expect(host.steerCalls).toEqual([
      { sessionId: TARGET.sessionId, content: 'second follow-up' },
    ]);

    releasePrompt.resolve();
    await expect(first).resolves.toBeUndefined();
  });

  it('keeps the child lease until an active completed-node prompt settles', async () => {
    const { transport, host, extension } = createFixture();
    transport.setWorkflowLoad(loadedWorkflow());
    const promptStarted = deferred<void>();
    const releasePrompt = deferred<void>();
    transport.promptBehavior = async () => {
      promptStarted.resolve();
      await releasePrompt.promise;
    };

    const message = extension.sendMessage(TARGET, 'follow-up');
    await promptStarted.promise;
    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'completed',
      finalState: loadedWorkflow().state,
    });

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);
    expect(host.clearedSessions).not.toContain(TARGET.sessionId);

    releasePrompt.resolve();
    await message;

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    expect(host.clearedSessions).toContain(TARGET.sessionId);
  });

  it('preserves the existing conversation when transcript replay fails', async () => {
    const { transport, host, extension } = createFixture();
    transport.setWorkflowLoad(loadedWorkflow());
    host.seedConversation(TARGET.sessionId, [
      contentEvent('existing', 'existing conversation'),
    ]);
    transport.replayBehavior = async (sessionId) => {
      await transport.emitUpdate(sessionId, agentUpdate('partial replay'));
      throw new Error('replay failed');
    };

    await expect(extension.sendMessage(TARGET, 'follow-up')).rejects.toThrow(
      'replay failed'
    );

    expect(host.resetCount(TARGET.sessionId)).toBe(0);
    expect(host.childEvents).toEqual([]);
    expect(host.conversationTexts(TARGET.sessionId)).toEqual([
      'existing conversation',
    ]);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
  });

  it('rejects cross-parent and unknown workflow node ownership', async () => {
    const { transport, extension } = createFixture();

    await expect(
      extension.sendMessage(
        {
          ...TARGET,
          parentSessionId: OTHER_PARENT_SESSION_ID,
        },
        'follow-up'
      )
    ).rejects.toThrow('Workflow node belongs to a different chat session');
    expect(transport.requests).toEqual([]);

    transport.setWorkflowLoad(
      loadedWorkflow({
        ...TARGET,
        nodeId: 'different-node',
        nodePath: ['root', 'different-node'],
        sessionId: 'different-session',
      })
    );
    await expect(extension.sendMessage(TARGET, 'follow-up')).rejects.toThrow(
      'Session is not owned by the requested workflow node'
    );
    expect(transport.requests).toHaveLength(1);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
  });

  it('does not steer an old child when the parent changes during workflow/load', async () => {
    const { transport, host, extension } = createFixture();
    const releaseLoad = deferred<WorkflowLoadResponse>();
    transport.workflowLoadBehavior = async () => releaseLoad.promise;

    const message = extension.sendMessage(TARGET, 'stale direction');

    expect(transport.requests).toHaveLength(1);
    extension.setParentSession(OTHER_PARENT_SESSION_ID);
    releaseLoad.resolve(loadedWorkflow(TARGET, 'running'));

    await expect(message).rejects.toThrow(
      'Session is not owned by the requested workflow node'
    );
    expect(host.steerCalls).toEqual([]);
    expect(transport.leaseCalls).toEqual([]);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
  });

  it('rejects an in-flight replay after a parent switch without stale output', async () => {
    const { transport, host, extension } = createFixture();
    transport.setWorkflowLoad(loadedWorkflow());
    const replayStarted = deferred<void>();
    const releaseReplay = deferred<void>();
    transport.replayBehavior = async (sessionId) => {
      await transport.emitUpdate(sessionId, agentUpdate('stale replay output'));
      replayStarted.resolve();
      await releaseReplay.promise;
    };

    const message = extension.sendMessage(TARGET, 'follow-up');
    await replayStarted.promise;
    extension.setParentSession(OTHER_PARENT_SESSION_ID);

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    releaseReplay.resolve();
    await expect(message).rejects.toThrow(/ownership changed during replay/i);

    expect(host.resetCount(TARGET.sessionId)).toBe(0);
    expect(host.childEvents).toEqual([]);
  });

  it('disposes child listeners when the workflow reaches a terminal lifecycle state', () => {
    const { transport, host } = createFixture();
    emitNodeStart(transport);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);

    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'completed',
      finalState: loadedWorkflow().state,
    });

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    expect(host.clearedSessions).toContain(TARGET.sessionId);
    expect(host.sessionEvents.at(-1)).toEqual({
      type: 'session_status_changed',
      sessionId: TARGET.sessionId,
      status: 'idle',
    });
  });

  it('ignores conflicting terminal envelopes without releasing child listeners', () => {
    const { transport, host } = createFixture();
    emitNodeStart(transport);
    const finalState = loadedWorkflow().state;

    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'completed',
      finalState: {
        ...finalState,
        parentSessionId: OTHER_PARENT_SESSION_ID,
      },
    });
    transport.emitNotification('_kiro/workflow/run_failed', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      finalState: { status: 'failed' },
    });
    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'running',
      finalState: loadedWorkflow(TARGET, 'running').state,
    });
    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'completed',
      finalState: {
        ...finalState,
        root: {
          nodeId: 'root',
          type: 'sequence',
          status: 'completed',
          children: [
            {
              nodeId: 'different-node',
              type: 'step',
              status: 'completed',
              sessionId: TARGET.sessionId,
            },
          ],
        },
      },
    });

    expect(transport.invalidNotifications).toEqual([
      '_kiro/workflow/run_complete',
      '_kiro/workflow/run_failed',
      '_kiro/workflow/run_complete',
    ]);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);
    expect(host.clearedSessions).not.toContain(TARGET.sessionId);
    expect(host.sessionEvents.at(-1)).toEqual({
      type: 'session_status_changed',
      sessionId: TARGET.sessionId,
      status: 'busy',
    });
  });

  it('retains child listeners for a coherent paused completion', () => {
    const { transport, host } = createFixture();
    emitNodeStart(transport);
    const finalState = loadedWorkflow().state;
    transport.emitNotification('_kiro/workflow/run_complete', {
      workflowId: TARGET.workflowId,
      parentSessionId: TARGET.parentSessionId,
      status: 'paused',
      finalState: {
        ...finalState,
        status: 'paused',
        root: {
          ...finalState.root,
          status: 'paused',
          children: finalState.root.children?.map((node) => ({
            ...node,
            status: 'paused' as const,
          })),
        },
      },
    });

    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(true);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(true);
    expect(host.clearedSessions).not.toContain(TARGET.sessionId);
  });

  it('dispose removes lifecycle and child session listeners', () => {
    const { transport, host, extension } = createFixture();
    emitNodeStart(transport);
    const mainEventCount = host.mainEvents.length;
    const sessionEventCount = host.sessionEvents.length;

    extension.dispose();

    expect(transport.activeNotificationCount).toBe(0);
    expect(transport.hasUpdateListener(TARGET.sessionId)).toBe(false);
    expect(transport.hasPermissionListener(TARGET.sessionId)).toBe(false);
    expect(host.clearedSessions).toContain(TARGET.sessionId);

    emitNodeStart(
      transport,
      nodeStartEvent({
        workflowId: 'late-workflow',
        sessionId: 'late-session',
      })
    );
    expect(host.mainEvents).toHaveLength(mainEventCount);
    expect(host.sessionEvents).toHaveLength(sessionEventCount);
  });
});
