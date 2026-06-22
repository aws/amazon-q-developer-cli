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
  test('omits the last unfinished streaming model when isProcessing', () => {
    const msgs: MessageType[] = [
      user('u1'),
      model('m1', { standalone: false }),
    ];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1']);
  });

  test('includes a finished standalone model even when isProcessing', () => {
    const msgs: MessageType[] = [user('u1'), model('m1', { standalone: true })];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1', 'm1']);
  });

  test('keeps tools in the in-flight batch out of static', () => {
    const msgs: MessageType[] = [user('u1'), tool('a', false), tool('b', true)];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1']);
  });

  test('flushes tool batch once every tool is finished', () => {
    const msgs: MessageType[] = [user('u1'), tool('a', true), tool('b', true)];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a', 'b']);
  });

  test('preserves creation order on flush', () => {
    // Even if logically the agent finished `c` first, then `a`, then `b`,
    // we must render them in the order they were CREATED so the user sees
    // the same order the agent invoked them.
    const msgs: MessageType[] = [
      user('u1'),
      tool('a', true),
      tool('b', true),
      tool('c', true),
    ];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a', 'b', 'c']);
  });

  test('deduplicates tool messages by id', () => {
    const msgs: MessageType[] = [user('u1'), tool('a', true), tool('a', true)];
    const out = selectStaticEligible(msgs, false);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a']);
  });

  test('drops empty Model messages between tools', () => {
    // flushContentToStore pushes a Model with content='' when only Thought
    // events arrived. The empty Model would otherwise add 3 blank rows
    // between neighboring tools (own \n prefix splits to 2 blanks, plus the
    // next tool's \n prefix adds one more). Eligibility skips it so the
    // tool→tool boundary stays compact.
    const msgs: MessageType[] = [
      user('u1'),
      tool('a', true),
      model('empty', { content: '' }),
      tool('b', true),
    ];
    const out = selectStaticEligible(msgs, false);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a', 'b']);
  });

  test('drops whitespace-only Model messages', () => {
    const msgs: MessageType[] = [
      user('u1'),
      model('blank', { content: '   \n\t' }),
      tool('a', true),
    ];
    const out = selectStaticEligible(msgs, false);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a']);
  });

  test('keeps empty-content Model when thinking is set (default — block renders)', () => {
    // Default behavior: an empty-content Model with populated thinking
    // stays in eligible so renderMessageToText can emit the bordered
    // thinking block. This is the carve-out the fix below depends on
    // having a complementary toggle for.
    const msgs: MessageType[] = [
      user('u1'),
      model('m1', { content: '', thinking: 'reasoning payload' }),
      tool('a', true),
    ];
    const out = selectStaticEligible(msgs, false);
    expect(out.map((m) => m.id)).toEqual(['u1', 'm1', 'a']);
  });

  test('drops empty-content Model with thinking when hideThinkingContent is true', () => {
    // Minimal preset (showThinkingContent: false) bug fix. Without this
    // gate, the empty-content row stayed in eligible, rendered to '' via
    // renderMessageToText (since the thinking block is suppressed), and
    // the LiteLayout delta walk baked a '\n' leading-blank prefix on top
    // of the empty render — pinning a 2-row gap into <Static>. The next
    // message also computed its own leading blank against this empty
    // prevMsg, totaling 3 phantom blank rows where the agent's
    // Thought-only round used to be. Append-only contract means those
    // ghost rows persisted until LiteLayout remounted.
    //
    // Reproduces every time the agent thinks silently then directly
    // calls a tool — common enough to be visible in normal use.
    const msgs: MessageType[] = [
      user('u1'),
      tool('a', true),
      model('m1', { content: '', thinking: 'reasoning payload' }),
      user('u2'),
    ];
    const out = selectStaticEligible(msgs, false, undefined, 'main', true);
    expect(out.map((m) => m.id)).toEqual(['u1', 'a', 'u2']);
  });

  test('hideThinkingContent does not drop Model rows that have actual content', () => {
    // Belt-and-suspenders: the new gate is scoped to empty-content rows
    // only. A Model with both content and thinking should still appear
    // in eligible regardless of the toggle — only the persisted thinking
    // block inside renderMessageToText is suppressed by the toggle, the
    // spoken text still renders.
    const msgs: MessageType[] = [
      user('u1'),
      model('m1', { content: 'hello', thinking: 'reasoning' }),
    ];
    const out = selectStaticEligible(msgs, false, undefined, 'main', true);
    expect(out.map((m) => m.id)).toEqual(['u1', 'm1']);
  });

  test('shellOutput Model holds out of static while in flight', () => {
    // The shell-escape branch in app-store seeds an empty Model with
    // `shellOutput: true` then streams PTY chunks into its content. While
    // the command is running (`isProcessing: true`), the row is the trailing
    // non-standalone Model, so the existing skip rule keeps it out of
    // <Static>. The live region paints it via `renderShellOutputBlock`
    // until the PTY exits — at which point isProcessing flips to false
    // and the row falls back into eligible (verified in the next test).
    const shellRow: MessageType = {
      id: 'shell-out',
      role: MessageRole.Model,
      content: 'Enter PIN:',
    };
    // shellOutput is intentionally not on the MessageType union here
    // (the field lives in app-store's interleaved type extension); the
    // skip rule keys on `standalone`, not on `shellOutput`, so the
    // existing logic covers it without further change.
    const msgs: MessageType[] = [user('u1'), shellRow];
    const out = selectStaticEligible(msgs, true);
    expect(out.map((m) => m.id)).toEqual(['u1']);
  });

  test('shellOutput Model commits to static once the command exits', () => {
    // Mirror of the above: when isProcessing flips to false (command
    // exited cleanly or was cancelled), the trailing-Model skip no longer
    // fires and the row joins the eligible set. This is what makes the
    // live → static transition a no-op visual change — the same body
    // renders via the same helper, just on a different surface.
    const shellRow: MessageType = {
      id: 'shell-out',
      role: MessageRole.Model,
      content: 'Got: 1234\n[exit code: 0]',
    };
    const msgs: MessageType[] = [user('u1'), shellRow];
    const out = selectStaticEligible(msgs, false);
    expect(out.map((m) => m.id)).toEqual(['u1', 'shell-out']);
  });

  test('shellOutput Model with empty content stays out of static after cancel', () => {
    // The trigger for the React duplicate-key bug we fixed in app-store:
    // user types `!sleep 30`, hits Ctrl+C before any PTY output. The
    // store no longer inserts a `(no output)` placeholder, so the row's
    // content stays empty and the empty-Model filter drops it from
    // eligible. The reason it MUST be filtered: the cancelArmedRef
    // effect appends a `user interrupted` System row right after
    // isProcessing flips false — if the empty Model became eligible
    // mid-cancel, it would shift the System row's index in the eligible
    // list AFTER the System had already been pushed to <Static>, and
    // the LiteLayout staticItems delta-walk would push the System row
    // a second time. Verified end-to-end via Knight Rider; this test
    // pins the eligibility-side invariant the fix relies on.
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
  test('isInnerSubagentTool flags ToolUse with non-main agentName', () => {
    expect(
      isInnerSubagentTool(tool('t', false, { agentName: 'sub-1' }), 'main')
    ).toBe(true);
  });

  test('isInnerSubagentTool false for main-agent tools and missing names', () => {
    expect(
      isInnerSubagentTool(tool('t', false, { agentName: 'main' }), 'main')
    ).toBe(false);
    expect(isInnerSubagentTool(tool('t', false), 'main')).toBe(false);
    expect(isInnerSubagentTool(user('u'), 'main')).toBe(false);
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
