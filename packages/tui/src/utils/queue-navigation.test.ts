import { describe, expect, it } from 'bun:test';
import {
  navigateQueueUp,
  navigateQueueDown,
  commitQueueRestore,
  type QueueRestoreState,
} from './queue-navigation.js';

describe('navigateQueueUp', () => {
  it('falls through to history when queue is empty', () => {
    expect(navigateQueueUp(null, '', [])).toEqual({ kind: 'history' });
  });

  it('falls through to history when user is mid-typing', () => {
    expect(navigateQueueUp(null, 'hello', ['queued'])).toEqual({
      kind: 'history',
    });
  });

  it('enters restore mode at the newest queue entry', () => {
    const queue = ['oldest', 'middle', 'newest'];
    const result = navigateQueueUp(null, '', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 2, originalText: 'newest' },
      loadText: 'newest',
    });
  });

  it('walks backward through the queue without replacing if input is unchanged', () => {
    const queue = ['oldest', 'middle', 'newest'];
    const state: QueueRestoreState = {
      index: 2,
      originalText: 'newest',
    };
    const result = navigateQueueUp(state, 'newest', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, originalText: 'middle' },
      loadText: 'middle',
    });
    expect((result as any).replace).toBeUndefined();
  });

  it('commits edited text before stepping back', () => {
    const queue = ['oldest', 'newest'];
    const state: QueueRestoreState = {
      index: 1,
      originalText: 'newest',
    };
    const result = navigateQueueUp(state, 'newest-edited', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 0, originalText: 'oldest' },
      loadText: 'oldest',
      replace: { index: 1, text: 'newest-edited' },
    });
  });

  it('exits restore mode past the oldest entry', () => {
    const queue = ['only'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'only',
    };
    const result = navigateQueueUp(state, 'only', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: null,
      loadText: '',
    });
  });

  it('commits edited text even when exiting past the oldest entry', () => {
    const queue = ['only'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'only',
    };
    const result = navigateQueueUp(state, 'only-edited', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: null,
      loadText: '',
      replace: { index: 0, text: 'only-edited' },
    });
  });
});

describe('navigateQueueDown', () => {
  it('falls through to history when not in restore mode', () => {
    expect(navigateQueueDown(null, '', ['anything'])).toEqual({
      kind: 'history',
    });
  });

  it('walks forward through the queue', () => {
    const queue = ['oldest', 'middle', 'newest'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'oldest',
    };
    const result = navigateQueueDown(state, 'oldest', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, originalText: 'middle' },
      loadText: 'middle',
    });
  });

  it('commits edited text before stepping forward', () => {
    const queue = ['a', 'b', 'c'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'a',
    };
    const result = navigateQueueDown(state, 'a-edited', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: { index: 1, originalText: 'b' },
      loadText: 'b',
      replace: { index: 0, text: 'a-edited' },
    });
  });

  it('exits restore mode past the newest entry', () => {
    const queue = ['only'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'only',
    };
    const result = navigateQueueDown(state, 'only', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: null,
      loadText: '',
    });
  });

  it('commits edited text when exiting past the newest entry', () => {
    const queue = ['only'];
    const state: QueueRestoreState = {
      index: 0,
      originalText: 'only',
    };
    const result = navigateQueueDown(state, 'only-edited', queue);
    expect(result).toEqual({
      kind: 'queue',
      state: null,
      loadText: '',
      replace: { index: 0, text: 'only-edited' },
    });
  });
});

describe('commitQueueRestore', () => {
  it('replaces the slot when the queue is unchanged', () => {
    const state: QueueRestoreState = {
      index: 1,
      originalText: 'middle',
    };
    const queue = ['oldest', 'middle', 'newest'];
    expect(commitQueueRestore(state, 'middle-edited', queue)).toEqual({
      kind: 'replace',
      index: 1,
      text: 'middle-edited',
    });
  });

  it('falls back to a normal send when the slot drained out', () => {
    // The agent ate index 0 (old: ['middle', 'newest']) while we were editing.
    const state: QueueRestoreState = {
      index: 1,
      originalText: 'middle',
    };
    const queue = ['middle', 'newest'];
    // Index 1 still exists but it's now `newest`, not the originalText.
    expect(commitQueueRestore(state, 'middle-edited', queue)).toEqual({
      kind: 'fallback',
      text: 'middle-edited',
    });
  });

  it('falls back when index drops past the queue end', () => {
    const state: QueueRestoreState = {
      index: 2,
      originalText: 'newest',
    };
    const queue = ['middle']; // queue drained
    expect(commitQueueRestore(state, 'newest-edited', queue)).toEqual({
      kind: 'fallback',
      text: 'newest-edited',
    });
  });
});
