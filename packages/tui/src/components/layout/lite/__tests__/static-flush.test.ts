import { describe, test, expect } from 'vitest';
import {
  computeActiveToolBatchIds,
  formatTurnSummaryRow,
  isInnerSubagentTool,
  needsLeadingBlank,
  selectStaticEligible,
} from '../static-flush.js';
import { MessageRole, type MessageType } from '../../../../stores/app-store.js';

function tool(
  id: string,
  finished: boolean,
  opts: { agentName?: string; name?: string } = {}
): MessageType {
  return {
    id,
    role: MessageRole.ToolUse,
    name: opts.name ?? id,
    content: '',
    isFinished: finished,
    ...(opts.agentName ? { agentName: opts.agentName } : {}),
  };
}

function user(id: string): MessageType {
  return { id, role: MessageRole.User, content: 'q' };
}

function model(
  id: string,
  opts: { standalone?: boolean; content?: string; thinking?: string } = {}
): MessageType {
  return {
    id,
    role: MessageRole.Model,
    content: opts.content ?? 'a',
    standalone: opts.standalone,
    ...(opts.thinking !== undefined ? { thinking: opts.thinking } : {}),
  } as MessageType;
}

describe('computeActiveToolBatchIds', () => {
  // Prefix-flush semantics: tools finished BEFORE the first unfinished tool
  // flush to static immediately; only the suffix starting at the first
  // unfinished tool stays live (preserving creation order when later parallel
  // tools settle out of order). A fully-settled trailing run holds back
  // nothing, and a settled batch separated by a Model message is never folded
  // into the current run.
  const batchCases: Array<{
    name: string;
    msgs: MessageType[];
    expectedIds: string[];
  }> = [
    {
      name: 'empty when no trailing tool run',
      msgs: [user('u1'), model('m1')],
      expectedIds: [],
    },
    {
      name: 'empty when trailing tool run is fully settled',
      msgs: [user('u1'), tool('a', true), tool('b', true)],
      expectedIds: [],
    },
    {
      name: 'captures the suffix from the first unfinished tool onward',
      msgs: [user('u1'), tool('a', true), tool('b', false), tool('c', true)],
      expectedIds: ['b', 'c'],
    },
    {
      name: 'all-finished trailing run is fully flushed (no batch)',
      msgs: [user('u1'), tool('a', true), tool('b', true), tool('c', true)],
      expectedIds: [],
    },
    {
      name: 'does not include earlier settled batch separated by a model message',
      msgs: [
        user('u1'),
        tool('past', true),
        model('m1'),
        tool('current_a', false),
        tool('current_b', true),
      ],
      expectedIds: ['current_a', 'current_b'],
    },
  ];
  test.each(batchCases)('$name', ({ msgs, expectedIds }) => {
    const ids = computeActiveToolBatchIds(msgs);
    expect([...ids].sort()).toEqual([...expectedIds].sort());
  });
});

