import { describe, expect, it } from 'bun:test';

import type { SessionEntry } from '../list-all-sessions-cli';
import { isFullSessionId, resolveResumeTarget } from '../resolve-resume-target';

function entry(sessionId: string, executionTarget?: string): SessionEntry {
  return {
    sessionId,
    source: 'v3',
    title: 't',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...(executionTarget ? { executionTarget } : {}),
  };
}

describe('resolveResumeTarget', () => {
  it('flags an ambiguous short prefix and leaves the id untouched (no resume)', () => {
    const r = resolveResumeTarget('abc12345', false, [
      entry('abc12345-1111-4aaa-8bbb-cccccccccccc'),
      entry('abc12345-2222-4aaa-8bbb-cccccccccccc'),
    ]);
    expect(r.ambiguous).toBe(true);
    expect(r.matchCount).toBe(2);
    expect(r.resumeId).toBe('abc12345');
    expect(r.cloud).toBe(false);
  });

  it('expands a unique short prefix to the full id', () => {
    const full = 'abc12345-1111-4aaa-8bbb-cccccccccccc';
    const r = resolveResumeTarget('abc12345', false, [
      entry(full),
      entry('def67890-2222-4aaa-8bbb-cccccccccccc'),
    ]);
    expect(r.ambiguous).toBe(false);
    expect(r.resumeId).toBe(full);
    expect(r.cloud).toBe(false);
  });

  it('flips cloud on when the matched row is a cloud sandbox', () => {
    const full = 'abc12345-1111-4aaa-8bbb-cccccccccccc';
    const r = resolveResumeTarget('abc12345', false, [
      entry(full, 'cloud-sandbox'),
    ]);
    expect(r.cloud).toBe(true);
    expect(r.resumeId).toBe(full);
  });

  it('flips cloud on for any non-local kind (fail-closed)', () => {
    // A future or separate non-local placement (e.g. remote-control) must still
    // route the resume to cloud mode, not be mistaken for local.
    const full = 'abc12345-1111-4aaa-8bbb-cccccccccccc';
    const r = resolveResumeTarget('abc12345', false, [
      entry(full, 'remote-control'),
    ]);
    expect(r.cloud).toBe(true);
    expect(r.resumeId).toBe(full);
  });

  it('leaves cloud unchanged for a local row', () => {
    const full = 'abc12345-1111-4aaa-8bbb-cccccccccccc';
    expect(resolveResumeTarget('abc12345', false, [entry(full)]).cloud).toBe(
      false
    );
  });

  it('matches a full id exactly, not as a prefix', () => {
    const full = 'abc12345-1111-4aaa-8bbb-cccccccccccc';
    const r = resolveResumeTarget(full, false, [
      entry(full),
      entry(`${full}-extra`),
    ]);
    expect(r.ambiguous).toBe(false);
    expect(r.matchCount).toBe(1);
    expect(r.resumeId).toBe(full);
  });

  it('returns no match cleanly (not ambiguous, id untouched)', () => {
    const r = resolveResumeTarget('zzzzzzzz', false, [
      entry('abc12345-1111-4aaa-8bbb-cccccccccccc'),
    ]);
    expect(r.ambiguous).toBe(false);
    expect(r.matchCount).toBe(0);
    expect(r.resumeId).toBe('zzzzzzzz');
  });

  it('classifies full vs short ids', () => {
    expect(isFullSessionId('abc12345-1111-4aaa-8bbb-cccccccccccc')).toBe(true);
    expect(isFullSessionId('abc12345')).toBe(false);
  });

  it('cannot expand or cloud-detect a short id absent from the listing', () => {
    // The pre-connect `--list-sessions` shell-out never returns cloud rows (its
    // one-shot KAS child isn't wired to the remote store), so a cloud session's
    // short prefix has nothing to match: 0 matches, id untouched, cloud unchanged.
    // The launch path must therefore refuse to pass a short id through to
    // session/load as an exact cloud id (only a full id may pass through).
    const localOnly = [entry('abc12345-1111-4aaa-8bbb-cccccccccccc')];
    const r = resolveResumeTarget('dead1234', true, localOnly);
    expect(r.matchCount).toBe(0);
    expect(r.resumeId).toBe('dead1234');
    expect(r.ambiguous).toBe(false);
    // Still short after resolution → caller must not treat it as an exact id.
    expect(isFullSessionId(r.resumeId)).toBe(false);
  });
});
