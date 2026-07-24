import { beforeEach, describe, expect, it } from 'bun:test';
import { AgentEventType } from '../../types/agent-events';
import type {
  AgentStreamEvent,
  KiroMeta,
  ToolCallEvent,
  ToolCallFinishedEvent,
} from '../../types/agent-events';
import {
  extractInvokeSubagentResults,
  InvokeSubagentPipelineAdapter,
} from '../invoke-subagent-pipeline';

function invokeParentToolCall(
  overrides: Partial<ToolCallEvent> = {}
): ToolCallEvent {
  return {
    type: AgentEventType.ToolCall,
    id: 'tc-1',
    name: 'Sub-agent: architect',
    kind: 'other',
    args: {
      name: 'architect',
      prompt: 'Design the auth flow',
      explanation: 'Designing auth',
    },
    ...overrides,
  };
}

function invokeParentMeta(agentSubtaskId = 'sub-exec-1'): KiroMeta {
  return { kind: 'agent-subtask', agentSubtaskId };
}

describe('InvokeSubagentPipelineAdapter', () => {
  let adapter: InvokeSubagentPipelineAdapter;

  beforeEach(() => {
    adapter = new InvokeSubagentPipelineAdapter();
    // The adapter is cloud-session-gated; unit tests exercise it enabled.
    adapter.setEnabled(true);
  });

  it('is a pure passthrough while disabled (local sessions)', () => {
    const disabled = new InvokeSubagentPipelineAdapter();
    const event = invokeParentToolCall();
    const meta = invokeParentMeta();
    expect(disabled.normalize(event, meta)).toBe(meta);
    expect(event.name).toBe('Sub-agent: architect');
    expect(event.args.stages).toBeUndefined();
    // Disabling mid-session drops any claimed state.
    const toggled = new InvokeSubagentPipelineAdapter();
    toggled.setEnabled(true);
    toggled.normalize(invokeParentToolCall(), invokeParentMeta());
    toggled.setEnabled(false);
    const late = invokeParentToolCall({ id: 'tc-late' });
    const lateMeta = invokeParentMeta('sub-late');
    expect(toggled.normalize(late, lateMeta)).toBe(lateMeta);
    expect(late.name).toBe('Sub-agent: architect');
  });

  it('rewrites an invoke parent tool_call to a one-stage orchestrate pipeline', () => {
    const event = invokeParentToolCall();
    const meta = adapter.normalize(event, invokeParentMeta());

    expect(event.name).toBe('orchestrate_subagent');
    expect(meta?.pipeline).toEqual({
      groupId: 'invoke-tc-1',
      stages: [
        {
          // Stage name is the derived label (explanation, boilerplate
          // stripped) — invoke carries no semantic stage names.
          name: 'Designing auth',
          role: 'architect',
          status: 'running',
          dependsOn: [],
          agentSubtaskId: 'sub-exec-1',
        },
      ],
    });
    // The parent must NOT keep the subtask discriminators, or the routing
    // store would hide the parent card itself as crew activity.
    expect(meta?.kind).toBeUndefined();
    expect(meta?.agentSubtaskId).toBeUndefined();
    // Event carries the rewritten meta for downstream store stamping.
    expect(event.meta?.kiro?.pipeline?.groupId).toBe('invoke-tc-1');
  });

  it('projects orchestrate-shaped args (task + one stage with prompt_template)', () => {
    const event = invokeParentToolCall();
    adapter.normalize(event, invokeParentMeta());

    expect(event.args.task).toBe('Designing auth');
    expect(event.args.stages).toEqual([
      {
        name: 'Designing auth',
        role: 'architect',
        prompt_template: 'Design the auth flow',
      },
    ]);
  });

  it('falls back to prompt for task, and derives the stage label from it', () => {
    const event = invokeParentToolCall({
      name: 'Sub-agent: reviewer',
      args: { prompt: 'Review it' },
    });
    const meta = adapter.normalize(event, invokeParentMeta());

    expect(event.args.task).toBe('Review it');
    expect(meta?.pipeline?.stages[0]?.name).toBe('Review it');
    expect(meta?.pipeline?.stages[0]?.role).toBe('reviewer');
  });

  it('uses a generic name when neither args nor title carry one', () => {
    const event = invokeParentToolCall({
      name: 'Sub-agent execution',
      args: {},
    });
    const meta = adapter.normalize(event, invokeParentMeta());
    expect(meta?.pipeline?.stages[0]?.name).toBe('sub-agent');
  });

  it('marks the stage completed on successful ToolCallFinished', () => {
    adapter.normalize(invokeParentToolCall(), invokeParentMeta());
    const finished: ToolCallFinishedEvent = {
      type: AgentEventType.ToolCallFinished,
      id: 'tc-1',
      result: { status: 'success', output: 'the answer' },
    };
    const meta = adapter.normalize(finished, invokeParentMeta());
    expect(meta?.pipeline?.stages[0]?.status).toBe('completed');
  });

  it('marks the stage failed on error and cancelled terminals', () => {
    adapter.normalize(
      invokeParentToolCall({ id: 'tc-err' }),
      invokeParentMeta('sub-err')
    );
    const errMeta = adapter.normalize(
      {
        type: AgentEventType.ToolCallFinished,
        id: 'tc-err',
        result: { status: 'error', error: 'denied' },
      } as AgentStreamEvent,
      invokeParentMeta('sub-err')
    );
    expect(errMeta?.pipeline?.stages[0]?.status).toBe('failed');

    adapter.normalize(
      invokeParentToolCall({ id: 'tc-cancel' }),
      invokeParentMeta('sub-cancel')
    );
    // cancelled conversions arrive meta-less in base.ts
    const cancelMeta = adapter.normalize(
      {
        type: AgentEventType.ToolCallFinished,
        id: 'tc-cancel',
        result: { status: 'cancelled' },
      } as AgentStreamEvent,
      undefined
    );
    expect(cancelMeta?.pipeline?.stages[0]?.status).toBe('failed');
  });

  it('stamps updates for a claimed parent and forgets it after terminal', () => {
    adapter.normalize(invokeParentToolCall(), invokeParentMeta());
    const update: AgentStreamEvent = {
      type: AgentEventType.ToolCallUpdate,
      id: 'tc-1',
      content: { type: 'text' as never, text: 'progress' },
    };
    const meta = adapter.normalize(update, invokeParentMeta());
    expect(meta?.pipeline?.groupId).toBe('invoke-tc-1');

    adapter.normalize(
      {
        type: AgentEventType.ToolCallFinished,
        id: 'tc-1',
        result: { status: 'success', output: '' },
      } as AgentStreamEvent,
      invokeParentMeta()
    );
    // After terminal, unrelated updates with the same id pass through.
    const after = adapter.normalize(
      {
        type: AgentEventType.ToolCallUpdate,
        id: 'tc-1',
        content: { type: 'text' as never, text: 'late' },
      } as AgentStreamEvent,
      undefined
    );
    expect(after).toBeUndefined();
  });

  it('re-stamps a duplicate tool_call for an already-claimed parent', () => {
    const first = invokeParentToolCall();
    adapter.normalize(first, invokeParentMeta());
    // base.ts synthesizes a second ToolCall on the Completed-only response path.
    const dup = invokeParentToolCall({ args: { response: 'final text' } });
    const meta = adapter.normalize(dup, invokeParentMeta());
    expect(dup.name).toBe('orchestrate_subagent');
    expect(meta?.pipeline?.groupId).toBe('invoke-tc-1');
  });

  it('does NOT claim child events (agentSubtaskId without kind)', () => {
    const child: ToolCallEvent = {
      type: AgentEventType.ToolCall,
      id: 'tc-child',
      name: 'fs_read',
      args: {},
    };
    const childMeta: KiroMeta = { agentSubtaskId: 'sub-exec-1' };
    const meta = adapter.normalize(child, childMeta);
    expect(meta).toBe(childMeta);
    expect(child.name).toBe('fs_read');
  });

  it('does NOT claim orchestrate stage cards (their ids appear in pipeline meta first)', () => {
    // Orchestrate parent publishes its stage ids up front.
    const orchestrateMeta: KiroMeta = {
      pipeline: {
        groupId: 'pipeline-build',
        stages: [
          {
            name: 'coder',
            role: 'coder',
            status: 'running',
            dependsOn: [],
            agentSubtaskId: 'stage-sub-1',
          },
        ],
      },
    };
    const parent: ToolCallEvent = {
      type: AgentEventType.ToolCall,
      id: 'tc-orch',
      name: 'orchestrate_subagent',
      args: {},
    };
    const passthrough = adapter.normalize(parent, orchestrateMeta);
    expect(passthrough).toBe(orchestrateMeta);
    expect(parent.name).toBe('orchestrate_subagent');

    // A stage card then arrives with the SAME envelope shape as a
    // standalone invoke parent — it must pass through untouched.
    const stageCard: ToolCallEvent = {
      type: AgentEventType.ToolCall,
      id: 'tc-stage',
      name: 'Sub-agent: coder',
      args: { name: 'coder', prompt: 'implement' },
    };
    const stageMeta: KiroMeta = {
      kind: 'agent-subtask',
      agentSubtaskId: 'stage-sub-1',
    };
    const result = adapter.normalize(stageCard, stageMeta);
    expect(result).toBe(stageMeta);
    expect(stageCard.name).toBe('Sub-agent: coder');
  });

  it('coalesces concurrent invokes into ONE anchored group (N agents, N named stages)', () => {
    const a = invokeParentToolCall({
      id: 'tc-a',
      args: {
        name: 'general-task-execution',
        explanation: 'Propulsion systems',
      },
    });
    const b = invokeParentToolCall({
      id: 'tc-b',
      args: {
        name: 'general-task-execution',
        explanation: 'Structures and aero',
      },
    });
    const metaA = adapter.normalize(a, invokeParentMeta('sub-a'));
    const metaB = adapter.normalize(b, invokeParentMeta('sub-b'));

    // Both events target the SAME anchor card with a growing stage list.
    expect(a.id).toBe('tc-a');
    expect(b.id).toBe('tc-a');
    expect(metaA?.pipeline?.groupId).toBe('invoke-tc-a');
    expect(metaB?.pipeline?.groupId).toBe('invoke-tc-a');
    expect(metaB?.pipeline?.stages.map((s) => s.name)).toEqual([
      'Propulsion systems',
      'Structures and aero',
    ]);
    expect(metaB?.pipeline?.stages.map((s) => s.agentSubtaskId)).toEqual([
      'sub-a',
      'sub-b',
    ]);
    expect(b.args.stages).toHaveLength(2);
  });

  it('keeps the anchor open until the LAST member finishes, then finishes it', () => {
    const a = invokeParentToolCall({
      id: 'tc-a',
      args: { name: 'g', explanation: 'First topic' },
    });
    const b = invokeParentToolCall({
      id: 'tc-b',
      args: { name: 'g', explanation: 'Second topic' },
    });
    adapter.normalize(a, invokeParentMeta('sub-a'));
    adapter.normalize(b, invokeParentMeta('sub-b'));

    // First member finishes → demoted to an anchor UPDATE, not a finish.
    // Typed loosely: normalize() mutates the event's discriminant in place.
    const finA = {
      type: AgentEventType.ToolCallFinished,
      id: 'tc-a',
      result: { status: 'success', output: 'answer A' },
    } as { type: AgentEventType; id: string };
    const metaMid = adapter.normalize(
      finA as AgentStreamEvent,
      invokeParentMeta('sub-a')
    );
    expect(finA.type).toBe(AgentEventType.ToolCallUpdate);
    expect(finA.id).toBe('tc-a');
    expect(metaMid?.pipeline?.stages.map((s) => s.status)).toEqual([
      'completed',
      'running',
    ]);

    // Last member finishes → genuine anchor ToolCallFinished.
    const finB = {
      type: AgentEventType.ToolCallFinished,
      id: 'tc-b',
      result: { status: 'error', error: 'boom' },
    } as { type: AgentEventType; id: string };
    const metaEnd = adapter.normalize(
      finB as AgentStreamEvent,
      invokeParentMeta('sub-b')
    );
    expect(finB.type).toBe(AgentEventType.ToolCallFinished);
    expect(finB.id).toBe('tc-a');
    expect(metaEnd?.pipeline?.stages.map((s) => s.status)).toEqual([
      'completed',
      'failed',
    ]);
    const aggregate = (finB as unknown as ToolCallFinishedEvent).result;
    expect(aggregate.status).toBe('error');
    expect(aggregate.status === 'error' && aggregate.error).toBe(
      'Second topic: boom'
    );
    expect(extractInvokeSubagentResults(aggregate.output)).toEqual([
      { name: 'First topic', status: 'success', output: 'answer A' },
      { name: 'Second topic', status: 'error', error: 'boom' },
    ]);

    // Group closed: the next invoke starts a NEW anchor.
    const c = invokeParentToolCall({
      id: 'tc-c',
      args: { name: 'g', explanation: 'Third topic' },
    });
    const metaC = adapter.normalize(c, invokeParentMeta('sub-c'));
    expect(metaC?.pipeline?.groupId).toBe('invoke-tc-c');
    expect(metaC?.pipeline?.stages).toHaveLength(1);
  });

  it('aggregates successful outputs in stage order regardless of completion order', () => {
    adapter.normalize(
      invokeParentToolCall({
        id: 'tc-a',
        args: { name: 'g', explanation: 'First topic' },
      }),
      invokeParentMeta('sub-a')
    );
    adapter.normalize(
      invokeParentToolCall({
        id: 'tc-b',
        args: { name: 'g', explanation: 'Second topic' },
      }),
      invokeParentMeta('sub-b')
    );

    adapter.normalize(
      {
        type: AgentEventType.ToolCallFinished,
        id: 'tc-b',
        result: { status: 'success', output: 'answer B' },
      },
      invokeParentMeta('sub-b')
    );
    const last = {
      type: AgentEventType.ToolCallFinished,
      id: 'tc-a',
      result: { status: 'success', output: 'answer A' },
    } as ToolCallFinishedEvent;
    adapter.normalize(last, invokeParentMeta('sub-a'));

    expect(last.id).toBe('tc-a');
    expect(last.result.status).toBe('success');
    expect(extractInvokeSubagentResults(last.result.output)).toEqual([
      { name: 'First topic', status: 'success', output: 'answer A' },
      { name: 'Second topic', status: 'success', output: 'answer B' },
    ]);
  });

  it('starts a fresh group in a new turn instead of coalescing with the prior group', () => {
    adapter.beginTurn();
    adapter.normalize(
      invokeParentToolCall({ id: 'tc-a' }),
      invokeParentMeta('sub-a')
    );
    adapter.endTurn();
    adapter.beginTurn();

    const next = invokeParentToolCall({ id: 'tc-b' });
    const meta = adapter.normalize(next, invokeParentMeta('sub-b'));
    expect(next.id).toBe('tc-b');
    expect(meta?.pipeline?.groupId).toBe('invoke-tc-b');
    expect(meta?.pipeline?.stages).toHaveLength(1);
  });

  it('does not claim a new invoke after the turn admission boundary closes', () => {
    adapter.beginTurn();
    adapter.endTurn();

    const late = invokeParentToolCall({ id: 'tc-late' });
    const meta = invokeParentMeta('sub-late');
    expect(adapter.normalize(late, meta)).toBe(meta);
    expect(late.name).toBe('Sub-agent: architect');
    expect(meta.pipeline).toBeUndefined();
  });

  it('still correlates an existing member terminal after the turn ends', () => {
    adapter.beginTurn();
    adapter.normalize(
      invokeParentToolCall({ id: 'tc-a' }),
      invokeParentMeta('sub-a')
    );
    adapter.endTurn();

    const terminal = {
      type: AgentEventType.ToolCallFinished,
      id: 'tc-a',
      result: { status: 'success', output: 'answer A' },
    } as ToolCallFinishedEvent;
    const meta = adapter.normalize(terminal, undefined);
    expect(terminal.id).toBe('tc-a');
    expect(meta?.pipeline?.stages[0]?.status).toBe('completed');
  });

  it('dedupes derived stage labels within a group', () => {
    const a = invokeParentToolCall({
      id: 'tc-a',
      args: { name: 'g', explanation: 'Same label' },
    });
    const b = invokeParentToolCall({
      id: 'tc-b',
      args: { name: 'g', explanation: 'Same label' },
    });
    adapter.normalize(a, invokeParentMeta('sub-a'));
    const meta = adapter.normalize(b, invokeParentMeta('sub-b'));
    expect(meta?.pipeline?.stages.map((s) => s.name)).toEqual([
      'Same label',
      'Same label #2',
    ]);
  });

  it('preserves unrelated meta fields on the rewritten parent', () => {
    const event = invokeParentToolCall();
    const meta = adapter.normalize(event, {
      ...invokeParentMeta(),
      mcpServerName: 'srv',
    });
    expect(meta?.mcpServerName).toBe('srv');
    expect(meta?.pipeline).toBeDefined();
  });

  it('reset() drops claimed nodes and learned stage ids', () => {
    adapter.normalize(invokeParentToolCall(), invokeParentMeta());
    adapter.reset();
    const update = adapter.normalize(
      {
        type: AgentEventType.ToolCallUpdate,
        id: 'tc-1',
        content: { type: 'text' as never, text: 'x' },
      } as AgentStreamEvent,
      undefined
    );
    expect(update).toBeUndefined();
    // Its previous own-id is forgotten too: a fresh claim works again.
    const again = invokeParentToolCall();
    const meta = adapter.normalize(again, invokeParentMeta());
    expect(meta?.pipeline?.groupId).toBe('invoke-tc-1');
  });

  // Cloud relay/replay strips kind/agentSubtaskId from …-sub-agent-complete
  // events; base.ts synthesizes the parent card from those on the
  // cancel/steer paths with only messageId/timestamp meta.
  it('claims a meta-stripped Sub-agent title card (cloud cancel/steer path)', () => {
    const event = invokeParentToolCall({
      id: 'tc-stripped',
      name: 'Sub-agent: general-task-execution',
    });
    const meta = adapter.normalize(event, {
      messageId: 'invoke_subagent_x-sub-agent-complete',
    } as KiroMeta);

    expect(event.name).toBe('orchestrate_subagent');
    expect(meta?.pipeline).toEqual({
      groupId: 'invoke-tc-stripped',
      stages: [
        {
          name: 'Designing auth',
          role: 'architect',
          status: 'running',
          dependsOn: [],
          agentSubtaskId: null,
        },
      ],
    });
    // Terminal without any meta still resolves via the toolCallId registry.
    const fin = adapter.normalize(
      {
        type: AgentEventType.ToolCallFinished,
        id: 'tc-stripped',
        result: { status: 'error', error: 'cancelled' },
      } as AgentStreamEvent,
      undefined
    );
    expect(fin?.pipeline?.stages[0]?.status).toBe('failed');
  });

  it('does NOT claim a meta-stripped card that carries an agentSubtaskId (crew stage wrapper)', () => {
    const stageCard = invokeParentToolCall({
      id: 'tc-crew-wrap',
      name: 'Sub-agent: worker',
    });
    const crewMeta: KiroMeta = { agentSubtaskId: 'stage-99' };
    expect(adapter.normalize(stageCard, crewMeta)).toBe(crewMeta);
    expect(stageCard.name).toBe('Sub-agent: worker');
  });

  it('does NOT claim non-subagent titles without meta', () => {
    const plain = invokeParentToolCall({ id: 'tc-plain', name: 'web_search' });
    expect(adapter.normalize(plain, undefined)).toBeUndefined();
    expect(plain.name).toBe('web_search');
  });

  it('passes through non-tool events untouched', () => {
    const content: AgentStreamEvent = {
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: 'text' as never, text: 'hi' },
    } as AgentStreamEvent;
    const meta: KiroMeta = { agentSubtaskId: 'sub-exec-1' };
    expect(adapter.normalize(content, meta)).toBe(meta);
  });
});