describe('selectStaticEligible', () => {
  // The shell-escape branch in app-store seeds an empty Model with
  // `shellOutput: true` then streams PTY chunks into its content. The skip
  // rule keys on `standalone`, not `shellOutput`, so a trailing non-standalone
  // Model (shell or plain) holds out of <Static> while isProcessing and falls
  // back into eligible once it flips false — making the live→static transition
  // a no-op visual change (same helper, different surface). Each distinct
  // eligibility rule is one row; the trailing args mirror the real signature
  // `(msgs, isProcessing, _opts, agentName, hideThinkingContent)`.
  const shell = (id: string, content: string): MessageType =>
    ({ id, role: MessageRole.Model, content }) as MessageType;
  const eligibleCases: Array<{
    name: string;
    msgs: MessageType[];
    isProcessing: boolean;
    extra?: [undefined, string, boolean];
    expectedIds: string[];
  }> = [
    {
      name: 'omits the last unfinished streaming model when isProcessing',
      msgs: [user('u1'), model('m1', { standalone: false })],
      isProcessing: true,
      expectedIds: ['u1'],
    },
    {
      name: 'includes a finished standalone model even when isProcessing',
      msgs: [user('u1'), model('m1', { standalone: true })],
      isProcessing: true,
      expectedIds: ['u1', 'm1'],
    },
    {
      name: 'keeps tools in the in-flight batch out of static',
      msgs: [user('u1'), tool('a', false), tool('b', true)],
      isProcessing: true,
      expectedIds: ['u1'],
    },
    {
      name: 'flushes tool batch once every tool is finished',
      msgs: [user('u1'), tool('a', true), tool('b', true)],
      isProcessing: true,
      expectedIds: ['u1', 'a', 'b'],
    },
    {
      // Even if the agent finished `c` first, then `a`, then `b`, we render in
      // CREATION order so the user sees the order the agent invoked them.
      name: 'preserves creation order on flush',
      msgs: [user('u1'), tool('a', true), tool('b', true), tool('c', true)],
      isProcessing: true,
      expectedIds: ['u1', 'a', 'b', 'c'],
    },
    {
      name: 'deduplicates tool messages by id',
      msgs: [user('u1'), tool('a', true), tool('a', true)],
      isProcessing: false,
      expectedIds: ['u1', 'a'],
    },
    {
      // flushContentToStore pushes a content='' Model when only Thought events
      // arrived; it would otherwise add 3 blank rows at the tool→tool boundary
      // (own \n splits to 2 blanks + next tool's \n adds 1). Skip keeps it tight.
      name: 'drops empty Model messages between tools',
      msgs: [
        user('u1'),
        tool('a', true),
        model('empty', { content: '' }),
        tool('b', true),
      ],
      isProcessing: false,
      expectedIds: ['u1', 'a', 'b'],
    },
    {
      name: 'drops whitespace-only Model messages',
      msgs: [
        user('u1'),
        model('blank', { content: '   \n\t' }),
        tool('a', true),
      ],
      isProcessing: false,
      expectedIds: ['u1', 'a'],
    },
    {
      // Default: an empty-content Model with populated thinking stays eligible
      // so renderMessageToText can emit the bordered thinking block. This is
      // the carve-out the hideThinkingContent gate below complements.
      name: 'keeps empty-content Model when thinking is set (default block renders)',
      msgs: [
        user('u1'),
        model('m1', { content: '', thinking: 'reasoning payload' }),
        tool('a', true),
      ],
      isProcessing: false,
      expectedIds: ['u1', 'm1', 'a'],
    },
    {
      // Minimal-preset (showThinkingContent: false) bug fix. Without this gate
      // the empty-content row stayed eligible, rendered to '' (thinking block
      // suppressed), and the delta walk baked leading-blank prefixes into
      // <Static> — pinning phantom blank rows where a silent-think-then-tool
      // round used to be (append-only, so they persisted until remount).
      name: 'drops empty-content Model with thinking when hideThinkingContent is true',
      msgs: [
        user('u1'),
        tool('a', true),
        model('m1', { content: '', thinking: 'reasoning payload' }),
        user('u2'),
      ],
      isProcessing: false,
      extra: [undefined, 'main', true],
      expectedIds: ['u1', 'a', 'u2'],
    },
    {
      // Belt-and-suspenders: the gate is scoped to empty-content rows only. A
      // Model with both content and thinking still appears regardless of the
      // toggle — only the persisted thinking block inside renderMessageToText
      // is suppressed; the spoken text still renders.
      name: 'hideThinkingContent does not drop Model rows that have actual content',
      msgs: [
        user('u1'),
        model('m1', { content: 'hello', thinking: 'reasoning' }),
      ],
      isProcessing: false,
      extra: [undefined, 'main', true],
      expectedIds: ['u1', 'm1'],
    },
    {
      name: 'shellOutput Model holds out of static while in flight',
      msgs: [user('u1'), shell('shell-out', 'Enter PIN:')],
      isProcessing: true,
      expectedIds: ['u1'],
    },
    {
      name: 'shellOutput Model commits to static once the command exits',
      msgs: [user('u1'), shell('shell-out', 'Got: 1234\n[exit code: 0]')],
      isProcessing: false,
      expectedIds: ['u1', 'shell-out'],
    },
  ];
  test.each(eligibleCases)(
    '$name',
    ({ msgs, isProcessing, extra, expectedIds }) => {
      const out = extra
        ? selectStaticEligible(msgs, isProcessing, ...extra)
        : selectStaticEligible(msgs, isProcessing);
      expect(out.map((m) => m.id)).toEqual(expectedIds);
    }
  );

  test('shellOutput Model with empty content stays out of static after cancel', () => {
    // Trigger for the React duplicate-key bug fixed in app-store: `!sleep 30`
    // then Ctrl+C before any PTY output leaves an empty-content shell Model.
    // It MUST stay filtered — the cancelArmedRef effect pushes a `user
    // interrupted` System row once isProcessing flips false; an empty Model
    // turning eligible mid-cancel would shift indices and make the delta walk
    // double-push the System row. Pins the eligibility-side invariant.
    const shellRow: MessageType = {
      id: 'shell-out-empty',
      role: MessageRole.Model,
      content: '',
    };
    const msgs: MessageType[] = [user('u1'), shellRow];
    const outProcessing = selectStaticEligible(msgs, true);
    expect(outProcessing.map((m) => m.id)).toEqual(['u1']);
    const outIdle = selectStaticEligible(msgs, false);
    expect(outIdle.map((m) => m.id)).toEqual(['u1']);
  });
});

