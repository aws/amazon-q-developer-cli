import { describe, expect, it } from 'bun:test';
import type {
  WorkflowEvent,
  WorkflowLoadResponse,
} from '../../../../types/workflow.js';
import {
  parseWorkflowCancelResponse,
  parseWorkflowInspectResponse,
  parseWorkflowListResponse,
  parseWorkflowPauseResponse,
  parsePersistedWorkflowProgress,
  parseWorkflowResumeResponse,
  parseWorkflowLoadResponse,
  parseWorkflowNotification,
} from '../contracts.js';

const PARENT_SESSION_ID = 'parent-session';

function validLoadResponse(): WorkflowLoadResponse {
  return {
    workflowId: 'workflow-1',
    state: {
      workflowId: 'workflow-1',
      workflowName: 'Contract test',
      status: 'completed',
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      parentSessionId: PARENT_SESSION_ID,
      planRevision: 2,
      root: {
        nodeId: 'root',
        type: 'sequence',
        status: 'completed',
        children: [
          {
            nodeId: 'build',
            type: 'step',
            status: 'completed',
            sessionId: 'child-session',
          },
        ],
      },
    },
    stepSessions: [
      {
        nodeId: 'build',
        nodePath: ['root', 'build'],
        sessionId: 'child-session',
      },
    ],
  };
}

