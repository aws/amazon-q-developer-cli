import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { DASHBOARD_SIDECAR_MAX_BYTES } from '../bounded-json';
import {
  SessionBookmarkStore,
  normalizeTag,
  parseTags,
  getSessionBookmarkStore,
  resetSessionBookmarkStore,
} from '../session-bookmarks';

const BRIDGED_UUID = '44c187c1-3509-4b20-9965-6dbda1203007';
const V2_COPY = BRIDGED_UUID;
const KAS_COPY = `cli_${BRIDGED_UUID}_run`;

describe('normalizeTag', () => {
  it('lowercases, trims, strips leading #, hyphenates spaces', () => {
    expect(normalizeTag('  #Auth Bug ')).toBe('auth-bug');
    expect(normalizeTag('WIP')).toBe('wip');
    expect(normalizeTag('##double')).toBe('double');
    expect(normalizeTag('   ')).toBe('');
  });
});

describe('parseTags', () => {
  it('splits on commas and spaces, dedupes, normalizes', () => {
    expect(parseTags('auth, wip  bug, #Auth')).toEqual(['auth', 'wip', 'bug']);
  });
  it('handles empty input', () => {
    expect(parseTags('')).toEqual([]);
    expect(parseTags('  ,  ')).toEqual([]);
  });
});

