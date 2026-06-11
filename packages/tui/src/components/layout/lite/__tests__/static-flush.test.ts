import { describe, test, expect } from 'vitest';
import {
  computeActiveToolBatchIds,
  computeTurnSummaryInsertions,
  formatTurnSummaryRow,
  isInnerSubagentTool,
  needsLeadingBlank,
  selectStaticEligible,
  shouldPushSwapWithContentBanner,
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
  test('empty when no trailing tool run', () => {
    const ids = computeActiveToolBatchIds([user('u1'), model('m1')]);
    expect(ids.size).toBe(0);
  });

  test('empty when trailing tool run is fully settled', () => {
    const ids = computeActiveToolBatchIds([
      user('u1'),
      tool('a', true),
      tool('b', true),
    ]);
    expect(ids.size).toBe(0);
  });

  test('captures the suffix from the first unfinished tool onward', () => {
    // Prefix-flush semantics: tools finished BEFORE the first unfinished
    // tool flush to static immediately. Only the suffix starting at the
    // first unfinished tool stays in the live region (so creation order
    // is preserved in scrollback when later parallel tools complete out
    // of order).
    const ids = computeActiveToolBatchIds([
      user('u1'),
      tool('a', true),
      tool('b', false),
      tool('c', true),
    ]);
    expect([...ids].sort()).toEqual(['b', 'c']);
  });

  test('all-finished trailing run is fully flushed (no batch)', () => {
    // No unfinished tool → nothing held back from static.
    const ids = computeActiveToolBatchIds([
      user('u1'),
      tool('a', true),
      tool('b', true),
      tool('c', true),
    ]);
    expect(ids.size).toBe(0);
  });

  test('does not include earlier settled batch separated by a model message', () => {
    const ids = computeActiveToolBatchIds([
      user('u1'),
      tool('past', true),
      model('m1'),
      tool('current_a', false),
      tool('current_b', true),
    ]);
    expect([...ids].sort()).toEqual(['current_a', 'current_b']);
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

describe('needsLeadingBlank', () => {
  test('user → tool: blank', () => {
    expect(needsLeadingBlank(user('u'), tool('t', true))).toBe(true);
  });
  test('user → model: blank', () => {
    expect(needsLeadingBlank(user('u'), model('m'))).toBe(true);
  });
  test('model → tool: blank', () => {
    expect(needsLeadingBlank(model('m'), tool('t', true))).toBe(true);
  });
  test('tool → model: blank', () => {
    expect(needsLeadingBlank(tool('t', true), model('m'))).toBe(true);
  });
  test('tool → user: blank', () => {
    expect(needsLeadingBlank(tool('t', true), user('u'))).toBe(true);
  });
  test('tool → tool: compact (no blank)', () => {
    expect(needsLeadingBlank(tool('a', true), tool('b', true))).toBe(false);
  });
  test('model → user: blank', () => {
    // Pure conversational turn boundary: without a blank, the next user
    // line would print flush against the previous model reply. The input
    // divider sits above the prompt area, not between scrollback rows.
    expect(needsLeadingBlank(model('m'), user('u'))).toBe(true);
  });
});

describe('needsLeadingBlank — system messages', () => {
  function system(id: string): MessageType {
    return { id, role: MessageRole.System, content: 's', success: true };
  }
  test('user → system: blank', () => {
    expect(needsLeadingBlank(user('u'), system('s'))).toBe(true);
  });
  test('model → system: blank', () => {
    expect(needsLeadingBlank(model('m'), system('s'))).toBe(true);
  });
  test('tool → system: blank', () => {
    expect(needsLeadingBlank(tool('t', true), system('s'))).toBe(true);
  });
  test('system → user: blank', () => {
    expect(needsLeadingBlank(system('s'), user('u'))).toBe(true);
  });
  test('system → model: blank', () => {
    expect(needsLeadingBlank(system('s'), model('m'))).toBe(true);
  });
  test('system → tool: blank', () => {
    expect(needsLeadingBlank(system('s'), tool('t', true))).toBe(true);
  });
  test('system → system: compact', () => {
    expect(needsLeadingBlank(system('s1'), system('s2'))).toBe(false);
  });
});

describe('computeTurnSummaryInsertions', () => {
  function system(id: string): MessageType {
    return { id, role: MessageRole.System, content: 's', success: true };
  }

  test('returns empty when no turn summaries exist', () => {
    const eligible = [user('u1'), model('m1')];
    const insertions = computeTurnSummaryInsertions(eligible, new Map(), false);
    expect(insertions.size).toBe(0);
  });

  test('idle turn with summary: tail-emits at eligible.length', () => {
    const eligible = [user('u1'), model('m1')];
    const summaries = new Map([['u1', 'Credits: 0.05 · Time: 3s']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(2);
  });

  test('processing turn: no tail emission while in flight', () => {
    const eligible = [user('u1'), model('m1')];
    const summaries = new Map([['u1', 'Credits: 0.05 · Time: 3s']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, true);
    expect(insertions.size).toBe(0);
  });

  test('next-turn boundary: summary slots BEFORE the new user message', () => {
    // Boundary case — new user turn closes the previous one. Without this,
    // turn 1's summary would tail-append after turn 2's content.
    const eligible = [user('u1'), model('m1'), user('u2'), model('m2')];
    const summaries = new Map([['u1', 'sum1']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(2); // before u2
    expect(insertions.size).toBe(1);
  });

  test('System after a finished turn: trailer locks BEFORE the System line', () => {
    // Bug A reproduction. The trailer must land between the model reply and
    // the slash-command's System announcement; otherwise Twinki's monotonic
    // <Static> cursor re-emits it on every additional System message.
    const eligible = [user('u1'), model('m1'), system('s1')];
    const summaries = new Map([['u1', 'sum1']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(2); // before s1
  });

  test('multiple System messages after a turn: trailer locks at the FIRST System', () => {
    // /verbose status run repeatedly: the trailer index must not move with
    // each new System line, otherwise the cursor re-emits.
    const eligible = [
      user('u1'),
      model('m1'),
      system('s1'),
      system('s2'),
      system('s3'),
    ];
    const summaries = new Map([['u1', 'sum1']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(2);
    expect(insertions.size).toBe(1);
  });

  test('boot-time System with no preceding user: no insertion', () => {
    const eligible = [system('s_boot'), user('u1'), model('m1')];
    const summaries = new Map([['u1', 'sum1']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    // The leading System gets no summary (no current turn yet); u1's
    // summary tail-emits since no later non-turn message comes after.
    expect(insertions.size).toBe(1);
    expect(insertions.get('u1')).toBe(eligible.length);
  });

  test('multi-turn with mixed System messages between turns', () => {
    const eligible = [
      user('u1'),
      model('m1'),
      system('s1'),
      user('u2'),
      model('m2'),
      system('s2'),
    ];
    const summaries = new Map([
      ['u1', 'sum1'],
      ['u2', 'sum2'],
    ]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(2); // before s1
    expect(insertions.get('u2')).toBe(5); // before s2
  });

  test('turn ending in a tool batch: trailer follows the last tool', () => {
    const eligible = [
      user('u1'),
      tool('t1', true),
      tool('t2', true),
      system('s1'),
    ];
    const summaries = new Map([['u1', 'sum1']]);
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.get('u1')).toBe(3); // before s1
  });

  test('turn without summary entry: nothing inserted', () => {
    const eligible = [user('u1'), model('m1'), system('s1')];
    const summaries = new Map<string, string>(); // u1 has no summary
    const insertions = computeTurnSummaryInsertions(eligible, summaries, false);
    expect(insertions.size).toBe(0);
  });
});

describe('lite line-break rules (user spec)', () => {
  // The user's hard-coded spec (no configurability):
  //   1. Blank BEFORE every user message.
  //   2. Blank AFTER every user message.
  //   3. NO blank between consecutive tool calls in the same turn.
  //   4. Blank between the LAST tool call and the agent's text response.
  //   5. Blank BEFORE the credits/time trailer.
  // Each test below names the rule it locks in. If one fails, the rule
  // regressed — adjust needsLeadingBlank / formatTurnSummaryRow, not the
  // assertion.
  function system(id: string): MessageType {
    return { id, role: MessageRole.System, content: 's', success: true };
  }

  test('rule 1: blank BEFORE user — model → user', () => {
    expect(needsLeadingBlank(model('m'), user('u'))).toBe(true);
  });
  test('rule 1: blank BEFORE user — tool → user', () => {
    expect(needsLeadingBlank(tool('t', true), user('u'))).toBe(true);
  });
  test('rule 1: blank BEFORE user — system → user (slash command then prompt)', () => {
    expect(needsLeadingBlank(system('s'), user('u'))).toBe(true);
  });
  test('rule 2: blank AFTER user — user → model', () => {
    expect(needsLeadingBlank(user('u'), model('m'))).toBe(true);
  });
  test('rule 2: blank AFTER user — user → tool (first tool of turn)', () => {
    expect(needsLeadingBlank(user('u'), tool('t', true))).toBe(true);
  });
  test('rule 2: blank AFTER user — user → system', () => {
    expect(needsLeadingBlank(user('u'), system('s'))).toBe(true);
  });
  test('rule 3: NO blank between consecutive tool calls', () => {
    expect(needsLeadingBlank(tool('a', true), tool('b', true))).toBe(false);
  });
  test('rule 4: blank between last tool and agent text', () => {
    expect(needsLeadingBlank(tool('t', true), model('m'))).toBe(true);
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

describe('shouldPushSwapWithContentBanner', () => {
  // Cold-boot path: no prior static, no prior chat. The live-region
  // banner above the divider owns the welcome screen — the static
  // session-anchor banner must NOT fire here, otherwise resize would
  // see two banner emissions in the same render.
  test('returns false on a truly fresh session (no prior static, no User)', () => {
    expect(shouldPushSwapWithContentBanner([], false)).toBe(false);
    // Standalone agent greeting alone is not chat content — same rule as
    // the welcome-screen suppression in LiteLayout's visibleMessages
    // filter; a session with only the agent's "Hi, I'm planner..." row
    // is still considered fresh for banner-anchor purposes.
    expect(
      shouldPushSwapWithContentBanner(
        [model('m1', { standalone: true })],
        false
      )
    ).toBe(false);
  });

  // tui→lite swap mid-session and /chat <id> load: messages list
  // carries the prior chat at the moment of the bump. User arm fires.
  test('returns true when messages contain a User row (tui→lite, /chat <id>)', () => {
    expect(shouldPushSwapWithContentBanner([user('u1')], false)).toBe(true);
    expect(
      shouldPushSwapWithContentBanner([user('u1'), model('m1')], false)
    ).toBe(true);
  });

  // The bug fix: /chat new mid-session. resetMessages empties messages
  // BEFORE bumping the clear token, so messages.some(User) is false
  // here — but the layout had committed rows to staticItemsRef in
  // the prior session, so hadPriorStaticContent is true. Without
  // this arm the live-region banner re-renders against a terminal
  // scrollback that still holds the prior session's static rows
  // above it, and the user sees the KIRO art twice on screen.
  test('returns true when prior session committed static content (/chat new bug-fix path)', () => {
    expect(shouldPushSwapWithContentBanner([], true)).toBe(true);
    // Both arms true is also true (defensive — covers a future caller
    // that for some reason has both signals available).
    expect(shouldPushSwapWithContentBanner([user('u1')], true)).toBe(true);
  });

  // System rows alone (e.g. "Switched to TUI mode" announcement landing
  // on an empty session) must NOT trigger the static banner — the
  // integ test `lite-welcome-banner-roundtrip` documents this. The
  // User arm is intentionally narrow.
  test('returns false when messages contain only System / non-User rows', () => {
    const sys: MessageType = {
      id: 's1',
      role: MessageRole.System,
      content: 'Switched to TUI mode',
      success: true,
    };
    expect(shouldPushSwapWithContentBanner([sys], false)).toBe(false);
    expect(
      shouldPushSwapWithContentBanner(
        [sys, model('m1', { standalone: true })],
        false
      )
    ).toBe(false);
  });
});