describe('inner subagent filtering', () => {
  // Only a ToolUse whose agentName differs from the active agent is "inner";
  // main-agent tools, agentName-less tools, and non-tool rows are not.
  test.each<[string, MessageType, boolean]>([
    ['non-main agentName', tool('t', false, { agentName: 'sub-1' }), true],
    ['main agentName', tool('t', false, { agentName: 'main' }), false],
    ['missing agentName', tool('t', false), false],
    ['non-tool row', user('u'), false],
  ])('isInnerSubagentTool: %s → %s', (_name, msg, expected) => {
    expect(isInnerSubagentTool(msg, 'main')).toBe(expected);
  });

  test('computeActiveToolBatchIds skips inner subagent tools when computing the trailing batch', () => {
    // Parent subagent (main agent) is unfinished + a child tool from the
    // subagent's own session lands at the tail. The batch should track only
    // the parent subagent itself.
    const ids = computeActiveToolBatchIds(
      [
        user('u1'),
        tool('parent_subagent', false, { agentName: 'main', name: 'subagent' }),
        tool('child_read', false, { agentName: 'sub-1', name: 'read' }),
      ],
      'main'
    );
    expect([...ids]).toEqual(['parent_subagent']);
  });

  test('selectStaticEligible filters inner subagent tools from static even when finished', () => {
    const msgs: MessageType[] = [
      user('u1'),
      tool('parent_subagent', true, { agentName: 'main', name: 'subagent' }),
      tool('child_read', true, { agentName: 'sub-1', name: 'read' }),
      tool('child_summary', true, { agentName: 'sub-1', name: 'summary' }),
    ];
    const out = selectStaticEligible(msgs, false, undefined, 'main');
    expect(out.map((m) => m.id)).toEqual(['u1', 'parent_subagent']);
  });
});

// Locks in the lite line-break spec (no configurability):
//   1. Blank BEFORE + AFTER every user message.
//   2. NO blank between consecutive tool calls in the same turn.
//   3. Blank between the last tool call and the agent's text response.
//   4. System rows behave like conversational rows (blank on both sides)
//      except system→system, which stays compact.
//   5. Blank BEFORE the credits/time trailer (formatTurnSummaryRow).
// A failure means the rule regressed — adjust needsLeadingBlank /
// formatTurnSummaryRow, not the table.
describe('needsLeadingBlank — section-boundary rules', () => {
  function system(id: string): MessageType {
    return { id, role: MessageRole.System, content: 's', success: true };
  }
  const make: Record<string, (id: string) => MessageType> = {
    user,
    model,
    tool: (id) => tool(id, true),
    system,
  };
  test.each([
    ['user', 'tool', true],
    ['user', 'model', true],
    ['user', 'system', true],
    ['model', 'tool', true],
    ['model', 'user', true],
    ['model', 'system', true],
    ['tool', 'model', true],
    ['tool', 'user', true],
    ['tool', 'system', true],
    ['tool', 'tool', false], // consecutive tools stay compact
    ['system', 'user', true],
    ['system', 'model', true],
    ['system', 'tool', true],
    ['system', 'system', false], // consecutive system rows stay compact
  ] as const)('%s → %s: blank=%s', (prev, next, expected) => {
    expect(needsLeadingBlank(make[prev]!('p'), make[next]!('n'))).toBe(
      expected
    );
  });

  test('rule 5: trailer text starts with a blank line', () => {
    // formatTurnSummaryRow guarantees the leading blank — even if the
    // upstream renderer changes its color/indent. Locks the rule against
    // future tweaks to the trailer's appearance.
    const out = formatTurnSummaryRow('  Credits: 0.05 · Time: 3s');
    expect(out.startsWith('\n')).toBe(true);
    expect(out).toBe('\n  Credits: 0.05 · Time: 3s');
  });
});
