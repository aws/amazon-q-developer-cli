import { describe, expect, it } from 'bun:test';
import {
  navigateQueueUp,
  navigateQueueDown,
  commitQueueRestore,
  buildUnifiedQueueEntries,
  spliceSteerLine,
  removeSteerLine,
  type QueueRestoreState,
  type UnifiedQueueEntry,
} from './queue-navigation.js';

/** Helper: build the unified entry list the nav machine operates over. */
const q = (steer: string | null, queue: readonly string[]) =>
  buildUnifiedQueueEntries(steer, queue);

describe('buildUnifiedQueueEntries', () => {
  it('returns an empty list when nothing is staged', () => {
    expect(buildUnifiedQueueEntries(null, [])).toEqual([]);
    expect(buildUnifiedQueueEntries('', [])).toEqual([]);
  });

  it('renders queue entries with their queuedMessages index', () => {
    expect(buildUnifiedQueueEntries(null, ['a', 'b'])).toEqual([
      { kind: 'queue', text: 'a', queueIndex: 0 },
      { kind: 'queue', text: 'b', queueIndex: 1 },
    ]);
  });

  it('puts steer line(s) FIRST, splitting on the backend "\\n\\n" separator', () => {
    expect(buildUnifiedQueueEntries('s1\n\ns2', ['a'])).toEqual([
      { kind: 'steer', text: 's1' },
      { kind: 'steer', text: 's2' },
      { kind: 'queue', text: 'a', queueIndex: 0 },
    ]);
  });

  it('preserves queueIndex regardless of how many steer lines sit above', () => {
    const entries = buildUnifiedQueueEntries('s1\n\ns2', ['a', 'b']);
    // The queue entries are at display index 2 and 3, but keep queueIndex 0/1.
    expect(entries[2]).toEqual({ kind: 'queue', text: 'a', queueIndex: 0 });
    expect(entries[3]).toEqual({ kind: 'queue', text: 'b', queueIndex: 1 });
  });
});

describe('navigateQueueUp', () => {
  it('falls through to history when list is empty', () => {
    expect(navigateQueueUp(null, '', q(null, []))).toEqual({ kind: 'history' });
  });

  it('falls through to history when user is mid-typing', () => {
    expect(navigateQueueUp(null, 'hello', q(null, ['queued']))).toEqual({
      kind: 'history',
    });
  });

  it('enters restore mode at the newest queue entry', () => {
    const entries = q(null, ['oldest', 'middle', 'newest']);
    const result = navigateQueueUp(null, '', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 2, kind: 'queue', queueIndex: 2, originalText: 'newest' },
      loadText: 'newest',
    });
  });

  it('walks backward through the queue without replacing if input is unchanged', () => {
    const entries = q(null, ['oldest', 'middle', 'newest']);
    const state: QueueRestoreState = {
      index: 2,
      kind: 'queue',
      queueIndex: 2,
      originalText: 'newest',
    };
    const result = navigateQueueUp(state, 'newest', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, kind: 'queue', queueIndex: 1, originalText: 'middle' },
      loadText: 'middle',
    });
    expect((result as any).replace).toBeUndefined();
  });

  it('commits edited QUEUE text before stepping back', () => {
    const entries = q(null, ['oldest', 'newest']);
    const state: QueueRestoreState = {
      index: 1,
      kind: 'queue',
      queueIndex: 1,
      originalText: 'newest',
    };
    const result = navigateQueueUp(state, 'newest-edited', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 0, kind: 'queue', queueIndex: 0, originalText: 'oldest' },
      loadText: 'oldest',
      replace: { kind: 'queue', queueIndex: 1, text: 'newest-edited' },
    });
  });

  it('exits restore mode past the oldest entry', () => {
    const entries = q(null, ['only']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'queue',
      queueIndex: 0,
      originalText: 'only',
    };
    const result = navigateQueueUp(state, 'only', entries);
    expect(result).toEqual({ kind: 'queue', state: null, loadText: '' });
  });

  it('crosses from the oldest queue entry up into a steer entry', () => {
    // Display: [steer:s] [queue:q@0]. From the queue entry, ↑ lands on steer.
    const entries = q('s', ['q']);
    const state: QueueRestoreState = {
      index: 1,
      kind: 'queue',
      queueIndex: 0,
      originalText: 'q',
    };
    const result = navigateQueueUp(state, 'q', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 0, kind: 'steer', originalText: 's' },
      loadText: 's',
    });
  });

  it('commits an edited STEER entry as a steer replace (not a queue index)', () => {
    const entries = q('s1\n\ns2', []);
    const state: QueueRestoreState = {
      index: 1,
      kind: 'steer',
      originalText: 's2',
    };
    const result = navigateQueueUp(state, 's2-edited', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 0, kind: 'steer', originalText: 's1' },
      loadText: 's1',
      replace: { kind: 'steer', text: 's2-edited', targetLine: 's2' },
    });
  });
});

describe('navigateQueueDown', () => {
  it('falls through to history when not in restore mode', () => {
    expect(navigateQueueDown(null, '', q(null, ['anything']))).toEqual({
      kind: 'history',
    });
  });

  it('walks forward through the queue', () => {
    const entries = q(null, ['oldest', 'middle', 'newest']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'queue',
      queueIndex: 0,
      originalText: 'oldest',
    };
    const result = navigateQueueDown(state, 'oldest', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, kind: 'queue', queueIndex: 1, originalText: 'middle' },
      loadText: 'middle',
    });
  });

  it('crosses from a steer entry down into the first queue entry', () => {
    const entries = q('s', ['q']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'steer',
      originalText: 's',
    };
    const result = navigateQueueDown(state, 's', entries);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, kind: 'queue', queueIndex: 0, originalText: 'q' },
      loadText: 'q',
    });
  });

  it('exits restore mode past the newest entry', () => {
    const entries = q(null, ['only']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'queue',
      queueIndex: 0,
      originalText: 'only',
    };
    const result = navigateQueueDown(state, 'only', entries);
    expect(result).toEqual({ kind: 'queue', state: null, loadText: '' });
  });
});

