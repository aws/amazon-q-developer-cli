import { describe, expect, it } from 'bun:test';
import { MessageRole } from '../types/message-role.js';
import type {
  WorkflowProgressEvent,
  WorkflowStatus,
} from '../types/workflow.js';
import {
  activeWorkflowOriginTurnId,
  appendWorkflowLifecycleMessage,
  createWorkflowLifecycleTracker,
  hasWorkflowLaunchToolForTurn,
} from './workflow-lifecycle.js';

function runStart(workflowId: string): WorkflowProgressEvent {
  return {
    type: 'run_start',
    workflowId,
    workflowName: `name-${workflowId}`,
    inputs: {},
    nodeTree: [],
  };
}

function runComplete(
  workflowId: string,
  status: Extract<WorkflowStatus, 'completed' | 'failed' | 'aborted'>
): WorkflowProgressEvent {
  return {
    type: 'run_complete',
    workflowId,
    status,
    finalState: {
      workflowId,
      workflowName: `name-${workflowId}`,
      status,
      inputs: {},
      artifacts: {},
      capturedOutputs: {},
      root: {
        nodeId: 'root',
        type: 'sequence',
        status,
      },
    },
  };
}

describe('workflow lifecycle projection', () => {
  it('deduplicates terminal rows while preserving array identity', () => {
    const first = appendWorkflowLifecycleMessage([], {
      workflowId: 'wf-1',
      workflowName: 'release',
      status: 'completed',
    });
    const duplicate = appendWorkflowLifecycleMessage(first, {
      workflowId: 'wf-1',
      workflowName: 'release',
      status: 'completed',
    });

    expect(duplicate).toBe(first);
    expect(first).toEqual([
      {
        id: 'workflow-lifecycle:wf-1:completed',
        role: MessageRole.System,
        content: 'Workflow "release" completed',
        success: true,
        kind: 'workflow-completion',
        workflowId: 'wf-1',
        workflowName: 'release',
        workflowStatus: 'completed',
      },
    ]);
  });

  it.each([
    ['failed', false],
    ['aborted', false],
    ['started', true],
  ] as const)(
    'projects %s status with the expected outcome',
    (status, success) => {
      const [message] = appendWorkflowLifecycleMessage([], {
        workflowId: 'wf-1',
        workflowName: 'release',
        status,
      });

      expect(message?.success).toBe(success);
      expect(message?.kind).toBe(
        status === 'started' ? 'workflow-lifecycle' : 'workflow-completion'
      );
    }
  );

  it('keeps late completion attached to its launch turn', () => {
    const tracker = createWorkflowLifecycleTracker('turn-1');
    expect(tracker.consume(runStart('wf-1'))).toEqual({
      workflowId: 'wf-1',
      workflowName: 'name-wf-1',
      status: 'started',
      workflowTurnId: 'turn-1',
    });
    tracker.recordOriginTurn('turn-2');

    expect(tracker.consume(runComplete('wf-1', 'completed'))).toEqual({
      workflowId: 'wf-1',
      workflowName: 'name-wf-1',
      status: 'completed',
      workflowTurnId: 'turn-1',
    });
  });

  it('leaves completion standalone when no launch turn was active', () => {
    const tracker = createWorkflowLifecycleTracker();
    expect(tracker.consume(runStart('wf-1'))).toEqual({
      workflowId: 'wf-1',
      workflowName: 'name-wf-1',
      status: 'started',
    });

    expect(tracker.consume(runComplete('wf-1', 'failed'))).toEqual({
      workflowId: 'wf-1',
      workflowName: 'name-wf-1',
      status: 'failed',
    });
  });

  it('selects only an active non-steered user turn as fallback ownership', () => {
    const messages = [
      { id: 'prompt', role: MessageRole.User },
      { id: 'steer', role: MessageRole.User, steered: true },
    ];

    expect(activeWorkflowOriginTurnId(messages, true)).toBe('prompt');
    expect(activeWorkflowOriginTurnId(messages, false)).toBeUndefined();
  });

  it('retains a settled goal command only for the goal workflow', () => {
    const messages = [
      {
        id: 'goal-turn',
        role: MessageRole.User,
        content: '/goal say hello',
      },
    ];

    expect(activeWorkflowOriginTurnId(messages, false, 'goal')).toBe(
      'goal-turn'
    );
    expect(
      activeWorkflowOriginTurnId(messages, false, 'release')
    ).toBeUndefined();
  });

  it('detects a launch tool only within its owning turn', () => {
    const messages = [
      { id: 'turn-1', role: MessageRole.User },
      {
        id: 'tool-1',
        role: MessageRole.ToolUse,
        name: 'run_workflow',
      },
      { id: 'turn-2', role: MessageRole.User },
      {
        id: 'tool-2',
        role: MessageRole.ToolUse,
        name: 'inspect_workflow',
      },
    ];

    expect(hasWorkflowLaunchToolForTurn(messages, 'turn-1')).toBe(true);
    expect(hasWorkflowLaunchToolForTurn(messages, 'turn-2')).toBe(false);
    expect(hasWorkflowLaunchToolForTurn(messages, undefined)).toBe(false);
  });
});