describe('SessionBookmarkStore', () => {
  let dir: string;
  let path: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'bookmarks-'));
    path = join(dir, 'dashboard-meta.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('toggles a bookmark and reports state', () => {
    const store = new SessionBookmarkStore(path);
    expect(store.isBookmarked('s1')).toBe(false);
    expect(store.toggleBookmark('s1')).toEqual({ ok: true, value: true });
    expect(store.isBookmarked('s1')).toBe(true);
    expect(store.toggleBookmark('s1')).toEqual({ ok: true, value: false });
    expect(store.isBookmarked('s1')).toBe(false);
  });

  it('persists bookmarks across instances', () => {
    const a = new SessionBookmarkStore(path);
    a.toggleBookmark('s1');
    // New instance reads the sidecar from disk.
    const b = new SessionBookmarkStore(path);
    expect(b.isBookmarked('s1')).toBe(true);
  });

  it('adds, removes, and sets tags', () => {
    const store = new SessionBookmarkStore(path);
    store.addTag('s1', 'auth');
    store.addTag('s1', '#WIP');
    expect(store.getTags('s1')).toEqual(['auth', 'wip']);
    store.removeTag('s1', 'auth');
    expect(store.getTags('s1')).toEqual(['wip']);
    store.setTags('s1', ['One', 'two', 'two', '#three']);
    expect(store.getTags('s1')).toEqual(['one', 'two', 'three']);
  });

  it('does not add duplicate tags', () => {
    const store = new SessionBookmarkStore(path);
    store.addTag('s1', 'auth');
    store.addTag('s1', 'auth');
    store.addTag('s1', 'AUTH');
    expect(store.getTags('s1')).toEqual(['auth']);
  });

  it('persists tags across instances', () => {
    const a = new SessionBookmarkStore(path);
    a.setTags('s1', ['auth', 'wip']);
    const b = new SessionBookmarkStore(path);
    expect(b.getTags('s1')).toEqual(['auth', 'wip']);
  });

  it('lists all bookmarked ids', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('s1');
    store.toggleBookmark('s3');
    expect(store.allBookmarked().sort()).toEqual(['s1', 's3']);
  });

  it('lists the union of all tags sorted', () => {
    const store = new SessionBookmarkStore(path);
    store.setTags('s1', ['zebra', 'auth']);
    store.setTags('s2', ['auth', 'beta']);
    expect(store.allTags()).toEqual(['auth', 'beta', 'zebra']);
  });

  it('prunes empty entries on save', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('s1'); // on
    store.toggleBookmark('s1'); // off — now empty
    store.addTag('s2', 'keep');
    // Reload from disk: s1 (empty) should be gone, s2 retained.
    const reloaded = new SessionBookmarkStore(path);
    expect(reloaded.isBookmarked('s1')).toBe(false);
    expect(reloaded.getTags('s2')).toEqual(['keep']);
    const onDisk = JSON.parse(readFileSync(path, 'utf-8'));
    expect(onDisk.s1).toBeUndefined();
    expect(onDisk.s2).toBeDefined();
  });

  it('handles a missing sidecar file gracefully', () => {
    const store = new SessionBookmarkStore(join(dir, 'nonexistent.json'));
    expect(store.isBookmarked('s1')).toBe(false);
    expect(store.allBookmarked()).toEqual([]);
  });

  it('fails closed without overwriting a corrupt sidecar', () => {
    const corrupt = 'not valid json {';
    writeFileSync(path, corrupt);
    const store = new SessionBookmarkStore(path);
    expect(store.isBookmarked('s1')).toBe(false);
    expect(store.toggleBookmark('s1')).toEqual({
      ok: false,
      reason: 'read-failed',
    });
    expect(readFileSync(path, 'utf-8')).toBe(corrupt);
  });

  it('fails closed without overwriting an oversized sidecar', () => {
    const oversized = Buffer.alloc(DASHBOARD_SIDECAR_MAX_BYTES + 1, 0x20);
    writeFileSync(path, oversized);
    const store = new SessionBookmarkStore(path);

    expect(store.allBookmarked()).toEqual([]);
    expect(store.toggleBookmark('s1')).toEqual({
      ok: false,
      reason: 'read-failed',
    });
    expect(readFileSync(path)).toEqual(oversized);
  });

  it('refuses to publish a sidecar larger than the read limit', () => {
    const store = new SessionBookmarkStore(path);
    expect(
      store.setTitle('s1', 'x'.repeat(DASHBOARD_SIDECAR_MAX_BYTES))
    ).toEqual({ ok: false, reason: 'write-failed' });
    expect(existsSync(path)).toBe(false);
    expect(store.getTitle('s1')).toBeUndefined();
  });

  it('creates the sidecar file on first write', () => {
    const store = new SessionBookmarkStore(path);
    expect(existsSync(path)).toBe(false);
    store.toggleBookmark('s1');
    expect(existsSync(path)).toBe(true);
  });

  it('toggles archive and persists it', () => {
    const store = new SessionBookmarkStore(path);
    expect(store.isArchived('s1')).toBe(false);
    expect(store.toggleArchived('s1')).toEqual({ ok: true, value: true });
    expect(store.allArchived()).toEqual(['s1']);
    // Persists across instances even with no bookmark/tags.
    const b = new SessionBookmarkStore(path);
    expect(b.isArchived('s1')).toBe(true);
    // Un-archive prunes the entry entirely.
    expect(b.toggleArchived('s1')).toEqual({ ok: true, value: false });
    const c = new SessionBookmarkStore(path);
    expect(c.isArchived('s1')).toBe(false);
    expect(c.allArchived()).toEqual([]);
  });

  it('archive is independent of bookmark state', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('s1');
    store.toggleArchived('s1');
    expect(store.isBookmarked('s1')).toBe(true);
    expect(store.isArchived('s1')).toBe(true);
    store.toggleArchived('s1');
    expect(store.isBookmarked('s1')).toBe(true);
  });

  describe('singleton', () => {
    afterEach(() => {
      resetSessionBookmarkStore();
      delete process.env.KIRO_TEST_SESSIONS_DIR;
    });

    it('returns the same instance', () => {
      process.env.KIRO_TEST_SESSIONS_DIR = dir;
      resetSessionBookmarkStore();
      const a = getSessionBookmarkStore();
      const b = getSessionBookmarkStore();
      expect(a).toBe(b);
    });
  });
});

