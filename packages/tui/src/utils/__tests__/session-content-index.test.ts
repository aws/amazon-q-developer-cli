import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  appendFileSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  closeIndex,
  createReconcileState,
  firstPromptHeads,
  forget,
  openIndex,
  promptlessIds,
  readAllMarks,
  reconcile,
  search,
  type IndexHandle,
  type SessionRef,
} from '../session-content-index';
import { readFileRange } from '../bounded-json';

function v2Line(text: string): string {
  return (
    JSON.stringify({
      version: 'v1',
      kind: 'Prompt',
      data: { content: [{ kind: 'text', data: text }] },
    }) + '\n'
  );
}

function kasLine(text: string): string {
  return JSON.stringify({ payload: { type: 'user', content: text } }) + '\n';
}

describe('session-content-index', () => {
  it('completes descriptor ranges across positive short reads', () => {
    const source = Buffer.from('complete transcript range');
    let calls = 0;
    const bytes = readFileRange(
      123,
      0,
      source.length,
      (_fd, buffer, offset, length, position) => {
        const read = Math.min(4, length);
        source.copy(buffer, offset, position ?? 0, (position ?? 0) + read);
        calls++;
        return read;
      }
    );

    expect(bytes).toEqual(source);
    expect(calls).toBeGreaterThan(1);
  });

  let dir: string;
  let dbPath: string;
  let handle: IndexHandle;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'sci-'));
    dbPath = join(dir, 'index.db');
    handle = openIndex(dbPath);
  });

  afterEach(() => {
    closeIndex(handle);
    rmSync(dir, { recursive: true, force: true });
  });

  function ref(id: string, title = ''): SessionRef {
    const transcriptPath = join(dir, `${id}.jsonl`);
    try {
      const stat = statSync(transcriptPath);
      return {
        id,
        transcriptPath,
        title,
        mtimeMs: stat.mtimeMs,
        size: stat.size,
        ctimeMs: stat.ctimeMs,
        transcriptState: 'present',
      };
    } catch {
      return {
        id,
        transcriptPath,
        title,
        mtimeMs: 0,
        size: 0,
        ctimeMs: 0,
        transcriptState: 'missing',
      };
    }
  }

  /** Writes in one test can share a millisecond; force a distinct mtime. */
  function bumpMtime(p: string): void {
    const future = new Date(Date.now() + 2000);
    utimesSync(p, future, future);
  }

  it('opens in content mode on a local filesystem', () => {
    expect(handle.mode).toBe('content');
  });

  it('opens a symlinked index path as titles-only without mutating its target', () => {
    const target = join(dir, 'target.db');
    const link = join(dir, 'linked.db');
    writeFileSync(target, 'sentinel database bytes');
    symlinkSync(target, link);

    const linked = openIndex(link);
    try {
      expect(linked).toEqual({ mode: 'titles', reason: 'unavailable' });
      expect(readFileSync(target, 'utf-8')).toBe('sentinel database bytes');
    } finally {
      closeIndex(linked);
    }
  });

  it('rebuilds a mismatched schema in place while another handle stays open', () => {
    expect(handle.mode).toBe('content');
    if (handle.mode !== 'content') return;
    const inode = statSync(dbPath).ino;
    handle.db.run('PRAGMA user_version = 1');

    const second = openIndex(dbPath);
    try {
      expect(second.mode).toBe('content');
      expect(statSync(dbPath).ino).toBe(inode);
      writeFileSync(join(dir, 'shared.jsonl'), v2Line('shared live handles'));
      expect(reconcile(handle, [ref('shared')]).added).toBe(1);
      expect(search(second, 'live handles').map((hit) => hit.id)).toEqual([
        'shared',
      ]);
      expect(reconcile(second, [ref('shared')]).done).toBe(true);
    } finally {
      closeIndex(second);
    }
  });

  it('honors the disable switch', () => {
    process.env.KIRO_DISABLE_SESSION_SEARCH_INDEX = '1';
    try {
      const h = openIndex(join(dir, 'other.db'));
      expect(h.mode).toBe('titles');
      if (h.mode === 'titles') expect(h.reason).toBe('disabled');
    } finally {
      delete process.env.KIRO_DISABLE_SESSION_SEARCH_INDEX;
    }
  });

  it('indexes V2 and KAS prompt formats and searches them', () => {
    writeFileSync(join(dir, 'v2s.jsonl'), v2Line('fix the flaky login test'));
    writeFileSync(join(dir, 'kass.jsonl'), kasLine('deploy the ingest worker'));
    const r = reconcile(handle, [ref('v2s'), ref('kass')]);
    expect(r.added).toBe(2);

    expect(search(handle, 'flaky login').map((h) => h.id)).toEqual(['v2s']);
    expect(search(handle, 'ingest worker').map((h) => h.id)).toEqual(['kass']);
  });

  it('makes titles searchable even without a transcript', () => {
    const r = reconcile(handle, [ref('bare', 'refactor billing pipeline')]);
    expect(r.added).toBe(1);
    expect(search(handle, 'billing pipeline').map((h) => h.id)).toEqual([
      'bare',
    ]);
  });

  it('tail-reads grown transcripts and finds appended content', () => {
    const p = join(dir, 'grow.jsonl');
    writeFileSync(p, v2Line('first prompt about caching'));
    reconcile(handle, [ref('grow')]);

    appendFileSync(p, v2Line('second prompt about eviction'));
    bumpMtime(p);
    const r = reconcile(handle, [ref('grow')]);
    expect(r.updated).toBe(1);
    expect(search(handle, 'eviction').map((h) => h.id)).toEqual(['grow']);
    // Earlier content survives the incremental pass.
    expect(search(handle, 'caching').map((h) => h.id)).toEqual(['grow']);
  });

  it('revalidates capped transcripts after they are rewritten', () => {
    const p = join(dir, 'capped.jsonl');
    writeFileSync(p, v2Line(`original capped topic ${'x'.repeat(300_000)}`));
    reconcile(handle, [ref('capped')]);
    expect(search(handle, 'original capped').map((hit) => hit.id)).toEqual([
      'capped',
    ]);

    writeFileSync(p, v2Line('replacement after capped rewrite'));
    bumpMtime(p);
    const r = reconcile(handle, [ref('capped')]);

    expect(r.updated).toBe(1);
    expect(search(handle, 'original capped')).toHaveLength(0);
    expect(search(handle, 'replacement after').map((hit) => hit.id)).toEqual([
      'capped',
    ]);
  });

  it('re-reads from scratch when the file head changes (rewrite)', () => {
    const p = join(dir, 'rw.jsonl');
    writeFileSync(p, v2Line('original topic alpha'));
    reconcile(handle, [ref('rw')]);

    // Rewrite with different content of similar size (temp+rename shape).
    writeFileSync(p, v2Line('replaced topic bravo!'));
    bumpMtime(p);
    reconcile(handle, [ref('rw')]);
    expect(search(handle, 'bravo').map((h) => h.id)).toEqual(['rw']);
    expect(search(handle, 'alpha')).toHaveLength(0);
  });

  it('detects a head-preserving rewrite (same first bytes, changed middle)', () => {
    const p = join(dir, 'hp.jsonl');
    const first = v2Line('shared head line kept identical across rewrites');
    writeFileSync(p, first + v2Line('middle about quorum drift'));
    reconcile(handle, [ref('hp')]);
    expect(search(handle, 'quorum').map((h) => h.id)).toEqual(['hp']);

    // Rewrite: identical head, different middle, longer file. Head hash
    // alone would call this an append and tail-read from a stale offset.
    writeFileSync(
      p,
      first +
        v2Line('middle now about lease renewal instead') +
        v2Line('and a third prompt')
    );
    bumpMtime(p);
    reconcile(handle, [ref('hp')]);
    expect(search(handle, 'lease renewal').map((h) => h.id)).toEqual(['hp']);
    expect(search(handle, 'quorum')).toHaveLength(0);
  });

  it('re-reads in full when the tail will not parse', () => {
    const p = join(dir, 'up.jsonl');
    writeFileSync(p, v2Line('starting prompt about tracing'));
    reconcile(handle, [ref('up')]);

    // Grow the file with a complete but non-JSON line, then a real prompt.
    // The mtime bump is explicit: an append within the same millisecond
    // would otherwise hit the unchanged-mtime fast path and be skipped.
    appendFileSync(
      p,
      'this line is not json\n' + v2Line('later prompt about spans')
    );
    bumpMtime(p);
    const r = reconcile(handle, [ref('up')]);
    expect(r.updated).toBe(1);
    // Both old and new content present — the full re-read kept everything.
    expect(search(handle, 'tracing').map((h) => h.id)).toEqual(['up']);
    expect(search(handle, 'spans').map((h) => h.id)).toEqual(['up']);
    // The doubled-content failure would surface as duplicated prompts; a
    // subsequent no-op reconcile must not change anything.
    const again = reconcile(handle, [ref('up')]);
    expect(again.updated).toBe(0);
  });

  it('does not classify malformed transcripts as known promptless sessions', () => {
    writeFileSync(join(dir, 'malformed.jsonl'), 'not json\n');

    reconcile(handle, [ref('malformed')]);

    expect(promptlessIds(handle).has('malformed')).toBe(false);
  });

  it('reindexes title-only and same-mtime size changes', () => {
    const path = join(dir, 'fingerprint.jsonl');
    writeFileSync(path, v2Line('original prompt'));
    reconcile(handle, [ref('fingerprint', 'Original title')]);
    const originalTime = statSync(path).mtime;

    appendFileSync(path, v2Line('appended prompt'));
    utimesSync(path, originalTime, originalTime);
    reconcile(handle, [ref('fingerprint', 'Renamed title')]);

    expect(search(handle, 'appended').map((hit) => hit.id)).toEqual([
      'fingerprint',
    ]);
    expect(search(handle, 'Renamed').map((hit) => hit.id)).toEqual([
      'fingerprint',
    ]);
  });

  it('full-reads large transcripts whose unchecked middle may have changed', () => {
    const path = join(dir, 'large-middle.jsonl');
    const head = v2Line(`stable head ${'h'.repeat(400)}`);
    const tail = v2Line(`stable tail ${'t'.repeat(5000)}`);
    writeFileSync(path, head + v2Line('obsolete middle topic') + tail);
    reconcile(handle, [ref('large-middle')]);

    writeFileSync(
      path,
      head +
        v2Line('replacement middle topic') +
        tail +
        v2Line('new appended topic')
    );
    bumpMtime(path);
    reconcile(handle, [ref('large-middle')]);

    expect(search(handle, 'replacement').map((hit) => hit.id)).toEqual([
      'large-middle',
    ]);
    expect(search(handle, 'appended').map((hit) => hit.id)).toEqual([
      'large-middle',
    ]);
    expect(search(handle, 'obsolete')).toHaveLength(0);
  });

  it('drops vanished sessions', () => {
    writeFileSync(join(dir, 'gone.jsonl'), v2Line('ephemeral'));
    reconcile(handle, [ref('gone'), ref('stays', 'stays')]);
    expect(search(handle, 'ephemeral')).toHaveLength(1);

    const r = reconcile(handle, [ref('stays', 'stays')]);
    expect(r.removed).toBe(1);
    expect(search(handle, 'ephemeral')).toHaveLength(0);
  });

  it('removes large sets when the complete listing confirms deletion', () => {
    const refs: SessionRef[] = [];
    for (let i = 0; i < 30; i++) {
      writeFileSync(join(dir, `s${i}.jsonl`), v2Line(`topic number ${i}`));
      refs.push(ref(`s${i}`));
    }
    reconcile(handle, refs);
    expect(search(handle, 'topic', 40).length).toBe(30);

    const r = reconcile(handle, refs.slice(0, 2));
    expect(r.removed).toBe(28);
    expect(search(handle, 'topic', 40).length).toBe(2);
  });

  it('preserves missing rows when reconciliation has a partial listing', () => {
    writeFileSync(join(dir, 'kept.jsonl'), v2Line('retained partial topic'));
    writeFileSync(join(dir, 'updated.jsonl'), v2Line('updated partial topic'));
    reconcile(handle, [ref('kept'), ref('updated')]);

    const r = reconcile(handle, [ref('updated')], { removeMissing: false });

    expect(r.removed).toBe(0);
    expect(search(handle, 'retained partial').map((hit) => hit.id)).toEqual([
      'kept',
    ]);
  });

  it('stops at the byte budget and resumes to completion', () => {
    const refs: SessionRef[] = [];
    for (let i = 0; i < 5; i++) {
      writeFileSync(
        join(dir, `b${i}.jsonl`),
        v2Line(`budget item ${i} ` + 'x'.repeat(2000))
      );
      refs.push(ref(`b${i}`));
    }
    const state = createReconcileState(handle, refs);
    let result = reconcile(handle, refs, { byteBudget: 512, state });
    expect(result.done).toBe(false);

    let calls = 1;
    while (!result.done && calls < 100) {
      result = reconcile(handle, refs, { byteBudget: 512, state });
      calls++;
    }
    expect(result.done).toBe(true);
    expect(calls).toBeGreaterThan(5);
    expect(search(handle, 'budget item', 10).length).toBe(5);
  });

  it('does not let a stale budgeted writer replace a newer publication', () => {
    const path = join(dir, 'cas.jsonl');
    writeFileSync(path, v2Line('shared immutable prompt'));
    const staleRefs = [ref('cas', 'Stale title')];
    const staleState = createReconcileState(handle, staleRefs);

    const first = reconcile(handle, staleRefs, {
      byteBudget: 1,
      state: staleState,
    });
    expect(first.done).toBe(false);

    const newer = openIndex(dbPath);
    try {
      expect(
        reconcile(newer, [ref('cas', 'Authoritative newer title')]).added
      ).toBe(1);

      const stale = reconcile(handle, staleRefs, { state: staleState });
      expect(stale.done).toBe(false);
      expect(stale.skipped).toBe('conflict');
      expect(
        search(handle, 'Authoritative newer').map((hit) => hit.id)
      ).toEqual(['cas']);
      expect(search(handle, 'Stale title')).toHaveLength(0);
      expect(readAllMarks(handle).get('cas')?.revision).toBe(1);
    } finally {
      closeIndex(newer);
    }
  });

  it('rolls back stale removals when a later row loses its CAS', () => {
    const keptPath = join(dir, 'kept-after-conflict.jsonl');
    const changedPath = join(dir, 'changed-after-conflict.jsonl');
    writeFileSync(keptPath, v2Line('must survive stale removal'));
    writeFileSync(changedPath, v2Line('shared conflict prompt'));
    reconcile(handle, [
      ref('kept-after-conflict'),
      ref('changed-after-conflict', 'Original title'),
    ]);

    const staleRefs = [ref('changed-after-conflict', 'Stale title')];
    const staleState = createReconcileState(handle, staleRefs);
    const newer = openIndex(dbPath);
    try {
      expect(
        reconcile(newer, [
          ref('kept-after-conflict'),
          ref('changed-after-conflict', 'Current title'),
        ]).updated
      ).toBe(1);

      const conflict = reconcile(handle, staleRefs, { state: staleState });

      expect(conflict.skipped).toBe('conflict');
      expect(conflict.removed).toBe(0);
      expect(
        search(handle, 'must survive stale removal').map((hit) => hit.id)
      ).toEqual(['kept-after-conflict']);
    } finally {
      closeIndex(newer);
    }
  });

  it('requires a fresh plan when a stale writer publishes first', () => {
    const path = join(dir, 'stale-first.jsonl');
    writeFileSync(path, v2Line('shared prompt'));
    const staleRefs = [ref('stale-first', 'Stale title')];
    const currentRefs = [ref('stale-first', 'Current title')];
    const staleState = createReconcileState(handle, staleRefs);
    const currentState = createReconcileState(handle, currentRefs);

    expect(reconcile(handle, staleRefs, { state: staleState }).done).toBe(true);
    const conflict = reconcile(handle, currentRefs, { state: currentState });
    expect(conflict.done).toBe(false);
    expect(conflict.skipped).toBe('conflict');
    expect(search(handle, 'Current title')).toHaveLength(0);

    const freshState = createReconcileState(handle, currentRefs);
    const refreshed = reconcile(handle, currentRefs, { state: freshState });
    expect(refreshed.done).toBe(true);
    expect(search(handle, 'Current title').map((hit) => hit.id)).toEqual([
      'stale-first',
    ]);
  });

  it('restarts a budgeted read when the transcript is rewritten', () => {
    const path = join(dir, 'rewrite-during-read.jsonl');
    writeFileSync(path, v2Line('obsolete budgeted topic'));
    const refs = [ref('rewrite-during-read')];
    const state = createReconcileState(handle, refs);

    const first = reconcile(handle, refs, { byteBudget: 8, state });
    expect(first.done).toBe(false);

    writeFileSync(path, v2Line('replaced budgeted topic'));
    bumpMtime(path);
    const final = reconcile(handle, refs, { state });

    expect(final.done).toBe(true);
    expect(search(handle, 'replaced budgeted').map((hit) => hit.id)).toEqual([
      'rewrite-during-read',
    ]);
    expect(search(handle, 'obsolete budgeted')).toHaveLength(0);
  });

  it('keeps every budgeted chunk on one opened file identity', () => {
    const sourceA = join(dir, 'source-a.jsonl');
    const sourceB = join(dir, 'source-b.jsonl');
    const alias = join(dir, 'aba.jsonl');
    const aLines = [
      v2Line('source A first'),
      v2Line('source A middle'),
      v2Line('source A final'),
    ];
    const bLines = [
      v2Line('source B first'),
      v2Line('source B middle'),
      v2Line('source B final'),
    ];
    writeFileSync(sourceA, aLines.join(''));
    writeFileSync(sourceB, bLines.join(''));
    symlinkSync(sourceA, alias);
    const refs = [ref('aba')];
    const state = createReconcileState(handle, refs);

    expect(
      reconcile(handle, refs, { byteBudget: aLines[0]!.length, state }).done
    ).toBe(false);
    rmSync(alias);
    symlinkSync(sourceB, alias);
    expect(
      reconcile(handle, refs, { byteBudget: aLines[1]!.length, state }).done
    ).toBe(false);
    rmSync(alias);
    symlinkSync(sourceA, alias);

    let result = reconcile(handle, refs, { state });
    while (!result.done) result = reconcile(handle, refs, { state });
    expect(search(handle, 'source A middle').map((hit) => hit.id)).toEqual([
      'aba',
    ]);
    expect(search(handle, 'source B middle')).toHaveLength(0);
  });

  it('does not publish one oversized transcript until bounded slices consume it', () => {
    const id = 'oversized';
    const text = `bounded reconciliation ${'x'.repeat(12_000)}`;
    writeFileSync(join(dir, `${id}.jsonl`), v2Line(text));
    const refs = [ref(id)];
    const state = createReconcileState(handle, refs);

    let result = reconcile(handle, refs, { byteBudget: 1024, state });
    expect(result.done).toBe(false);
    expect(result.added).toBe(0);
    expect(state.cursor).toBe(0);
    expect(search(handle, 'bounded reconciliation')).toHaveLength(0);

    let calls = 1;
    while (!result.done && calls < 100) {
      result = reconcile(handle, refs, { byteBudget: 1024, state });
      calls++;
    }
    expect(result.done).toBe(true);
    expect(calls).toBeGreaterThan(1);
    expect(
      search(handle, 'bounded reconciliation').map((hit) => hit.id)
    ).toEqual([id]);
  });

  it('reuses one work plan and advances its cursor across many slices', () => {
    const refs: SessionRef[] = [];
    for (let i = 0; i < 55; i++) {
      writeFileSync(join(dir, `slice${i}.jsonl`), v2Line(`slice topic ${i}`));
      refs.push(ref(`slice${i}`));
    }
    const state = createReconcileState(handle, refs);
    const work = state.work;
    const sliceBudget = Buffer.byteLength(v2Line('slice topic 0'));

    let result = reconcile(handle, refs, { byteBudget: sliceBudget, state });
    expect(result.done).toBe(false);
    expect(state.work).toBe(work);
    expect(state.cursor).toBe(1);

    let calls = 1;
    while (!result.done && calls < 120) {
      result = reconcile(handle, refs, { byteBudget: sliceBudget, state });
      calls++;
      expect(state.work).toBe(work);
    }
    expect(result.done).toBe(true);
    expect(state.cursor).toBe(55);
    expect(search(handle, 'slice topic', 60)).toHaveLength(55);
  });

  it('neutralises FTS operators in user queries', () => {
    writeFileSync(join(dir, 'q.jsonl'), v2Line('normal text'));
    reconcile(handle, [ref('q')]);
    // None of these may throw or form a syntax query.
    expect(() => search(handle, 'NEAR( OR "unclosed')).not.toThrow();
    expect(() => search(handle, 'a* AND b')).not.toThrow();
  });

  it('forget removes one session', () => {
    writeFileSync(join(dir, 'f.jsonl'), v2Line('forgettable content'));
    reconcile(handle, [ref('f')]);
    forget(handle, 'f');
    expect(search(handle, 'forgettable')).toHaveLength(0);
  });

  it('keeps an unterminated JSONL record incomplete until it is finished', () => {
    const p = join(dir, 'frag.jsonl');
    const eventual = v2Line('eventual prompt').trimEnd();
    writeFileSync(p, v2Line('complete line here') + eventual.slice(0, -1));

    const first = reconcile(handle, [ref('frag')]);

    expect(first.added).toBe(1);
    expect(search(handle, 'complete line').map((hit) => hit.id)).toEqual([
      'frag',
    ]);
    expect(search(handle, 'eventual prompt')).toEqual([]);
    expect(readAllMarks(handle).get('frag')?.parseComplete).toBe(false);

    appendFileSync(p, eventual.slice(-1) + '\n');
    bumpMtime(p);
    const second = reconcile(handle, [ref('frag')]);

    expect(second.updated).toBe(1);
    expect(search(handle, 'eventual prompt').map((hit) => hit.id)).toEqual([
      'frag',
    ]);
    expect(readAllMarks(handle).get('frag')?.parseComplete).toBe(true);
  });

  it('skips an oversized unterminated record with bounded parser state', () => {
    const id = 'huge-fragment';
    const path = join(dir, `${id}.jsonl`);
    writeFileSync(
      path,
      `{"payload":{"type":"user","content":"${'x'.repeat(1024 * 1024 + 64 * 1024)}`
    );

    let refs = [ref(id, 'Oversized record session')];
    let state = createReconcileState(handle, refs);
    let result = reconcile(handle, refs, { byteBudget: 64 * 1024, state });
    let calls = 1;
    while (!result.done && calls < 100) {
      result = reconcile(handle, refs, { byteBudget: 64 * 1024, state });
      calls++;
    }
    expect(result.done).toBe(true);
    expect(calls).toBeGreaterThan(10);
    expect(
      search(handle, 'Oversized record session').map((hit) => hit.id)
    ).toEqual([id]);

    appendFileSync(path, `\n${v2Line('valid prompt after oversized record')}`);
    bumpMtime(path);
    refs = [ref(id, 'Oversized record session')];
    state = createReconcileState(handle, refs);
    result = reconcile(handle, refs, { byteBudget: 64 * 1024, state });
    calls = 1;
    while (!result.done && calls < 100) {
      result = reconcile(handle, refs, { byteBudget: 64 * 1024, state });
      calls++;
    }
    expect(result.done).toBe(true);
    expect(
      search(handle, 'valid prompt after oversized').map((hit) => hit.id)
    ).toEqual([id]);
  });

  it('serves first-prompt heads for prompted sessions only', () => {
    writeFileSync(join(dir, 'kp.jsonl'), kasLine('deploy the ingest worker'));
    writeFileSync(join(dir, 'bare.jsonl'), '');
    reconcile(handle, [ref('kp'), ref('bare', 'has a title, no prompt')]);

    const heads = firstPromptHeads(handle);
    expect(heads.get('kp')).toContain('deploy the ingest worker');
    expect(heads.has('bare')).toBe(false);
  });
});
