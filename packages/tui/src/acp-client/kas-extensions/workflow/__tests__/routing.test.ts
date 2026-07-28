import { describe, expect, it } from 'bun:test';
import {
  AgentEventType,
  ContentType,
  type AgentStreamEvent,
  type KiroMeta,
} from '../../../../types/agent-events.js';
import type {
  WorkflowEvent,
  WorkflowLoadResponse,
  WorkflowNodeSessionTarget,
} from '../../../../types/workflow.js';
import {
  WorkflowChildMessageState,
  WorkflowParentRelayFilter,
} from '../event-routing.js';
import {
  WorkflowOwnerRegistry,
  type WorkflowRouteResult,
  type WorkflowSessionOwner,
} from '../owner-registry.js';

type NodeStartEvent = Extract<WorkflowEvent, { type: 'node_start' }>;
type UserMessageEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.UserMessage }
>;
type ContentEvent = Extract<AgentStreamEvent, { type: AgentEventType.Content }>;
type ToolCallEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.ToolCall }
>;
type ToolCallUpdateEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.ToolCallUpdate }
>;
type ToolCallFinishedEvent = Extract<
  AgentStreamEvent,
  { type: AgentEventType.ToolCallFinished }
>;

const PARENT_SESSION_ID = 'parent-session';
const OTHER_PARENT_SESSION_ID = 'other-parent-session';

const NODE_TARGET = {
  workflowId: 'workflow-1',
  parentSessionId: PARENT_SESSION_ID,
  nodeId: 'implement',
  nodePath: ['root', 'loop', 'implement'],
  sessionId: 'node-session-1',
  iteration: 2,
  branchId: 'branch-a',
} as const satisfies WorkflowNodeSessionTarget;

function nodeStart(overrides: Partial<NodeStartEvent> = {}): NodeStartEvent {
  const event = {
    type: 'node_start' as const,
    workflowId: NODE_TARGET.workflowId,
    parentSessionId: NODE_TARGET.parentSessionId,
    nodeId: NODE_TARGET.nodeId,
    nodePath: NODE_TARGET.nodePath,
    sessionId: NODE_TARGET.sessionId,
    iteration: NODE_TARGET.iteration,
    branchId: NODE_TARGET.branchId,
    agentName: 'builder',
    ...overrides,
  };
  return {
    ...event,
    nodePath: event.nodePath ?? NODE_TARGET.nodePath,
    nodeType: event.nodeType ?? 'step',
  };
}

function runStart(
  workflowId: string,
  parentSessionId?: string
): Extract<WorkflowEvent, { type: 'run_start' }> {
  return {
    type: 'run_start',
    workflowId,
    parentSessionId,
    workflowName: workflowId,
    inputs: {},
    nodeTree: [],
  };
}

function registerNode(
  router: WorkflowOwnerRegistry,
  overrides: Partial<NodeStartEvent> = {}
): WorkflowSessionOwner {
  const result = router.routeLifecycle(nodeStart(overrides), PARENT_SESSION_ID);
  expect(result.accepted).toBe(true);
  expect(result.changedOwners).toHaveLength(1);
  return result.changedOwners[0]!;
}

function ownerTarget(owner: WorkflowSessionOwner): WorkflowNodeSessionTarget {
  return {
    workflowId: owner.workflowId,
    parentSessionId: owner.parentSessionId,
    nodeId: owner.nodeId,
    nodePath: owner.nodePath,
    sessionId: owner.sessionId,
    iteration: owner.iteration,
    branchId: owner.branchId,
  };
}

function userMessage(id: string): UserMessageEvent {
  return {
    type: AgentEventType.UserMessage,
    id,
    content: { type: ContentType.Text, text: id },
  };
}

function assistantContent(id: string): ContentEvent {
  return {
    type: AgentEventType.Content,
    id,
    content: { type: ContentType.Text, text: id },
  };
}

function toolCall(id: string): ToolCallEvent {
  return {
    type: AgentEventType.ToolCall,
    id,
    name: 'read_files',
    kind: 'read',
    args: {},
  };
}