describe('SessionBookmarkStore cross-process safety', () => {
  let dir: string;
  let path: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'sd-bm-'));
    path = join(dir, 'dashboard-meta.json');
  });
  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('allUserTouched unions bookmarked, tagged, and archived ids', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('s-book');
    store.addTag('s-tag', 'wip');
    store.toggleArchived('s-arch');
    expect(new Set(store.allUserTouched())).toEqual(
      new Set(['s-book', 's-tag', 's-arch'])
    );
    // Un-marking removes a session from the union.
    store.toggleBookmark('s-book');
    expect(new Set(store.allUserTouched())).toEqual(
      new Set(['s-tag', 's-arch'])
    );
  });

  it('stores, survives reload, and clears the title override', () => {
    const store = new SessionBookmarkStore(path);
    store.setTitle('s-1', '  My   renamed\nsession  ');
    expect(store.getTitle('s-1')).toBe('My renamed session');
    // A renamed session counts as user-touched (cleanup exemption).
    expect(store.allUserTouched()).toContain('s-1');

    const reloaded = new SessionBookmarkStore(path);
    expect(reloaded.getTitle('s-1')).toBe('My renamed session');

    reloaded.setTitle('s-1', '');
    expect(reloaded.getTitle('s-1')).toBeUndefined();
    expect(reloaded.allUserTouched()).not.toContain('s-1');
  });

  it('shares user metadata across bridged V2 and KAS copies', () => {
    const store = new SessionBookmarkStore(path);
    store.setTitle(V2_COPY, 'Renamed conversation');
    store.toggleBookmark(V2_COPY);
    store.setTags(V2_COPY, ['important']);
    store.toggleArchived(V2_COPY);

    expect(store.getTitle(KAS_COPY)).toBe('Renamed conversation');
    expect(store.isBookmarked(KAS_COPY)).toBe(true);
    expect(store.getTags(KAS_COPY)).toEqual(['important']);
    expect(store.isArchived(KAS_COPY)).toBe(true);

    expect(store.toggleBookmark(KAS_COPY)).toEqual({
      ok: true,
      value: false,
    });
    store.setTitle(KAS_COPY, 'Renamed from winner');
    const reloaded = new SessionBookmarkStore(path);
    expect(reloaded.isBookmarked(V2_COPY)).toBe(false);
    expect(reloaded.getTitle(V2_COPY)).toBe('Renamed from winner');
  });

  it("two stores on the same file do not lose each other's writes", () => {
    const a = new SessionBookmarkStore(path);
    const b = new SessionBookmarkStore(path);
    a.load();
    b.load(); // b snapshots an empty file — the stale-overwrite setup
    a.toggleBookmark('s-1');
    b.addTag('s-2', 'wip');
    // Without a re-read before mutate, b's write would have erased a's.
    const fresh = new SessionBookmarkStore(path);
    expect(fresh.isBookmarked('s-1')).toBe(true);
    expect(fresh.getTags('s-2')).toEqual(['wip']);
  });

  it('mutation returns state consistent with what another process wrote', () => {
    const a = new SessionBookmarkStore(path);
    const b = new SessionBookmarkStore(path);
    a.toggleBookmark('s-1'); // on
    // b's snapshot predates a's write; the toggle must still act on disk state.
    expect(b.toggleBookmark('s-1')).toEqual({ ok: true, value: false }); // off, not "on again"
  });

  it('reports an unreadable target and preserves in-memory state', () => {
    const unreadableTarget = join(dir, 'target-directory');
    const { mkdirSync, readdirSync } =
      require('node:fs') as typeof import('node:fs');
    mkdirSync(unreadableTarget);
    const store = new SessionBookmarkStore(unreadableTarget);

    expect(store.toggleBookmark('s-1')).toEqual({
      ok: false,
      reason: 'read-failed',
    });
    expect(store.isBookmarked('s-1')).toBe(false);
    expect(readdirSync(dir).sort()).toEqual(['target-directory']);
  });

  it('does not steal an old lock owned by a live process', () => {
    const lockDir = `${path}.lock`;
    const ownerPath = join(lockDir, 'owner.json');
    mkdirSync(lockDir);
    writeFileSync(
      ownerPath,
      JSON.stringify({ pid: process.pid, token: 'live-owner' })
    );
    const old = new Date(Date.now() - 24 * 60 * 60 * 1000);
    utimesSync(ownerPath, old, old);

    const store = new SessionBookmarkStore(path);
    expect(store.toggleBookmark('s-1')).toEqual({
      ok: false,
      reason: 'locked',
    });
    expect(existsSync(ownerPath)).toBe(true);
  });

  it('recovers an ownerless lock directory left by an older writer', () => {
    const lockDir = `${path}.lock`;
    mkdirSync(lockDir);

    const store = new SessionBookmarkStore(path);
    expect(store.toggleBookmark('s-1')).toEqual({ ok: true, value: true });
    expect(existsSync(lockDir)).toBe(false);
  });

  it('reclaims a lock whose recorded owner is dead', () => {
    const lockDir = `${path}.lock`;
    mkdirSync(lockDir);
    writeFileSync(
      join(lockDir, 'owner.json'),
      JSON.stringify({ pid: 999999999, token: 'dead-owner' })
    );

    const store = new SessionBookmarkStore(path);
    expect(store.toggleBookmark('s-1')).toEqual({ ok: true, value: true });
    expect(existsSync(lockDir)).toBe(false);
  });

  it('does not release a lock when its ownership token differs', () => {
    const lockDir = `${path}.lock`;
    const ownerPath = join(lockDir, 'owner.json');
    mkdirSync(lockDir);
    writeFileSync(
      ownerPath,
      JSON.stringify({ pid: process.pid, token: 'current-owner' })
    );
    const store = new SessionBookmarkStore(path);
    const internals = store as unknown as {
      heldLockToken: string | null;
      releaseLock(): void;
    };
    internals.heldLockToken = 'stale-owner';

    internals.releaseLock();

    expect(existsSync(ownerPath)).toBe(true);
  });

  it('writes leave no temp file behind', () => {
    const a = new SessionBookmarkStore(path);
    a.toggleBookmark('s-1');
    const { readdirSync } = require('node:fs') as typeof import('node:fs');
    expect(readdirSync(dir)).toEqual(['dashboard-meta.json']);
  });
});

