import { describe, expect, it } from 'bun:test';
import { MessageRole, type MessageType } from '../../stores/app-store';
import { groupMessagesIntoTurns } from '../group-turns';

// --- Builders -------------------------------------------------------------

const user = (id: string, content: string): MessageType => ({
  id,
  role: MessageRole.User,
  content,
});

const steer = (id: string, content: string): MessageType => ({
  id,
  role: MessageRole.User,
  content,
  steered: true,
});

const model = (id: string, content: string): MessageType => ({
  id,
  role: MessageRole.Model,
  content,
});

const standaloneModel = (id: string, content: string): MessageType => ({
  id,
  role: MessageRole.Model,
  content,
  standalone: true,
});

const toolUse = (id: string): MessageType => ({
  id,
  role: MessageRole.ToolUse,
  name: 'fs_read',
  content: '{}',
});

// --- Tests ----------------------------------------------------------------

describe('groupMessagesIntoTurns', () => {
  it('folds consecutive steers into the originating turn with one response', () => {
    // The reported bug: prompt + three mid-turn steers that all concatenate
    // into a single agent response. They must form ONE turn whose body holds
    // the three steer bubbles followed by the shared response — not four
    // separate turns (which is what produced the bogus "Cancelled" labels).
    const turns = groupMessagesIntoTurns([
      user('u1', 'Testing'),
      steer('s1', 'Write poem'),
      steer('s2', 'Write poem'),
      model('m1', "Here's a poem"),
    ]);

    expect(turns).toHaveLength(1);
    expect(turns[0]!.userMessage.id).toBe('u1');
    expect(turns[0]!.aiMessages.map((m) => m.id)).toEqual(['s1', 's2', 'm1']);
    expect(turns[0]!.isActive).toBe(true);
  });

  it('preserves interleave order of steers and assistant content', () => {
    const turns = groupMessagesIntoTurns([
      user('u1', 'do a thing'),
      model('m1', 'working...'),
      steer('s1', 'actually focus here'),
      toolUse('t1'),
      model('m2', 'done'),
    ]);

    expect(turns).toHaveLength(1);
    expect(turns[0]!.aiMessages.map((m) => m.id)).toEqual([
      'm1',
      's1',
      't1',
      'm2',
    ]);
  });

  it('keeps normal prompt/response pairs as separate turns (regression)', () => {
    const turns = groupMessagesIntoTurns([
      user('u1', 'hello'),
      model('m1', 'hi'),
      user('u2', 'world'),
      model('m2', 'hey'),
    ]);

    expect(turns).toHaveLength(2);
    expect(turns[0]!.userMessage.id).toBe('u1');
    expect(turns[0]!.aiMessages.map((m) => m.id)).toEqual(['m1']);
    expect(turns[0]!.isActive).toBe(false);
    expect(turns[1]!.userMessage.id).toBe('u2');
    expect(turns[1]!.aiMessages.map((m) => m.id)).toEqual(['m2']);
    expect(turns[1]!.isActive).toBe(true);
  });

  it('leaves a genuinely empty (cancelled) turn with no body', () => {
    // A prompt that produced nothing — the view renders this as "Cancelled".
    const turns = groupMessagesIntoTurns([user('u1', 'do something')]);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.aiMessages).toEqual([]);
  });

  it('a cancelled turn that received steers still has no assistant body', () => {
    // prompt + steer, then cancelled before any response. The steer is in the
    // body, but there is no assistant content — the view must still be able to
    // detect "no response" (handled by its hasAiContent check, which ignores
    // user-role body messages).
    const turns = groupMessagesIntoTurns([
      user('u1', 'do something'),
      steer('s1', 'and this too'),
    ]);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.aiMessages.map((m) => m.id)).toEqual(['s1']);
    expect(turns[0]!.aiMessages.some((m) => m.role !== MessageRole.User)).toBe(
      false
    );
  });

  it('treats a standalone model message as its own inactive turn', () => {
    const turns = groupMessagesIntoTurns([
      standaloneModel('w1', 'welcome'),
      user('u1', 'hi'),
      model('m1', 'hello'),
    ]);

    expect(turns).toHaveLength(2);
    expect(turns[0]!.userMessage.id).toBe('w1');
    expect(turns[0]!.aiMessages).toEqual([]);
    expect(turns[0]!.isActive).toBe(false);
    expect(turns[1]!.userMessage.id).toBe('u1');
  });

  it('defensively anchors a steer with no preceding prompt', () => {
    const turns = groupMessagesIntoTurns([steer('s1', 'orphan steer')]);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.userMessage.id).toBe('s1');
    expect(turns[0]!.isActive).toBe(false);
  });

  it('returns an empty list for no messages', () => {
    expect(groupMessagesIntoTurns([])).toEqual([]);
  });
});
