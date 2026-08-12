/**
 * Session bookmarks & tags — a sidecar store of conversation metadata.
 *
 * Bookmarks and tags are the USER's organizational layer over sessions.
 * They live in a single sidecar file keyed by the physical session id that
 * received the edit, deliberately separate from the session files themselves so they:
 *   - never mutate V2 or KAS session metadata (safe, engine-agnostic),
 *   - span all workspaces and both stores uniformly,
 *   - survive independently of how a session was created.
 *
 * Storage: `~/.kiro/sessions/dashboard-meta.json`
 *   `{ "<sessionId>": { "bookmarked": true, "tags": ["auth", "wip"] }, ... }`
 */

import {
  existsSync,
  mkdirSync,
  renameSync,
  rmdirSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import {
  DASHBOARD_SIDECAR_MAX_BYTES,
  LOCK_METADATA_MAX_BYTES,
  readBoundedJson,
} from './bounded-json.js';
import { kiroHomePath } from './kiro-home.js';
import { logger } from './logger.js';
import { conversationKey } from './session-dashboard.js';

/** Per-session user metadata. */
export interface SessionMeta {
  bookmarked: boolean;
  tags: string[];
  /** Archived sessions are hidden from the dashboard list by default. */
  archived?: boolean;
  /** User-set title override — the top slot of the title chain. */
  title?: string;
}

const EMPTY: SessionMeta = { bookmarked: false, tags: [] };

export type BookmarkMutationResult<T = void> =
  | { ok: true; value: T }
  | {
      ok: false;
      reason: 'locked' | 'read-failed' | 'write-failed';
    };

export type FreshUntouchedResult<T> =
  | { ok: true; value: T }
  | { ok: false; reason: 'locked' | 'read-failed' | 'user-touched' };

function cloneMetadata(
  data: ReadonlyMap<string, SessionMeta>
): Map<string, SessionMeta> {
  return new Map(
    [...data].map(([id, meta]) => [id, { ...meta, tags: [...meta.tags] }])
  );
}

function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** Normalize a tag: trimmed, lowercased, spaces→hyphens, no leading '#'. */
export function normalizeTag(raw: string): string {
  return raw.trim().replace(/^#+/, '').toLowerCase().replace(/\s+/g, '-');
}

/** Parse a comma/space separated tag string into a clean, deduped list. */
export function parseTags(input: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of input.split(/[,\s]+/)) {
    const t = normalizeTag(part);
    if (t && !seen.has(t)) {
      seen.add(t);
      out.push(t);
    }
  }
  return out;
}

export class SessionBookmarkStore {
  private data = new Map<string, SessionMeta>();
  private mergedCache = new Map<string, SessionMeta>();
  private path: string;
  private loaded = false;
  private heldLockToken: string | null = null;

  constructor(filePath?: string) {
    this.path =
      filePath ??
      (process.env.KIRO_TEST_SESSIONS_DIR
        ? join(process.env.KIRO_TEST_SESSIONS_DIR, 'dashboard-meta.json')
        : kiroHomePath('sessions', 'dashboard-meta.json'));
  }

  /** Load the sidecar from disk (idempotent; safe if the file is absent). */
  load(): void {
    if (this.loaded) return;
    this.loaded = true;
    this.readFromDisk();
  }

  /** Replace in-memory state only after a complete disk read succeeds. */
  private readFromDisk(): boolean {
    this.mergedCache.clear();
    if (!existsSync(this.path)) {
      this.data.clear();
      return true;
    }
    try {
      const raw = readBoundedJson(this.path, DASHBOARD_SIDECAR_MAX_BYTES);
      const next = new Map<string, SessionMeta>();
      if (raw && typeof raw === 'object') {
        for (const [id, meta] of Object.entries(raw)) {
          const m = meta as Partial<SessionMeta>;
          next.set(id, {
            bookmarked: m.bookmarked === true,
            tags: Array.isArray(m.tags)
              ? m.tags.filter((t): t is string => typeof t === 'string')
              : [],
            ...(m.archived === true ? { archived: true } : {}),
            ...(typeof m.title === 'string' && m.title.trim()
              ? { title: m.title }
              : {}),
          });
        }
      }
      this.data = next;
      return true;
    } catch (err) {
      logger.warn('[session-bookmarks] failed to load sidecar:', err);
      return false;
    }
  }

  /** Apply a mutation against fresh disk state and publish it atomically. */
  private mutate<T>(fn: () => T): BookmarkMutationResult<T> {
    const locked = this.acquireLock();
    if (!locked) {
      logger.warn('[session-bookmarks] lock contention — mutation skipped');
      return { ok: false, reason: 'locked' };
    }
    try {
      this.loaded = true;
      if (!this.readFromDisk()) {
        return { ok: false, reason: 'read-failed' };
      }
      const before = cloneMetadata(this.data);
      const result = fn();
      if (!this.save()) {
        this.data = before;
        return { ok: false, reason: 'write-failed' };
      }
      return { ok: true, value: result };
    } finally {
      // Mutation callbacks edit this.data after the fresh read, so cached
      // merges from before or during the mutation are both stale.
      this.mergedCache.clear();
      this.releaseLock();
    }
  }

  /** Take the sidecar lock; reclaim it only when its recorded owner is dead. */
  private acquireLock(): boolean {
    const lockPath = `${this.path}.lock`;
    const ownerPath = join(lockPath, 'owner.json');
    mkdirSync(dirname(this.path), { recursive: true });
    const deadline = Date.now() + 250;
    for (;;) {
      if (Date.now() > deadline) return false;
      const token = `${process.pid}-${Date.now()}-${Math.random()}`;
      const contenderPath = `${lockPath}.${token}.tmp`;
      const contenderOwnerPath = join(contenderPath, 'owner.json');
      try {
        mkdirSync(contenderPath);
        writeFileSync(
          contenderOwnerPath,
          JSON.stringify({ pid: process.pid, token }),
          { flag: 'wx', mode: 0o600 }
        );
        renameSync(contenderPath, lockPath);
        this.heldLockToken = token;
        return true;
      } catch {
        try {
          unlinkSync(contenderOwnerPath);
        } catch {
          /* no unpublished owner file remains */
        }
        try {
          rmdirSync(contenderPath);
        } catch {
          /* no unpublished contender directory remains */
        }
        try {
          const owner = readBoundedJson(ownerPath, LOCK_METADATA_MAX_BYTES) as {
            pid?: unknown;
            token?: unknown;
          };
          const pid = Number(owner.pid);
          const ownerToken =
            typeof owner.token === 'string' ? owner.token : null;
          if (
            Number.isSafeInteger(pid) &&
            pid > 0 &&
            ownerToken &&
            !isPidAlive(pid)
          ) {
            const current = readBoundedJson(
              ownerPath,
              LOCK_METADATA_MAX_BYTES
            ) as { token?: unknown };
            if (current.token !== ownerToken) continue;
            unlinkSync(ownerPath);
            rmdirSync(lockPath);
            continue;
          }
        } catch {
          try {
            rmdirSync(lockPath);
            continue;
          } catch {
            /* non-empty malformed locks fail closed until the deadline */
          }
        }
        Bun.sleepSync(10);
      }
    }
  }

  private releaseLock(): void {
    const token = this.heldLockToken;
    this.heldLockToken = null;
    if (!token) return;
    const lockPath = `${this.path}.lock`;
    const ownerPath = join(lockPath, 'owner.json');
    try {
      const owner = readBoundedJson(ownerPath, LOCK_METADATA_MAX_BYTES) as {
        token?: unknown;
      };
      if (owner.token !== token) return;
      unlinkSync(ownerPath);
      rmdirSync(lockPath);
    } catch {
      /* ownership changed or was already released */
    }
  }

  /** Persist the sidecar atomically, dropping entries with no data. */
  private save(): boolean {
    const obj: Record<string, SessionMeta> = {};
    for (const [id, meta] of this.data) {
      if (
        meta.bookmarked ||
        meta.tags.length > 0 ||
        meta.archived ||
        meta.title
      ) {
        obj[id] = meta;
      }
    }
    const tmp = `${this.path}.${process.pid}.tmp`;
    try {
      const serialized = JSON.stringify(obj, null, 2);
      if (Buffer.byteLength(serialized) > DASHBOARD_SIDECAR_MAX_BYTES) {
        throw new Error(
          `dashboard sidecar exceeds ${DASHBOARD_SIDECAR_MAX_BYTES} byte limit`
        );
      }
      mkdirSync(dirname(this.path), { recursive: true });
      writeFileSync(tmp, serialized);
      renameSync(tmp, this.path);
      return true;
    } catch (err) {
      try {
        unlinkSync(tmp);
      } catch {
        /* no temporary file was published */
      }
      logger.warn('[session-bookmarks] failed to save sidecar:', err);
      return false;
    }
  }

  private entry(sessionId: string): SessionMeta {
    let meta = this.data.get(sessionId);
    if (!meta) {
      meta = { bookmarked: false, tags: [] };
      this.data.set(sessionId, meta);
    }
    return meta;
  }

  private equivalentEntries(sessionId: string): Array<[string, SessionMeta]> {
    const key = conversationKey(sessionId);
    return [...this.data.entries()].filter(
      ([id]) => conversationKey(id) === key
    );
  }

  private mergedMeta(sessionId: string): SessionMeta {
    const cached = this.mergedCache.get(sessionId);
    if (cached) return cached;
    const merged = this.computeMergedMeta(sessionId);
    this.mergedCache.set(sessionId, merged);
    return merged;
  }

  private computeMergedMeta(sessionId: string): SessionMeta {
    const matches = this.equivalentEntries(sessionId);
    if (matches.length === 0) return EMPTY;
    const exact = this.data.get(sessionId);
    const tags = new Set<string>();
    let bookmarked = false;
    let archived = false;
    for (const [, meta] of matches) {
      bookmarked ||= meta.bookmarked;
      archived ||= meta.archived === true;
      for (const tag of meta.tags) tags.add(tag);
    }
    const title =
      exact?.title ?? matches.find(([, meta]) => meta.title)?.[1].title;
    return {
      bookmarked,
      tags: [...tags],
      ...(archived ? { archived: true } : {}),
      ...(title ? { title } : {}),
    };
  }

  /** Read-only metadata merged across physical copies of one conversation. */
  get(sessionId: string): SessionMeta {
    this.load();
    return this.mergedMeta(sessionId);
  }

  isBookmarked(sessionId: string): boolean {
    return this.get(sessionId).bookmarked;
  }

  /** Toggle a bookmark across every physical copy of the conversation. */
  toggleBookmark(sessionId: string): BookmarkMutationResult<boolean> {
    return this.mutate(() => {
      const matches = this.equivalentEntries(sessionId);
      const bookmarked = matches.some(([, meta]) => meta.bookmarked);
      for (const [, meta] of matches) meta.bookmarked = false;
      if (!bookmarked) this.entry(sessionId).bookmarked = true;
      return !bookmarked;
    });
  }

  getTags(sessionId: string): string[] {
    return this.get(sessionId).tags;
  }

  /** Replace tags across every physical copy with one canonical edit. */
  setTags(sessionId: string, tags: string[]): BookmarkMutationResult {
    const clean: string[] = [];
    const seen = new Set<string>();
    for (const raw of tags) {
      const t = normalizeTag(raw);
      if (t && !seen.has(t)) {
        seen.add(t);
        clean.push(t);
      }
    }
    return this.mutate(() => {
      for (const [, meta] of this.equivalentEntries(sessionId)) meta.tags = [];
      this.entry(sessionId).tags = clean;
    });
  }

  addTag(sessionId: string, tag: string): BookmarkMutationResult {
    const t = normalizeTag(tag);
    if (!t) return { ok: true, value: undefined };
    return this.mutate(() => {
      const tags = new Set(this.mergedMeta(sessionId).tags);
      tags.add(t);
      for (const [, meta] of this.equivalentEntries(sessionId)) meta.tags = [];
      this.entry(sessionId).tags = [...tags];
    });
  }

  removeTag(sessionId: string, tag: string): BookmarkMutationResult {
    const t = normalizeTag(tag);
    return this.mutate(() => {
      const tags = this.mergedMeta(sessionId).tags.filter((x) => x !== t);
      for (const [, meta] of this.equivalentEntries(sessionId)) meta.tags = [];
      if (tags.length > 0) this.entry(sessionId).tags = tags;
    });
  }

  /** Physical session ids that hold a bookmark mark. */
  allBookmarked(): string[] {
    this.load();
    const out: string[] = [];
    for (const [id, meta] of this.data) if (meta.bookmarked) out.push(id);
    return out;
  }

  isArchived(sessionId: string): boolean {
    return this.get(sessionId).archived === true;
  }

  /** Toggle archived state across every physical copy of the conversation. */
  toggleArchived(sessionId: string): BookmarkMutationResult<boolean> {
    return this.mutate(() => {
      const matches = this.equivalentEntries(sessionId);
      const archived = matches.some(([, meta]) => meta.archived === true);
      for (const [, meta] of matches) delete meta.archived;
      if (!archived) this.entry(sessionId).archived = true;
      return !archived;
    });
  }

  /** Physical session ids that hold an archive mark. */
  allArchived(): string[] {
    this.load();
    const out: string[] = [];
    for (const [id, meta] of this.data) if (meta.archived) out.push(id);
    return out;
  }

  /** User-set title override from any physical copy of the conversation. */
  getTitle(sessionId: string): string | undefined {
    return this.get(sessionId).title;
  }

  /** Set one title override and clear stale overrides from physical copies. */
  setTitle(sessionId: string, title: string): BookmarkMutationResult {
    const clean = title.replace(/\s+/g, ' ').trim();
    return this.mutate(() => {
      for (const [, meta] of this.equivalentEntries(sessionId)) {
        delete meta.title;
      }
      if (clean) this.entry(sessionId).title = clean;
    });
  }

  /** Physical session ids with any user mark at all. */
  allUserTouched(): string[] {
    this.load();
    const out: string[] = [];
    for (const [id, meta] of this.data) {
      if (
        meta.bookmarked ||
        meta.tags.length > 0 ||
        meta.archived ||
        meta.title
      ) {
        out.push(id);
      }
    }
    return out;
  }

  /**
   * Drop metadata only when neither its physical session nor an equivalent
   * conversation copy exists. A listing that lost more than half of the
   * marked sessions is treated as a failed listing, not a mass deletion.
   */
  prune(liveIds: ReadonlySet<string>): BookmarkMutationResult {
    this.load();
    const liveKeys = new Set([...liveIds].map(conversationKey));
    const dead = [...this.data.keys()].filter(
      (id) => !liveIds.has(id) && !liveKeys.has(conversationKey(id))
    );
    if (dead.length === 0) return { ok: true, value: undefined };
    if (this.data.size > 10 && dead.length > this.data.size / 2) {
      return { ok: true, value: undefined };
    }
    return this.mutate(() => {
      for (const id of dead) this.data.delete(id);
    });
  }

  /** Hold sidecar ownership while acting only on a freshly unmarked conversation. */
  async runIfFreshUntouched<T>(
    sessionId: string,
    action: () => Promise<T> | T
  ): Promise<FreshUntouchedResult<T>> {
    if (!this.acquireLock()) {
      return { ok: false, reason: 'locked' };
    }
    try {
      this.loaded = true;
      if (!this.readFromDisk()) {
        return { ok: false, reason: 'read-failed' };
      }
      const meta = this.mergedMeta(sessionId);
      if (
        meta.bookmarked ||
        meta.tags.length > 0 ||
        meta.archived ||
        meta.title
      ) {
        return { ok: false, reason: 'user-touched' };
      }
      return { ok: true, value: await action() };
    } finally {
      this.releaseLock();
    }
  }

  /** Union of every tag in use (sorted), for filtering/autocomplete. */
  allTags(): string[] {
    this.load();
    const set = new Set<string>();
    for (const meta of this.data.values())
      for (const t of meta.tags) set.add(t);
    return [...set].sort();
  }
}

let globalStore: SessionBookmarkStore | null = null;

export function getSessionBookmarkStore(): SessionBookmarkStore {
  if (!globalStore) {
    globalStore = new SessionBookmarkStore();
    globalStore.load();
  }
  return globalStore;
}

export function resetSessionBookmarkStore(): void {
  globalStore = null;
}