describe('commitQueueRestore', () => {
  it('replaces the QUEUE slot when the entry is unchanged', () => {
    const entries = q(null, ['oldest', 'middle', 'newest']);
    const state: QueueRestoreState = {
      index: 1,
      kind: 'queue',
      queueIndex: 1,
      originalText: 'middle',
    };
    expect(commitQueueRestore(state, 'middle-edited', entries)).toEqual({
      kind: 'replace-queue',
      queueIndex: 1,
      text: 'middle-edited',
    });
  });

  it('deletes the QUEUE slot when the edit is emptied', () => {
    const entries = q(null, ['a', 'b']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'queue',
      queueIndex: 0,
      originalText: 'a',
    };
    expect(commitQueueRestore(state, '   ', entries)).toEqual({
      kind: 'delete-queue',
      queueIndex: 0,
    });
  });

  it('uses the right queueIndex even with steer lines above (index alignment)', () => {
    // Display: [steer:s1] [steer:s2] [queue:b@0] [queue:c@1].
    // Editing display row 3 must hit queuedMessages[1], NOT [3].
    const entries = q('s1\n\ns2', ['b', 'c']);
    const state: QueueRestoreState = {
      index: 3,
      kind: 'queue',
      queueIndex: 1,
      originalText: 'c',
    };
    expect(commitQueueRestore(state, 'c-edited', entries)).toEqual({
      kind: 'replace-queue',
      queueIndex: 1,
      text: 'c-edited',
    });
  });

  it('commits an edited STEER entry as a clear-and-resteer', () => {
    const entries = q('s1\n\ns2', ['a']);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'steer',
      originalText: 's1',
    };
    expect(commitQueueRestore(state, 's1-edited', entries)).toEqual({
      kind: 'replace-steer',
      text: 's1-edited',
      targetLine: 's1',
    });
  });

  it('deletes the STEER when the edit is emptied', () => {
    const entries = q('s1', []);
    const state: QueueRestoreState = {
      index: 0,
      kind: 'steer',
      originalText: 's1',
    };
    expect(commitQueueRestore(state, '  ', entries)).toEqual({
      kind: 'delete-steer',
      targetLine: 's1',
    });
  });

  it('falls back to a normal send when the queue slot drained out', () => {
    // The agent ate index 0 (old: ['middle', 'newest']) while we were editing.
    const entries = q(null, ['middle', 'newest']);
    const state: QueueRestoreState = {
      index: 1,
      kind: 'queue',
      queueIndex: 1,
      originalText: 'middle',
    };
    // Display index 1 still exists but it's now `newest`, not the originalText.
    expect(commitQueueRestore(state, 'middle-edited', entries)).toEqual({
      kind: 'fallback',
      text: 'middle-edited',
    });
  });

  it('falls back when the display index drops past the list end', () => {
    const entries = q(null, ['middle']); // queue drained
    const state: QueueRestoreState = {
      index: 2,
      kind: 'queue',
      queueIndex: 2,
      originalText: 'newest',
    };
    expect(commitQueueRestore(state, 'newest-edited', entries)).toEqual({
      kind: 'fallback',
      text: 'newest-edited',
    });
  });
});

// Multi-steer line-aware edit/delete. The steer buffer can hold
// several lines joined by "\n\n" (pre-init: successive submissions concatenate;
// mid-turn: the backend concatenates successive steers). Editing or deleting
// ONE steer row must preserve the others — the previous whole-buffer replace
// silently dropped sibling steer lines (pure local data loss pre-init).
describe('spliceSteerLine (line-aware steer edit)', () => {
  it('replaces only the targeted line, preserving siblings', () => {
    expect(spliceSteerLine('s1\n\ns2\n\ns3', 's2', 's2-edited')).toBe(
      's1\n\ns2-edited\n\ns3'
    );
  });

  it('replaces the first line without touching the rest', () => {
    expect(spliceSteerLine('s1\n\ns2', 's1', 'first-edited')).toBe(
      'first-edited\n\ns2'
    );
  });

  it('handles a single-line buffer (no separator)', () => {
    expect(spliceSteerLine('only', 'only', 'only-edited')).toBe('only-edited');
  });

  it('falls back to the replacement when the target is not found', () => {
    // Buffer changed underneath us — never silently drop the edit.
    expect(spliceSteerLine('a\n\nb', 'gone', 'edited')).toBe('edited');
  });

  it('only replaces the FIRST matching occurrence', () => {
    expect(spliceSteerLine('dup\n\ndup', 'dup', 'x')).toBe('x\n\ndup');
  });
});

describe('removeSteerLine (line-aware steer delete)', () => {
  it('removes only the targeted line, preserving siblings', () => {
    expect(removeSteerLine('s1\n\ns2\n\ns3', 's2')).toBe('s1\n\ns3');
  });

  it('removes the first line', () => {
    expect(removeSteerLine('s1\n\ns2', 's1')).toBe('s2');
  });

  it('returns null when removing the only line (caller clears the buffer)', () => {
    expect(removeSteerLine('only', 'only')).toBeNull();
  });

  it('returns null when the target is not found (full clear)', () => {
    expect(removeSteerLine('a\n\nb', 'gone')).toBeNull();
  });
});