describe('SessionBookmarkStore prune', () => {
  let dir: string;
  let path: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'sd-prune-'));
    path = join(dir, 'dashboard-meta.json');
  });
  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('drops entries for sessions missing from the listing', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('alive');
    store.addTag('dead', 'wip');
    store.prune(new Set(['alive']));
    const fresh = new SessionBookmarkStore(path);
    expect(fresh.isBookmarked('alive')).toBe(true);
    expect(fresh.getTags('dead')).toEqual([]);
  });

  it('preserves metadata when an equivalent bridged copy is still live', () => {
    const store = new SessionBookmarkStore(path);
    store.setTitle(V2_COPY, 'Keep this rename');
    store.toggleBookmark(V2_COPY);
    store.prune(new Set([KAS_COPY]));

    const fresh = new SessionBookmarkStore(path);
    expect(fresh.getTitle(KAS_COPY)).toBe('Keep this rename');
    expect(fresh.isBookmarked(KAS_COPY)).toBe(true);
  });

  it('refuses to prune when the listing lost most sessions', () => {
    const store = new SessionBookmarkStore(path);
    for (let i = 0; i < 12; i++) store.toggleBookmark(`s${i}`);
    // A listing with 2 of 12 looks like a failed listing.
    store.prune(new Set(['s0', 's1']));
    const fresh = new SessionBookmarkStore(path);
    expect(fresh.allBookmarked().length).toBe(12);
  });

  it('leaves no lock directory behind', () => {
    const store = new SessionBookmarkStore(path);
    store.toggleBookmark('s-1');
    store.prune(new Set([]));
    const { readdirSync } = require('node:fs') as typeof import('node:fs');
    expect(readdirSync(dir).filter((f) => f.endsWith('.lock'))).toEqual([]);
  });
});

describe('SessionBookmarkStore fresh untouched guard', () => {
  let dir: string;
  let path: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'sd-fresh-'));
    path = join(dir, 'dashboard-meta.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('rejects a fresh mark on an equivalent conversation copy', async () => {
    const writer = new SessionBookmarkStore(path);
    const guard = new SessionBookmarkStore(path);
    writer.addTag(KAS_COPY, 'keep');

    const action = expect.unreachable;
    await expect(guard.runIfFreshUntouched(V2_COPY, action)).resolves.toEqual({
      ok: false,
      reason: 'user-touched',
    });
  });

  it('holds the sidecar lock until the guarded action completes', async () => {
    const guard = new SessionBookmarkStore(path);
    const contender = new SessionBookmarkStore(path);

    const result = await guard.runIfFreshUntouched('session-1', async () => {
      expect(contender.toggleBookmark('session-1')).toEqual({
        ok: false,
        reason: 'locked',
      });
      return 'deleted';
    });

    expect(result).toEqual({ ok: true, value: 'deleted' });
    expect(contender.toggleBookmark('session-1')).toEqual({
      ok: true,
      value: true,
    });
  });
});
