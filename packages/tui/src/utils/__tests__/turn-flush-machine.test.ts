import { describe, it, expect } from 'bun:test';
import { computeFlushCount } from '../turn-flush-machine.js';
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

describe('computeFlushCount', () => {
  // --- Basic invariants ---
  it('returns 0 when no messages', () => {
    expect(computeFlushCount([], true, TAIL)).toBe(0);
  });

  it('never returns negative', () => {
    expect(computeFlushCount([user('u')], true, 5)).toBe(0);
  });

  it('returns 0 when total <= tailSize', () => {
    expect(computeFlushCount([user('u'), tool('t1', true)], true, TAIL)).toBe(
      0
    );
  });

  it('respects tailSize=0 — flushes everything done', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    expect(computeFlushCount(msgs, false, 0)).toBe(3);
  });

  // --- User message ---
  it('user message is always done', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', true),
    ];
    // doneCount=4, min(4, 4-2)=2
    expect(computeFlushCount(msgs, true, TAIL)).toBe(2);
  });

  // --- Tool use ---
  it('finished tool is done', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', false),
    ];
    // done: user, t1, t2 → blocked at t3. min(3, 4-2)=2
    expect(computeFlushCount(msgs, true, TAIL)).toBe(2);
  });

  it('unfinished tool blocks itself and everything after it', () => {
    // [user, tool(running), tool(done), model] — running blocks at index 1
    const msgs = [user('u'), tool('t1', false), tool('t2', true), model('m1')];
    // doneCount=1 (user only), min(1, 4-2)=1
    expect(computeFlushCount(msgs, true, TAIL)).toBe(1);
  });

  it('returns 0 when unfinished tool is first ai message and total <= tailSize+1', () => {
    const msgs = [user('u'), tool('t1', false)];
    expect(computeFlushCount(msgs, true, TAIL)).toBe(0);
  });

  // --- Model message ---
  it('does not flush streaming model message (last + isProcessing)', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    // model is last + processing → doneCount=2, min(2, 3-2)=1
    expect(computeFlushCount(msgs, true, TAIL)).toBe(1);
  });

  it('flushes model message when not last (something follows it)', () => {
    // [user, model, tool(done), tool(running)] — model not last → done
    const msgs = [user('u'), model('m1'), tool('t1', true), tool('t2', false)];
    // done: user(1), model(2), tool(done)(3), blocked at t2 → doneCount=3, min(3,4-2)=2
    expect(computeFlushCount(msgs, true, TAIL)).toBe(2);
  });

  it('flushes model message when isProcessing=false', () => {
    const msgs = [user('u'), tool('t1', true), model('m1')];
    // !isProcessing → model is done. doneCount=3, min(3, 3-2)=1
    expect(computeFlushCount(msgs, false, TAIL)).toBe(1);
  });

  // --- Long runs ---
  it('long run: 10 finished tools + final model, processing done', () => {
    const msgs = [
      user('u'),
      ...Array.from({ length: 10 }, (_, i) => tool(`t${i}`, true)),
      model('m1'),
    ];
    // doneCount=12, min(12, 12-2)=10
    expect(computeFlushCount(msgs, false, TAIL)).toBe(10);
  });

  it('long run: 10 finished tools + 1 running, still processing', () => {
    const msgs = [
      user('u'),
      ...Array.from({ length: 10 }, (_, i) => tool(`t${i}`, true)),
      tool('t10', false),
    ];
    // doneCount=11 (user+10 done), blocked at t10. min(11, 12-2)=10
    expect(computeFlushCount(msgs, true, TAIL)).toBe(10);
  });

  // --- LLM text in the middle ---
  it('model message between two tool uses flushes when second tool arrives', () => {
    // [user, tool1(done), model, tool2(done), tool3(running)]
    const msgs = [
      user('u'),
      tool('t1', true),
      model('m1'),
      tool('t2', true),
      tool('t3', false),
    ];
    // done: user, t1, model(not last), t2 → blocked at t3. min(4, 5-2)=3
    expect(computeFlushCount(msgs, true, TAIL)).toBe(3);
  });

  it('model message at end stays in tail while processing', () => {
    const msgs = [user('u'), tool('t1', true), tool('t2', true), model('m1')];
    // model is last+processing → doneCount=3, min(3, 4-2)=2
    expect(computeFlushCount(msgs, true, TAIL)).toBe(2);
  });

  it('model message at end flushes when turn completes', () => {
    const msgs = [user('u'), tool('t1', true), tool('t2', true), model('m1')];
    // !isProcessing → model done. doneCount=4, min(4, 4-2)=2
    expect(computeFlushCount(msgs, false, TAIL)).toBe(2);
  });

  // --- Parallel tool calls ---
  it('parallel tools: two unfinished tools — flushes only settled prefix', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', false),
      tool('t3', false),
    ];
    // done: user, t1 → blocked at t2. min(2, 4-2)=2
    expect(computeFlushCount(msgs, true, TAIL)).toBe(2);
  });

  it('parallel tools: first two finish, last two still running', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', false),
      tool('t4', false),
    ];
    // done: user, t1, t2 → blocked at t3. min(3, 5-2)=3
    expect(computeFlushCount(msgs, true, TAIL)).toBe(3);
  });

  it('parallel tools: all finish', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      tool('t3', true),
      tool('t4', true),
    ];
    // doneCount=5, min(5, 5-2)=3
    expect(computeFlushCount(msgs, true, TAIL)).toBe(3);
  });

  // --- Turn completion ---
  it('completed turn with no tool uses flushes user+models', () => {
    const msgs = [user('u'), model('m1'), model('m2'), model('m3')];
    // !isProcessing → all done. doneCount=4, min(4, 4-2)=2
    expect(computeFlushCount(msgs, false, TAIL)).toBe(2);
  });

  it('single message turn never flushes', () => {
    expect(computeFlushCount([user('u')], false, TAIL)).toBe(0);
  });

  it('exactly tailSize messages never flushes', () => {
    expect(computeFlushCount([user('u'), model('m1')], false, TAIL)).toBe(0);
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
    // done: user, model(not last), tool(done), model(not last) → blocked at t2. doneCount=4, min(4,5-2)=3
    expect(computeFlushCount(msgs, true, TAIL)).toBe(3);
  });

  it('user → tool(done) → tool(done) → model(streaming) → tool(running): model blocks', () => {
    const msgs = [
      user('u'),
      tool('t1', true),
      tool('t2', true),
      model('m1'),
      tool('t3', false),
    ];
    // done: user, t1, t2, model(not last) → blocked at t3. doneCount=4, min(4,5-2)=3
    expect(computeFlushCount(msgs, true, TAIL)).toBe(3);
  });
});
