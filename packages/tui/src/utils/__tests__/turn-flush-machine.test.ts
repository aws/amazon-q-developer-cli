import { describe, it, expect } from 'bun:test';
import { computeFlushSet, MAX_TAIL_SIZE } from '../turn-flush-machine.js';
import { MessageRole } from '../../stores/app-store.js';

const user = (id: string) => ({ id, role: MessageRole.User });
const tool = (id: string, isFinished: boolean) => ({
  id,
  role: MessageRole.ToolUse,
  isFinished,
});
const model = (id: string) => ({
  id,
  role: MessageRole.Model,
  content: 'text',
});

const TAIL = 2;

/** Helper: how many messages remain in the tail */
function tailLen(
  msgs: ReturnType<typeof user | typeof tool | typeof model>[],
  isProcessing: boolean,
  tailSize: number
) {
  return msgs.length - computeFlushSet(msgs, isProcessing, tailSize).size;
}

describe('computeFlushSet', () => {
  // --- Basic invariants ---
  it('returns empty set when no messages', () => {
    expect(computeFlushSet([], true, TAIL).size).toBe(0);
  });

  it('never flushes more than available', () => {
    expect(computeFlushSet([user('u')], true, 5).size).toBe(0);
  });

  it('returns empty when total <= tailSize', () => {
    expect(
      computeFlushSet([user('u'), tool('t1', true)], true, TAIL).size
    ).toBe(0);
  });

  it('respects tailSize=0 — flushes everything done', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    expect(computeFlushSet(msgs, false, 0).size).toBe(3);
  });

  // --- User message ---
  it('user message is always done', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', true),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  // --- Tool use ---
  it('finished tool is done', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  it('unfinished tool blocks FIFO but stays in tail', () => {
    const msgs = [user('u'), tool('t1', false), tool('t2', true), model('m1')];
    // FIFO: doneCount=1, flush=min(1,4-2)=1
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(1);
  });

  it('returns empty when unfinished tool is first ai message and total <= tailSize+1', () => {
    const msgs = [user('u'), tool('t1', false)];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(0);
  });

  // --- Model message ---
  it('does not flush streaming model message (last + isProcessing)', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(1);
  });

  it('flushes model message when not last (something follows it)', () => {
    const msgs = [user('u'), model('m1'), tool('t1', true), tool('t2', false)];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  it('flushes model message when isProcessing=false', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    expect(computeFlushSet(msgs, false, TAIL).size).toBe(1);
  });

  // --- Long runs ---
  it('long run: 10 finished tools + final model, processing done', () => {
    const msgs = [
      user('u'),
      ...Array.from({ length: 10 }, (_, i) => tool(`t${i}`, true)),
      model('m1'),
    ];
    // 12 msgs, all done, flush 12-2=10
    expect(computeFlushSet(msgs, false, TAIL).size).toBe(10);
  });

  it('long run: 10 finished tools + 1 running, still processing', () => {
    const msgs = [
      user('u'),
      ...Array.from({ length: 10 }, (_, i) => tool(`t${i}`, true)),
      tool('t10', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(10);
  });

  // --- LLM text in the middle ---
  it('model message between two tool uses flushes when second tool arrives', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      model('m1'),
      tool('t2', true),
      tool('t3', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(3);
  });

  it('model message at end stays in tail while processing', () => {
    const msgs = [user('u'), tool('t1', true), tool('t2', true), model('m1')];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  it('model message at end flushes when turn completes', () => {
    const msgs = [user('u'), tool('t1', true), tool('t2', true), model('m1')];
    expect(computeFlushSet(msgs, false, TAIL).size).toBe(2);
  });

  // --- Parallel tool calls ---
  it('parallel tools: two unfinished tools — flushes only settled prefix', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', false),
      tool('t3', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  it('parallel tools: first two finish, last two still running', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', false),
      tool('t4', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(3);
  });

  it('parallel tools: all finish', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', true),
      tool('t4', true),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(3);
  });

  // --- Turn completion ---
  it('completed turn with no tool uses flushes user+models', () => {
    const msgs = [user('u'), model('m1'), model('m2'), model('m3')];
    expect(computeFlushSet(msgs, false, TAIL).size).toBe(2);
  });

  it('single message turn never flushes', () => {
    expect(computeFlushSet([user('u')], false, TAIL).size).toBe(0);
  });

  it('exactly tailSize messages never flushes', () => {
    expect(computeFlushSet([user('u'), model('m1')], false, TAIL).size).toBe(0);
  });

  // --- Complex permutations ---
  it('user → model → tool(done) → model → tool(running): flushes up to first model', () => {
    const msgs = [
      user('u'),
      model('m1'),
      tool('t1', true),
      model('m2'),
      tool('t2', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(3);
  });

  it('many finished tools before active: flushes all before active tool', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', true),
      tool('t4', true),
      tool('t5', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(4);
  });

  it('active tool with model before it', () => {
    const msgs = [user('u'), tool('t1', true), model('m1'), tool('t2', false)];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(2);
  });

  it('active tool is second message: only user before it, tailSize keeps it in tail', () => {
    const msgs = [user('u'), tool('t1', false)];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(0);
  });

  it('large tail with active tool: flushes everything before active tool respecting tailSize', () => {
    const msgs = [
      user('u'),
      ...Array.from({ length: 8 }, (_, i) => tool(`t${i + 1}`, true)),
      tool('t9', false),
    ];
    expect(computeFlushSet(msgs, true, TAIL).size).toBe(8);
  });

  // --- MAX_TAIL_SIZE guardrail ---
  describe('MAX_TAIL_SIZE guardrail', () => {
    it('does not activate when tail <= MAX_TAIL_SIZE', () => {
      const msgs = [
        user('u'),
        tool('active', false),
        ...Array.from({ length: MAX_TAIL_SIZE - 2 }, (_, i) =>
          tool(`t${i}`, true)
        ),
      ];
      // total = MAX_TAIL_SIZE, doneCount=1, flush=min(1, total-2)
      // tail is small, no guardrail
      const set = computeFlushSet(msgs, true, TAIL);
      expect(tailLen(msgs, true, TAIL)).toBeLessThanOrEqual(MAX_TAIL_SIZE);
      // active tool should NOT be flushed
      expect(set.has('active')).toBe(false);
    });

    it('flushes finished messages past active tool when tail exceeds MAX_TAIL_SIZE', () => {
      // active tool early, many finished tools after it
      const msgs = [
        user('u'),
        tool('active', false),
        ...Array.from({ length: MAX_TAIL_SIZE + 2 }, (_, i) =>
          tool(`t${i}`, true)
        ),
      ];
      const set = computeFlushSet(msgs, true, TAIL);
      // Active tool must stay in tail
      expect(set.has('active')).toBe(false);
      // Tail should be capped to MAX_TAIL_SIZE
      expect(tailLen(msgs, true, TAIL)).toBe(MAX_TAIL_SIZE);
      // Some finished tools after the active one got flushed
      expect(set.size).toBeGreaterThan(1);
    });

    it('active tools are never flushed even under guardrail', () => {
      // Two active tools + many finished
      const msgs = [
        user('u'),
        tool('a1', false),
        tool('a2', false),
        ...Array.from({ length: MAX_TAIL_SIZE + 2 }, (_, i) =>
          tool(`t${i}`, true)
        ),
      ];
      const set = computeFlushSet(msgs, true, TAIL);
      expect(set.has('a1')).toBe(false);
      expect(set.has('a2')).toBe(false);
    });

    it('streaming model at end is never flushed', () => {
      const msgs = [
        user('u'),
        tool('active', false),
        ...Array.from({ length: MAX_TAIL_SIZE }, (_, i) => tool(`t${i}`, true)),
        model('streaming'),
      ];
      const set = computeFlushSet(msgs, true, TAIL);
      expect(set.has('active')).toBe(false);
      expect(set.has('streaming')).toBe(false);
    });

    it('caps tail to MAX_TAIL_SIZE when enough finished messages exist', () => {
      const finished = MAX_TAIL_SIZE + 4;
      const msgs = [
        user('u'),
        tool('active', false),
        ...Array.from({ length: finished }, (_, i) => tool(`t${i}`, true)),
      ];
      expect(tailLen(msgs, true, TAIL)).toBe(MAX_TAIL_SIZE);
    });

    it('tail may exceed MAX_TAIL_SIZE if too many active tools', () => {
      // All active — nothing to flush from the tail
      const msgs = [
        user('u'),
        ...Array.from({ length: MAX_TAIL_SIZE + 3 }, (_, i) =>
          tool(`a${i}`, false)
        ),
      ];
      const set = computeFlushSet(msgs, true, TAIL);
      // Can only flush user (FIFO doneCount=1, min(1, total-2)=1)
      // Guardrail can't flush any active tools, so tail stays large
      expect(set.size).toBe(1);
      expect(set.has('u')).toBe(true);
    });
  });
});
