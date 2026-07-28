import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import type { WorkflowNodeDescriptor } from '../../types/workflow';
import { logger } from '../logger';
import {
  parsePersistedWorkflowProgress,
  parseWorkflowNotification,
} from '../../acp-client/kas-extensions/workflow/contracts';

function createNodeTree(depth: number): WorkflowNodeDescriptor {
  if (depth === 0) {
    return {
      nodeId: 'leaf',
      type: 'step',
    };
  }

  return {
    nodeId: `node-${depth}`,
    type: 'sequence',
    steps: [createNodeTree(depth - 1)],
  };
}

describe('parseWorkflowNotification', () => {
  it('normalizes node_start payloads into workflow events', () => {
    const event = parseWorkflowNotification('_kiro/workflow/node_start', {
      workflowId: 'wf-123',
      nodeId: 'node-1',
      type: 'step',
      nodePath: ['root', 'node-1'],
      agentName: 'planner',
    });

    expect(event).toEqual({
      workflowId: 'wf-123',
      nodeId: 'node-1',
      type: 'node_start',
      nodeType: 'step',
      nodePath: ['root', 'node-1'],
      agentName: 'planner',
    });
  });

  it('rejects run_start payloads that exceed the node-tree depth limit', () => {
    const event = parseWorkflowNotification('_kiro/workflow/run_start', {
      workflowId: 'wf-123',
      workflowName: 'deep-tree',
      nodeTree: [createNodeTree(101)],
    });

    expect(event).toBeNull();
  });
});

describe('parsePersistedWorkflowProgress', () => {
  let originalDebug: typeof logger.debug;
  let debugCalls: Array<unknown[]>;

  beforeEach(() => {
    originalDebug = logger.debug.bind(logger);
    debugCalls = [];
    logger.debug = ((...args: unknown[]) => {
      debugCalls.push(args);
    }) as typeof logger.debug;
  });

  afterEach(() => {
    logger.debug = originalDebug;
  });

  it('rehydrates persisted workflow progress using notification fallback metadata', () => {
    const progress = parsePersistedWorkflowProgress({
      sessionUpdate: 'user_message_chunk',
      content: {
        type: 'text',
        text: JSON.stringify({
          nodeId: 'node-1',
          nodePath: ['workflow', 'node-1'],
          parentSessionId: 'parent-1',
          status: 'completed',
        }),
      },
      _meta: {
        kiro: {
          messageId: 'wf-progress-1',
          notification: {
            kind: 'workflow-progress',
            eventType: 'node_complete',
            workflowId: 'wf-123',
          },
        },
      },
    });

    expect(progress).toEqual({
      kind: 'workflow-progress',
      progress: {
        messageId: 'wf-progress-1',
        event: {
          workflowId: 'wf-123',
          nodeId: 'node-1',
          nodePath: ['workflow', 'node-1'],
          parentSessionId: 'parent-1',
          status: 'completed',
          type: 'node_complete',
        },
      },
    });
  });

  it('distinguishes ordinary user rows from invalid workflow records', () => {
    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: { type: 'text', text: 'hello' },
      })
    ).toEqual({ kind: 'not-workflow' });

    expect(
      parsePersistedWorkflowProgress({
        sessionUpdate: 'user_message_chunk',
        content: {
          type: 'text',
          text: JSON.stringify({
            method: '_kiro/workflow/future_event',
            workflowId: 'wf-123',
          }),
        },
        _meta: {
          kiro: {
            messageId: 'wf-progress-unknown',
            kind: 'workflow-progress',
          },
        },
      })
    ).toEqual({
      kind: 'invalid-workflow',
      messageId: 'wf-progress-unknown',
    });
  });

  it('logs malformed persisted workflow JSON before dropping the record', () => {
    const progress = parsePersistedWorkflowProgress({
      sessionUpdate: 'user_message_chunk',
      content: {
        type: 'text',
        text: '{invalid json',
      },
      _meta: {
        kiro: {
          messageId: 'wf-progress-bad',
          kind: 'workflow-progress',
        },
      },
    });

    expect(progress).toEqual({
      kind: 'invalid-workflow',
      messageId: 'wf-progress-bad',
    });
    expect(debugCalls).toHaveLength(1);
    expect(debugCalls[0]?.[0]).toBe(
      'workflow-protocol: failed to parse persisted progress'
    );
    expect(debugCalls[0]?.[1]).toMatchObject({
      messageId: 'wf-progress-bad',
    });
    expect(debugCalls[0]?.[1]).toHaveProperty('error');
  });
});