describe('workflow protocol boundary', () => {
  it('normalizes a typed node_start notification', () => {
    expect(
      parseWorkflowNotification('_kiro/workflow/node_start', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        nodeId: 'build',
        nodePath: ['root', 'build'],
        type: 'step',
        sessionId: 'child-session',
        iteration: 2,
        branchId: 'branch-a',
      })
    ).toEqual({
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: ['root', 'build'],
      type: 'node_start',
      nodeType: 'step',
      sessionId: 'child-session',
      iteration: 2,
      branchId: 'branch-a',
    });
  });

  it('normalizes terminal aliases into run_complete', () => {
    expect(
      parseWorkflowNotification('_kiro/workflow/run_failed', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
      })
    ).toEqual({
      type: 'run_complete',
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      status: 'failed',
      legacyTerminalAlias: true,
    });
    expect(
      parseWorkflowNotification('_kiro/workflow/run_aborted', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
      })
    ).toMatchObject({ type: 'run_complete', status: 'aborted' });
  });

  it('accepts only coherent canonical terminal envelopes', () => {
    const finalState = validLoadResponse().state;
    expect(
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'completed',
        finalState,
      })
    ).toEqual({
      type: 'run_complete',
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      status: 'completed',
      finalState,
    });

    const conflicts = [
      { status: 'failed' },
      { finalState: { ...finalState, workflowId: 'other-workflow' } },
      { finalState: { ...finalState, status: 'failed' } },
      {
        finalState: {
          ...finalState,
          parentSessionId: 'other-parent-session',
        },
      },
    ];
    for (const conflict of conflicts) {
      expect(
        parseWorkflowNotification('_kiro/workflow/run_complete', {
          workflowId: 'workflow-1',
          parentSessionId: PARENT_SESSION_ID,
          status: 'completed',
          finalState,
          ...conflict,
        })
      ).toBeNull();
    }

    expect(
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'running',
        finalState: {
          ...finalState,
          status: 'running',
          root: { ...finalState.root, status: 'running' },
        },
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'paused',
        finalState: {
          ...finalState,
          status: 'paused',
          root: { ...finalState.root, status: 'paused' },
        },
      })
    ).not.toBeNull();
  });

  it('validates terminal aliases that include a final snapshot', () => {
    const completedState = validLoadResponse().state;
    const failedState = {
      ...completedState,
      status: 'failed' as const,
      root: { ...completedState.root, status: 'failed' as const },
    };
    expect(
      parseWorkflowNotification('_kiro/workflow/run_failed', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'failed',
        finalState: failedState,
      })
    ).toEqual({
      type: 'run_complete',
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      status: 'failed',
      finalState: failedState,
    });

    expect(
      parseWorkflowNotification('_kiro/workflow/run_failed', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'aborted',
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/run_failed', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        finalState: { status: 'failed' },
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/run_failed', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        finalState: {
          ...failedState,
          parentSessionId: 'other-parent-session',
        },
      })
    ).toBeNull();
  });

  it('rejects empty, duplicate, and parent-owned child session identities', () => {
    const finalState = validLoadResponse().state;
    const parseState = (root: WorkflowLoadResponse['state']['root']) =>
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'completed',
        finalState: { ...finalState, root },
      });

    expect(
      parseState({
        ...finalState.root,
        children: [
          {
            nodeId: 'empty',
            type: 'step',
            status: 'completed',
            sessionId: '',
          },
        ],
      })
    ).toBeNull();
    expect(
      parseState({
        ...finalState.root,
        children: [
          {
            nodeId: 'first',
            type: 'step',
            status: 'completed',
            sessionId: 'duplicate-session',
          },
          {
            nodeId: 'second',
            type: 'step',
            status: 'completed',
            sessionId: 'duplicate-session',
          },
        ],
      })
    ).toBeNull();
    expect(
      parseState({
        ...finalState.root,
        children: [
          {
            nodeId: 'parent-alias',
            type: 'step',
            status: 'completed',
            sessionId: PARENT_SESSION_ID,
          },
        ],
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'completed',
        finalState: {
          ...finalState,
          parentSessionId: undefined,
          root: {
            ...finalState.root,
            children: [
              {
                nodeId: 'outer-parent-alias',
                type: 'step',
                status: 'completed',
                sessionId: PARENT_SESSION_ID,
              },
            ],
          },
        },
      })
    ).toBeNull();
  });

  it('does not trust a payload-controlled legacy marker', () => {
    const finalState = validLoadResponse().state;
    expect(
      parseWorkflowNotification('_kiro/workflow/run_complete', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        status: 'completed',
        finalState,
        legacyTerminalAlias: true,
      })
    ).toMatchObject({
      type: 'run_complete',
      status: 'completed',
      finalState,
    });
  });

  it('rejects malformed lifecycle payloads', () => {
    expect(
      parseWorkflowNotification('_kiro/workflow/node_start', {
        workflowId: '',
        nodeId: 'build',
        type: 'step',
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/node_complete', {
        workflowId: 'workflow-1',
        nodeId: 'build',
        status: 'not-a-status',
      })
    ).toBeNull();
    expect(
      parseWorkflowNotification('_kiro/workflow/not_supported', {
        workflowId: 'workflow-1',
      })
    ).toBeNull();
  });

  it('validates nested plan configuration instead of trusting object casts', () => {
    const runStart = (nodeTree: unknown[]) =>
      parseWorkflowNotification('_kiro/workflow/run_start', {
        workflowId: 'workflow-1',
        parentSessionId: PARENT_SESSION_ID,
        workflowName: 'Contract test',
        inputs: {},
        nodeTree,
      });

    expect(
      runStart([
        {
          nodeId: 'repeat',
          type: 'repeat',
          maxIterations: 3,
          onMaxIterations: 'pause',
          stopCondition: {
            fileCheck: { path: 'done.json', jsonPath: 'done', value: true },
          },
          steps: [{ nodeId: 'build', type: 'step' }],
        },
      ])
    ).not.toBeNull();
    expect(
      runStart([
        {
          nodeId: 'repeat',
          type: 'repeat',
          maxIterations: 0,
          stopCondition: {},
        },
      ])
    ).toBeNull();
    expect(
      runStart([
        {
          nodeId: 'parallel',
          type: 'parallel',
          joinPolicy: 'first',
        },
      ])
    ).toBeNull();
  });

  it('rehydrates only persisted workflow-progress messages', () => {
    const workflowEvent = {
      type: 'node_complete',
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: ['workflow-1', 'build'],
      status: 'completed',
    } as const satisfies WorkflowEvent;
    const persistedPayload = {
      method: '_kiro/workflow/node_complete',
      ...workflowEvent,
    };
    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: { type: 'text', text: JSON.stringify(persistedPayload) },
        _meta: {
          kiro: {
            messageId: 'wf-progress-1',
            notification: {
              kind: 'workflow-progress',
              notifyId: 'notify-1',
            },
          },
        },
      })
    ).toEqual({
      kind: 'workflow-progress',
      progress: { event: workflowEvent, messageId: 'wf-progress-1' },
    });

    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: { type: 'text', text: JSON.stringify(persistedPayload) },
        _meta: {
          kiro: {
            notification: { kind: 'system-notification' },
          },
        },
      })
    ).toEqual({ kind: 'not-workflow' });
  });

  it('accepts current and legacy persisted lifecycle metadata', () => {
    const eventPayload = {
      parentSessionId: PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: ['workflow-1', 'build'],
      status: 'completed' as const,
    } as const;

    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: { type: 'text', text: JSON.stringify(eventPayload) },
        _meta: {
          kiro: {
            notification: {
              kind: 'workflow-progress',
              workflowId: 'workflow-1',
              eventType: 'node_complete',
              notifyId: 'notify-current',
            },
          },
        },
      })
    ).toEqual({
      kind: 'workflow-progress',
      progress: {
        event: {
          ...eventPayload,
          type: 'node_complete',
          workflowId: 'workflow-1',
        },
        messageId: 'notify-current',
      },
    });

    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: {
          type: 'text',
          text: JSON.stringify({
            method: '_kiro/workflow/node_complete',
            workflowId: 'workflow-1',
            ...eventPayload,
          }),
        },
        _meta: {
          kiro: {
            kind: 'workflow-progress',
            messageId: 'wf-progress-legacy',
          },
        },
      })
    ).toMatchObject({
      kind: 'workflow-progress',
      progress: { messageId: 'wf-progress-legacy' },
    });
  });

  it('rejects conflicting persisted lifecycle identity', () => {
    const baseUpdate = {
      sessionUpdate: 'user_message_chunk',
      _meta: {
        kiro: {
          notification: {
            kind: 'workflow-progress',
            workflowId: 'workflow-1',
            eventType: 'node_complete',
          },
        },
      },
    };
    const eventPayload = {
      method: '_kiro/workflow/node_complete',
      workflowId: 'workflow-1',
      parentSessionId: PARENT_SESSION_ID,
      nodeId: 'build',
      nodePath: ['workflow-1', 'build'],
      status: 'completed',
    };

    for (const payload of [
      {
        ...eventPayload,
        method: '_kiro/workflow/node_start',
        type: 'step',
      },
      { ...eventPayload, workflowId: 'other-workflow' },
      { ...eventPayload, method: '' },
      { ...eventPayload, workflowId: '' },
    ]) {
      expect(
        parsePersistedWorkflowProgress({
          ...baseUpdate,
          content: { type: 'text', text: JSON.stringify(payload) },
        })
      ).toEqual({ kind: 'invalid-workflow' });
    }
  });

  it('validates workflow load ownership responses', () => {
    const response = validLoadResponse();
    expect(parseWorkflowLoadResponse(response)).toEqual(response);

    expect(
      parseWorkflowLoadResponse({
        ...response,
        state: { ...response.state, workflowId: 'different-workflow' },
      })
    ).toBeNull();
    expect(
      parseWorkflowLoadResponse({
        ...response,
        state: { ...response.state, planRevision: -1 },
      })
    ).toBeNull();
    expect(
      parseWorkflowLoadResponse({
        ...response,
        stepSessions: [{ nodeId: 'build', sessionId: '' }],
      })
    ).toBeNull();
    expect(
      parseWorkflowLoadResponse({
        ...response,
        stepSessions: [
          {
            nodeId: 'build',
            sessionId: 'child-session',
            nodePath: ['root', 123],
          },
        ],
      })
    ).toBeNull();
  });

  it('validates workflow history and control responses', () => {
    const load = validLoadResponse();
    expect(
      parseWorkflowListResponse({
        runs: [
          {
            workflowId: load.workflowId,
            name: 'Contract test',
            status: 'completed',
            createdAt: '2026-07-19T10:00:00.000Z',
            updatedAt: '2026-07-19T10:01:00.000Z',
            parentSessionId: PARENT_SESSION_ID,
          },
        ],
      })
    ).not.toBeNull();
    expect(
      parseWorkflowInspectResponse({
        workflowId: load.workflowId,
        state: load.state,
        nodePlan: [{ nodeId: 'build', type: 'step' }],
      })
    ).not.toBeNull();
    expect(parseWorkflowPauseResponse({ paused: true })).toEqual({
      paused: true,
    });
    expect(
      parseWorkflowResumeResponse({
        workflowId: load.workflowId,
        status: 'running',
      })
    ).toEqual({ workflowId: load.workflowId, status: 'running' });
    expect(
      parseWorkflowCancelResponse({
        ok: true,
        previousStatus: 'running',
      })
    ).toEqual({ ok: true, previousStatus: 'running' });
  });

  it('rejects malformed workflow history and control responses', () => {
    const load = validLoadResponse();
    expect(
      parseWorkflowListResponse({
        runs: [{ workflowId: load.workflowId, status: 'completed' }],
      })
    ).toBeNull();
    expect(
      parseWorkflowInspectResponse({
        workflowId: 'different',
        state: load.state,
      })
    ).toBeNull();
    expect(parseWorkflowPauseResponse({ paused: 'yes' })).toBeNull();
    expect(
      parseWorkflowResumeResponse({
        workflowId: load.workflowId,
        status: 'unknown',
      })
    ).toBeNull();
    expect(
      parseWorkflowCancelResponse({
        ok: true,
        previousStatus: 'unknown',
      })
    ).toBeNull();
  });
});
