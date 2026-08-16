import { describe, expect, it } from 'bun:test';
import {
  decodeExtSessionUpdate,
  extractKiroMeta,
} from '../session-update-contract.js';

describe('extension session update contract', () => {
  it('decodes a workflow-owned tool call chunk', () => {
    expect(
      decodeExtSessionUpdate({
        sessionId: 'child-session',
        update: {
          sessionUpdate: 'tool_call_chunk',
          toolCallId: 'tool-1',
          title: '@server/read',
          kind: 'read',
          _meta: {
            kiro: {
              workflow: {
                workflowId: 'workflow-1',
                nodeId: 'build',
                nodePath: ['root', 'build'],
                type: 'step',
              },
            },
          },
        },
      })
    ).toEqual({
      sessionId: 'child-session',
      update: {
        sessionUpdate: 'tool_call_chunk',
        toolCallId: 'tool-1',
        title: '@server/read',
        kind: 'read',
        kiroMeta: {
          workflow: {
            workflowId: 'workflow-1',
            nodeId: 'build',
            nodePath: ['root', 'build'],
            type: 'step',
          },
        },
      },
    });
  });

  it('rejects malformed required fields instead of casting them', () => {
    expect(
      decodeExtSessionUpdate({
        update: {
          sessionUpdate: 'tool_call_chunk',
          toolCallId: 7,
          title: 'read',
          kind: 'read',
        },
      })
    ).toBeNull();
    expect(
      decodeExtSessionUpdate({
        update: {
          sessionUpdate: 'retry_warning',
          attempt: 'one',
          maxAttempts: 3,
          delaySecs: 1,
          message: 'retrying',
        },
      })
    ).toBeNull();
  });

  it('decodes the shared retry and steering variants', () => {
    expect(
      decodeExtSessionUpdate({
        update: {
          sessionUpdate: 'retry_warning',
          attempt: 1,
          maxAttempts: 3,
          delaySecs: 2,
          message: 'retrying',
        },
      })?.update
    ).toEqual({
      sessionUpdate: 'retry_warning',
      attempt: 1,
      maxAttempts: 3,
      delaySecs: 2,
      message: 'retrying',
    });
    expect(
      decodeExtSessionUpdate({
        update: { sessionUpdate: 'AgentExecutionSteeringInjected' },
      })?.update
    ).toEqual({
      sessionUpdate: 'AgentExecutionSteeringInjected',
      content: '',
    });
  });

  it('decodes the message-only stream stall notice', () => {
    expect(
      decodeExtSessionUpdate({
        update: {
          sessionUpdate: 'stream_stall_notice',
          message: 'Still working, model is thinking...',
        },
      })?.update
    ).toEqual({
      sessionUpdate: 'stream_stall_notice',
      message: 'Still working, model is thinking...',
    });
    expect(
      decodeExtSessionUpdate({
        update: { sessionUpdate: 'stream_stall_notice', message: 7 },
      })
    ).toBeNull();
  });

  it('decodes the field-less stream discard notice', () => {
    expect(
      decodeExtSessionUpdate({
        update: { sessionUpdate: 'stream_discarded' },
      })?.update
    ).toEqual({ sessionUpdate: 'stream_discarded' });
  });

  it('drops malformed Kiro metadata at the protocol boundary', () => {
    expect(
      extractKiroMeta({
        _meta: {
          kiro: {
            pipeline: {
              groupId: 'pipeline-1',
              stages: [{ name: 'build', status: 'unknown' }],
            },
          },
        },
      })
    ).toBeUndefined();
  });

  it('preserves validated tool metadata', () => {
    expect(
      extractKiroMeta({
        _meta: {
          kiro: {
            toolName: 'AskUserQuestion',
            toolId: 'user_input',
            mcpServerName: 'builtin',
          },
        },
      })
    ).toEqual({
      toolName: 'AskUserQuestion',
      toolId: 'user_input',
      mcpServerName: 'builtin',
    });
  });

  it('preserves workflow metadata when sibling pipeline metadata is malformed', () => {
    expect(
      extractKiroMeta({
        _meta: {
          kiro: {
            pipeline: {
              groupId: 'pipeline-1',
              stages: [
                {
                  name: 'build',
                  role: 'builder',
                  status: 'new-status',
                  dependsOn: [],
                  agentSubtaskId: null,
                },
              ],
            },
            workflow: {
              workflowId: 'workflow-1',
              nodeId: 'build',
              nodePath: ['root', 'build'],
              type: 'step',
            },
          },
        },
      })
    ).toEqual({
      workflow: {
        workflowId: 'workflow-1',
        nodeId: 'build',
        nodePath: ['root', 'build'],
        type: 'step',
      },
    });
  });

  it('preserves typed hidden-turn markers and rejects malformed values', () => {
    expect(
      extractKiroMeta({
        _meta: {
          kiro: {
            agentInitiated: true,
            visibility: 'hidden',
          },
        },
      })
    ).toEqual({
      agentInitiated: true,
      visibility: 'hidden',
    });
    expect(
      extractKiroMeta({
        _meta: { kiro: { agentInitiated: 'true' } },
      })
    ).toBeUndefined();
    expect(
      extractKiroMeta({
        _meta: { kiro: { visibility: 1 } },
      })
    ).toBeUndefined();
  });
});
