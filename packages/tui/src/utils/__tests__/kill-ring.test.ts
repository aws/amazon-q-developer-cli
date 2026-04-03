import { describe, expect, it } from 'bun:test';
import { KillRing } from 'twinki';

describe('KillRing', () => {
  it('returns undefined when empty', () => {
    const ring = new KillRing();
    expect(ring.peek()).toBeUndefined();
    expect(ring.length).toBe(0);
  });

  it('push and peek', () => {
    const ring = new KillRing();
    ring.push('hello', { prepend: false });
    expect(ring.peek()).toBe('hello');
    expect(ring.length).toBe(1);
  });

  it('ignores empty string push', () => {
    const ring = new KillRing();
    ring.push('', { prepend: false });
    expect(ring.length).toBe(0);
  });

  it('accumulates with prepend (backward kills)', () => {
    const ring = new KillRing();
    ring.push('world', { prepend: true });
    ring.push('hello ', { prepend: true, accumulate: true });
    expect(ring.peek()).toBe('hello world');
    expect(ring.length).toBe(1);
  });

  it('accumulates with append (forward kills)', () => {
    const ring = new KillRing();
    ring.push('hello', { prepend: false });
    ring.push(' world', { prepend: false, accumulate: true });
    expect(ring.peek()).toBe('hello world');
    expect(ring.length).toBe(1);
  });

  it('does not accumulate when accumulate is false', () => {
    const ring = new KillRing();
    ring.push('first', { prepend: false });
    ring.push('second', { prepend: false });
    expect(ring.peek()).toBe('second');
    expect(ring.length).toBe(2);
  });

  it('rotate cycles entries for yank-pop', () => {
    const ring = new KillRing();
    ring.push('first', { prepend: false });
    ring.push('second', { prepend: false });
    ring.push('third', { prepend: false });
    expect(ring.peek()).toBe('third');
    ring.rotate();
    expect(ring.peek()).toBe('second');
    ring.rotate();
    expect(ring.peek()).toBe('first');
    ring.rotate();
    expect(ring.peek()).toBe('third');
  });

  it('rotate with single entry is a no-op', () => {
    const ring = new KillRing();
    ring.push('only', { prepend: false });
    ring.rotate();
    expect(ring.peek()).toBe('only');
  });
});