function toolCallUpdate(id: string): ToolCallUpdateEvent {
  return {
    type: AgentEventType.ToolCallUpdate,
    id,
    content: { type: ContentType.Text, text: 'working' },
  };
}

function toolCallFinished(id: string): ToolCallFinishedEvent {
  return {
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'success', output: 'done' },
  };
}

function loadedRun(): WorkflowLoadResponse {
  return {
    workflowId: 'loaded-workflow',
    state: {
      workflowId: 'loaded-workflow',
      workflowName: 'Loaded workflow',
      status: 'completed',
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      parentSessionId: PARENT_SESSION_ID,
      root: {
        nodeId: 'root',
        type: 'sequence',
        status: 'completed',
        children: [
          {
            nodeId: 'review',
            type: 'step',
            status: 'completed',
            sessionId: 'loaded-node-session',
            iteration: 3,
            branchId: 'review-branch',
            agentName: 'reviewer',
          },
        ],
      },
    },
    stepSessions: [
      {
        nodeId: 'review',
        nodePath: ['root', 'review'],
        sessionId: 'loaded-node-session',
        iteration: 3,
        branchId: 'review-branch',
      },
    ],
  };
}

describe('workflow ownership and event routing', () => {
  it('fails closed when lifecycle parent ownership is missing', () => {
    const router = new WorkflowOwnerRegistry();

    const missingReportedParent = router.routeLifecycle(
      runStart('missing-parent'),
      PARENT_SESSION_ID
    );
    const missingActiveParent = router.routeLifecycle(
      runStart('missing-active-parent', PARENT_SESSION_ID),
      undefined
    );
    const stillUnowned = router.routeLifecycle(
      {
        type: 'node_start',
        workflowId: 'missing-parent',
        nodeId: 'step',
        nodePath: ['missing-parent', 'step'],
        nodeType: 'step',
        sessionId: 'unowned-node-session',
      },
      PARENT_SESSION_ID
    );

    for (const result of [
      missingReportedParent,
      missingActiveParent,
      stillUnowned,
    ]) {
      expect(result).toMatchObject({
        accepted: false,
        changedOwners: [],
        reason: 'missing parent session ownership',
      });
    }
  });

  it('rejects direct and previously claimed cross-parent lifecycle events', () => {
    const router = new WorkflowOwnerRegistry();

    expect(
      router.routeLifecycle(
        runStart('foreign-workflow', OTHER_PARENT_SESSION_ID),
        PARENT_SESSION_ID
      )
    ).toMatchObject({
      accepted: false,
      parentSessionId: OTHER_PARENT_SESSION_ID,
      changedOwners: [],
      reason: 'workflow belongs to a different parent session',
    });

    expect(
      router.routeLifecycle(
        runStart('claimed-workflow', PARENT_SESSION_ID),
        PARENT_SESSION_ID
      ).accepted
    ).toBe(true);

    expect(
      router.routeLifecycle(
        runStart('claimed-workflow', OTHER_PARENT_SESSION_ID),
        OTHER_PARENT_SESSION_ID
      )
    ).toMatchObject({
      accepted: false,
      parentSessionId: OTHER_PARENT_SESSION_ID,
      changedOwners: [],
      reason: 'workflow belongs to a different parent session',
    });
  });

  it('rejects conflicting terminal identity without mutating owners', () => {
    const router = new WorkflowOwnerRegistry();
    const owner = registerNode(router);
    const response = loadedRun();
    response.workflowId = owner.workflowId;
    response.state = {
      ...response.state,
      workflowId: owner.workflowId,
      parentSessionId: OTHER_PARENT_SESSION_ID,
      status: 'failed',
    };

    expect(
      router.routeLifecycle(
        {
          type: 'run_complete',
          workflowId: owner.workflowId,
          parentSessionId: PARENT_SESSION_ID,
          status: 'failed',
          finalState: response.state,
        },
        PARENT_SESSION_ID
      )
    ).toEqual({
      accepted: false,
      changedOwners: [],
      reason: 'conflicting workflow terminal identity',
    });
    expect(router.resolveTarget(ownerTarget(owner))?.status).toBe('running');
  });

  it('rejects a terminal snapshot that rebinds a known child session', () => {
    const router = new WorkflowOwnerRegistry();
    const owner = registerNode(router);
    const response = loadedRun();
    response.workflowId = owner.workflowId;
    response.state = {
      ...response.state,
      workflowId: owner.workflowId,
      parentSessionId: PARENT_SESSION_ID,
      status: 'completed',
      root: {
        nodeId: 'root',
        type: 'sequence',
        status: 'completed',
        children: [
          {
            nodeId: 'different-node',
            type: 'step',
            status: 'completed',
            sessionId: owner.sessionId,
          },
        ],
      },
    };

    expect(
      router.routeLifecycle(
        {
          type: 'run_complete',
          workflowId: owner.workflowId,
          parentSessionId: PARENT_SESSION_ID,
          status: 'completed',
          finalState: response.state,
        },
        PARENT_SESSION_ID
      )
    ).toEqual({
      accepted: false,
      changedOwners: [],
      reason: 'conflicting workflow terminal identity',
    });
    expect(router.resolveTarget(ownerTarget(owner))?.status).toBe('running');
  });

  it('rejects a child session claimed by a conflicting node identity', () => {
    const router = new WorkflowOwnerRegistry();
    const owner = registerNode(router);

    expect(
      router.routeLifecycle(
        nodeStart({
          nodeId: 'other-node',
          nodePath: ['root', 'other-node'],
        }),
        PARENT_SESSION_ID
      )
    ).toMatchObject({
      accepted: false,
      parentSessionId: PARENT_SESSION_ID,
      changedOwners: [],
      reason: 'conflicting workflow session ownership',
    });
    expect(router.resolveTarget(ownerTarget(owner))).toEqual(owner);

    expect(
      router.routeLifecycle(
        nodeStart({
          workflowId: 'invalid-parent-child',
          nodeId: 'parent',
          nodePath: ['invalid-parent-child', 'parent'],
          sessionId: PARENT_SESSION_ID,
          iteration: undefined,
          branchId: undefined,
        }),
        PARENT_SESSION_ID
      )
    ).toMatchObject({
      accepted: false,
      changedOwners: [],
      reason: 'conflicting workflow session ownership',
    });
  });

  it('resolves only the explicitly owned node session identity', () => {
    const router = new WorkflowOwnerRegistry();
    const owner = registerNode(router);

    expect(owner).toEqual({
      ...NODE_TARGET,
      status: 'running',
      agentName: 'builder',
    });
    expect(router.resolveTarget(NODE_TARGET)).toEqual(owner);

    const conflictingTargets: WorkflowNodeSessionTarget[] = [
      { ...NODE_TARGET, workflowId: 'other-workflow' },
      { ...NODE_TARGET, parentSessionId: OTHER_PARENT_SESSION_ID },
      { ...NODE_TARGET, nodeId: 'other-node' },
      { ...NODE_TARGET, nodePath: ['root', 'other-node'] },
      { ...NODE_TARGET, sessionId: 'other-node-session' },
      { ...NODE_TARGET, iteration: NODE_TARGET.iteration + 1 },
      { ...NODE_TARGET, branchId: 'branch-b' },
    ];
    for (const target of conflictingTargets) {
      expect(router.resolveTarget(target)).toBeUndefined();
    }

    const partialOwner = registerNode(router, {
      workflowId: 'partial-workflow',
      nodeId: 'partial-node',
      nodePath: ['root', 'partial-node'],
      sessionId: 'partial-session',
      iteration: undefined,
      branchId: undefined,
    });
    expect(
      router.resolveTarget({ ...ownerTarget(partialOwner), iteration: 1 })
    ).toBeUndefined();
    expect(
      router.resolveTarget({
        ...ownerTarget(partialOwner),
        branchId: 'branch-a',
      })
    ).toBeUndefined();
  });

  it('atomically replaces a retried node owner', () => {
    const router = new WorkflowOwnerRegistry();
    const previous = registerNode(router);
    const replacementSessionId = 'retry-node-session';

    expect(
      router.routeLifecycle(
        nodeStart({ sessionId: replacementSessionId }),
        PARENT_SESSION_ID
      )
    ).toEqual({
      accepted: true,
      parentSessionId: PARENT_SESSION_ID,
      changedOwners: [
        {
          ...previous,
          sessionId: replacementSessionId,
        },
      ],
      removedSessionIds: [previous.sessionId],
    });
    expect(router.ownerForSession(previous.sessionId)).toBeUndefined();
    expect(router.ownerForSession(replacementSessionId)).toMatchObject({
      workflowId: previous.workflowId,
      nodeId: previous.nodeId,
      nodePath: previous.nodePath,
    });
  });

  it('does not mutate owners when a completion matches multiple sessions', () => {
    const router = new WorkflowOwnerRegistry();
    const first = registerNode(router, {
      nodePath: ['root', 'repeat', 'implement'],
      sessionId: 'iteration-1-session',
      iteration: 1,
      branchId: 'branch-a',
    });
    const second = registerNode(router, {
      nodePath: ['root', 'repeat', 'implement'],
      sessionId: 'iteration-2-session',
      iteration: 2,
      branchId: 'branch-b',
    });

    const result = router.routeLifecycle(
      {
        type: 'node_complete',
        workflowId: NODE_TARGET.workflowId,
        parentSessionId: PARENT_SESSION_ID,
        nodeId: NODE_TARGET.nodeId,
        nodePath: ['root', 'repeat', NODE_TARGET.nodeId],
        status: 'completed',
      },
      PARENT_SESSION_ID
    );

    expect(result).toEqual({
      accepted: true,
      parentSessionId: PARENT_SESSION_ID,
      changedOwners: [],
      removedSessionIds: [],
    });
    expect(router.resolveTarget(ownerTarget(first))?.status).toBe('running');
    expect(router.resolveTarget(ownerTarget(second))?.status).toBe('running');
  });

  it('hides the initial user prompt only until child output begins', () => {
    const router = new WorkflowChildMessageState();

    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        userMessage('before-registration')
      )
    ).toBe(false);
    router.hideInitialPrompt(NODE_TARGET.sessionId);

    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        userMessage('initial-prompt-chunk-1')
      )
    ).toBe(true);
    expect(
      router.shouldHideInitialPrompt(NODE_TARGET.sessionId, {
        type: AgentEventType.Metadata,
      })
    ).toBe(false);
    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        userMessage('initial-prompt-chunk-2')
      )
    ).toBe(true);
    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        assistantContent('first-output')
      )
    ).toBe(false);
    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        userMessage('later-user-message')
      )
    ).toBe(false);

    router.hideInitialPrompt(NODE_TARGET.sessionId);
    expect(
      router.shouldHideInitialPrompt(
        NODE_TARGET.sessionId,
        userMessage('explicitly-hidden-prompt')
      )
    ).toBe(true);
  });

  it('suppresses parent relays only when message metadata identifies a workflow', () => {
    const router = new WorkflowParentRelayFilter();
    const workflowMeta: KiroMeta = {
      workflow: {
        workflowId: NODE_TARGET.workflowId,
        nodeId: NODE_TARGET.nodeId,
        nodePath: NODE_TARGET.nodePath,
        iteration: NODE_TARGET.iteration,
        branchId: NODE_TARGET.branchId,
      },
    };
    const notificationMeta: KiroMeta = {
      kind: 'workflow-progress',
      notification: {
        kind: 'workflow-progress',
        workflowId: NODE_TARGET.workflowId,
      },
    };
    const ordinaryNotificationMeta: KiroMeta = {
      kind: 'system-notification',
      notification: {
        kind: 'system-notification',
        workflowId: NODE_TARGET.workflowId,
      },
    };

    expect(
      router.shouldSuppress(userMessage('workflow-message'), workflowMeta)
    ).toBe(true);
    expect(
      router.shouldSuppress(
        assistantContent('workflow-notification'),
        notificationMeta
      )
    ).toBe(true);
    expect(
      router.shouldSuppress(userMessage('ordinary-parent-message'), undefined)
    ).toBe(false);
    expect(
      router.shouldSuppress(
        assistantContent('ordinary-system-notification'),
        ordinaryNotificationMeta
      )
    ).toBe(false);
  });

  it('correlates metadata-poor tool updates and finishes to a tagged parent relay', () => {
    const router = new WorkflowParentRelayFilter();
    const notificationMeta: KiroMeta = {
      kind: 'workflow-progress',
      notification: {
        kind: 'workflow-progress',
        workflowId: NODE_TARGET.workflowId,
      },
    };
    const toolCallId = 'workflow-tool-call';

    expect(router.shouldSuppress(toolCall(toolCallId), notificationMeta)).toBe(
      true
    );
    expect(router.shouldSuppress(toolCall(toolCallId), undefined)).toBe(true);
    expect(router.shouldSuppress(toolCallUpdate(toolCallId), undefined)).toBe(
      true
    );
    expect(router.shouldSuppress(toolCallFinished(toolCallId), undefined)).toBe(
      true
    );
    expect(router.shouldSuppress(toolCallUpdate(toolCallId), undefined)).toBe(
      false
    );
  });

  it('registers loaded owners only for the active, non-conflicting parent', () => {
    const response = loadedRun();
    const router = new WorkflowOwnerRegistry();
    const missingParentResponse = loadedRun();
    missingParentResponse.state.parentSessionId = undefined;

    expect(router.registerLoadedRun(response, undefined)).toBeUndefined();
    expect(
      router.registerLoadedRun(missingParentResponse, PARENT_SESSION_ID)
    ).toBeUndefined();
    expect(
      router.registerLoadedRun(response, OTHER_PARENT_SESSION_ID)
    ).toBeUndefined();
    expect(
      router.resolveTarget({
        workflowId: response.workflowId,
        parentSessionId: PARENT_SESSION_ID,
        ...response.stepSessions[0]!,
      })
    ).toBeUndefined();

    const registered = router.registerLoadedRun(response, PARENT_SESSION_ID);
    expect(registered).toEqual({
      owners: [
        {
          workflowId: response.workflowId,
          parentSessionId: PARENT_SESSION_ID,
          ...response.stepSessions[0]!,
          status: 'completed',
          agentName: 'reviewer',
        },
      ],
      removedSessionIds: [],
    });

    const conflictingRouter = new WorkflowOwnerRegistry();
    expect(
      conflictingRouter.routeLifecycle(
        runStart(response.workflowId, OTHER_PARENT_SESSION_ID),
        OTHER_PARENT_SESSION_ID
      ).accepted
    ).toBe(true);
    expect(
      conflictingRouter.registerLoadedRun(response, PARENT_SESSION_ID)
    ).toBeUndefined();

    const parentAsChild = loadedRun();
    parentAsChild.stepSessions = [
      {
        nodeId: 'root',
        nodePath: ['root'],
        sessionId: PARENT_SESSION_ID,
      },
    ];
    const parentAsChildRouter = new WorkflowOwnerRegistry();
    expect(
      parentAsChildRouter.registerLoadedRun(parentAsChild, PARENT_SESSION_ID)
    ).toBeUndefined();
    expect(
      parentAsChildRouter.ownerForSession(PARENT_SESSION_ID)
    ).toBeUndefined();
  });

  it('retires a superseded owner from an authoritative load', () => {
    const router = new WorkflowOwnerRegistry();
    const previous = registerNode(router, {
      workflowId: 'loaded-workflow',
      nodeId: 'review',
      nodePath: ['root', 'review'],
      sessionId: 'previous-loaded-session',
      iteration: 3,
      branchId: 'review-branch',
    });
    const response = loadedRun();
    response.state.root.children![0]!.sessionId = 'replacement-loaded-session';
    response.stepSessions[0]!.sessionId = 'replacement-loaded-session';

    expect(router.registerLoadedRun(response, PARENT_SESSION_ID)).toEqual({
      owners: [
        {
          ...previous,
          sessionId: 'replacement-loaded-session',
          status: 'completed',
          agentName: 'reviewer',
        },
      ],
      removedSessionIds: [previous.sessionId],
    });
    expect(router.ownerForSession(previous.sessionId)).toBeUndefined();
    expect(router.ownerForSession('replacement-loaded-session')).toBeDefined();
  });

  it('enriches partial live ownership from durable workflow state', () => {
    const router = new WorkflowOwnerRegistry();
    const canonicalTarget = {
      ...NODE_TARGET,
      nodePath: ['root', 'loop', 'iter-2', NODE_TARGET.nodeId],
    } as const satisfies WorkflowNodeSessionTarget;
    const partial = registerNode(router, {
      nodePath: canonicalTarget.nodePath,
      iteration: undefined,
      branchId: undefined,
    });
    const response = loadedRun();
    response.workflowId = partial.workflowId;
    response.state.workflowId = partial.workflowId;
    response.state.root!.children = [
      {
        nodeId: 'loop',
        type: 'repeat',
        status: 'completed',
        children: [
          {
            nodeId: 'loop#2',
            type: 'sequence',
            status: 'completed',
            iteration: NODE_TARGET.iteration,
            children: [
              {
                nodeId: partial.nodeId,
                type: 'step',
                status: 'completed',
                sessionId: partial.sessionId,
                iteration: NODE_TARGET.iteration,
                branchId: NODE_TARGET.branchId,
                agentName: 'builder',
              },
            ],
          },
        ],
      },
    ];
    response.stepSessions = [
      {
        nodeId: partial.nodeId,
        nodePath: canonicalTarget.nodePath,
        sessionId: partial.sessionId,
        iteration: NODE_TARGET.iteration,
        branchId: NODE_TARGET.branchId,
      },
    ];

    expect(router.registerLoadedRun(response, PARENT_SESSION_ID)).toEqual({
      owners: [
        {
          ...canonicalTarget,
          status: 'completed',
          agentName: 'builder',
        },
      ],
      removedSessionIds: [],
    });
    expect(router.resolveTarget(canonicalTarget)).toMatchObject({
      status: 'completed',
    });
  });

  it('recovers completed child ownership from durable state without live refs', () => {
    const response = loadedRun();
    response.stepSessions = [];
    const router = new WorkflowOwnerRegistry();

    expect(router.registerLoadedRun(response, PARENT_SESSION_ID)).toEqual({
      owners: [
        {
          workflowId: response.workflowId,
          parentSessionId: PARENT_SESSION_ID,
          nodeId: 'review',
          nodePath: ['root', 'review'],
          sessionId: 'loaded-node-session',
          iteration: 3,
          branchId: 'review-branch',
          status: 'completed',
          agentName: 'reviewer',
        },
      ],
      removedSessionIds: [],
    });
  });

  it('rejects loaded session refs that conflict with durable node identity', () => {
    const response = loadedRun();
    response.stepSessions[0] = {
      ...response.stepSessions[0]!,
      nodeId: 'different-node',
    };
    const router = new WorkflowOwnerRegistry();

    expect(
      router.registerLoadedRun(response, PARENT_SESSION_ID)
    ).toBeUndefined();
    expect(
      router.ownerForSession(response.stepSessions[0]!.sessionId)
    ).toBeUndefined();
  });

  it('steers running nodes and prompts completed nodes', () => {
    const router = new WorkflowOwnerRegistry();
    const eventRouter = new WorkflowChildMessageState();
    const runningOwner = registerNode(router);

    expect(eventRouter.messageDelivery(runningOwner)).toBe('steer');

    const completion: WorkflowRouteResult = router.routeLifecycle(
      {
        type: 'node_complete',
        workflowId: NODE_TARGET.workflowId,
        parentSessionId: PARENT_SESSION_ID,
        nodeId: NODE_TARGET.nodeId,
        nodePath: NODE_TARGET.nodePath,
        sessionId: NODE_TARGET.sessionId,
        iteration: NODE_TARGET.iteration,
        branchId: NODE_TARGET.branchId,
        status: 'completed',
      },
      PARENT_SESSION_ID
    );
    const completedOwner = completion.changedOwners[0]!;

    expect(completedOwner.status).toBe('completed');
    expect(eventRouter.messageDelivery(completedOwner)).toBe('prompt');

    eventRouter.beginPrompt(completedOwner.sessionId);
    expect(eventRouter.messageDelivery(completedOwner)).toBe('steer');
    eventRouter.endPrompt(completedOwner.sessionId);
    expect(eventRouter.messageDelivery(completedOwner)).toBe('prompt');
  });
});
