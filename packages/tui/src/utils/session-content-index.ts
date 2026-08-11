/**
 * On-disk FTS5 content index for session search.
 *
 * Indexes session titles and user prompts (not responses or tool output) in
 * a SQLite FTS5 table beside the session stores, so search does not require
 * holding transcript text in memory. The index is disposable: any schema
 * mismatch rebuilds its tables transactionally, and losing it costs only a rebuild.
 *
 * Every reader/writer degrades to titles-only search rather than failing:
 * FTS5 missing from the SQLite build, a remote filesystem (WAL needs local
 * shared memory), or `KIRO_DISABLE_SESSION_SEARCH_INDEX=1` (an off switch,
 * because the index is a plaintext copy of prompt text).
 */

import { Database } from 'bun:sqlite';
import {
  closeSync,
  fstatSync,
  lstatSync,
  mkdirSync,
  openSync,
  statSync,
  statfsSync,
} from 'node:fs';
import { dirname } from 'node:path';
import { readFileRange } from './bounded-json.js';

const SCHEMA_VERSION = 5;
/** Byte cap on indexed prompt text per session — a high safety limit only. */
const PROMPT_CAP_BYTES = 256 * 1024;
/** Oversized JSONL records are skipped without retaining their full bytes. */
const JSONL_RECORD_MAX_BYTES = 1024 * 1024;
const HEAD_BYTES = 256;
/** Window hashed before the mark offset to prove the indexed tail survived. */
const TAIL_WINDOW = 4096;

/** What the field can honestly claim it searches. */
export type Coverage = 'titles-and-prompts' | 'titles-only';

export type IndexHandle =
  | { mode: 'content'; db: Database; coverage: Coverage }
  | {
      mode: 'titles';
      reason: 'unavailable' | 'remote-filesystem' | 'disabled';
    };

export interface SessionRef {
  id: string;
  transcriptPath: string;
  title: string;
  mtimeMs: number;
  size?: number;
  ctimeMs?: number;
  titleHash?: string;
  transcriptState?: 'present' | 'missing' | 'unknown';
  engine?: 'v2' | 'v3';
}

export interface Hit {
  id: string;
  snippet: string;
  score: number;
}

/** Records where extraction stopped, so a grown transcript is only tail-read. */
export interface Mark {
  revision: number;
  bytes: number;
  mtimeMs: number;
  size: number;
  ctimeMs: number;
  titleHash: string;
  headHash: string;
  /** Hash of the window ending at `bytes` — proves the indexed tail survived. */
  tailHash: string;
  capped: boolean;
  parseComplete: boolean;
}

/**
 * FTS5 is a compile-time option, so its presence is a runtime fact rather
 * than something the type system or the lockfile can promise.
 */
function hasFts5(): boolean {
  try {
    const probe = new Database(':memory:');
    try {
      return probe
        .query<{ v: string }, []>('PRAGMA compile_options')
        .all()
        .some((r) => String(Object.values(r)[0]).includes('FTS5'));
    } finally {
      probe.close();
    }
  } catch {
    return false;
  }
}

/**
 * WAL relies on shared memory the local filesystem provides and remote ones
 * do not. The filesystem type comes back as a magic number; the remote ones
 * are enumerated because the set of local ones is open-ended.
 */
const REMOTE_FS_TYPES = new Set([
  0x6969, // NFS
  0xff534d42, // CIFS/SMB
  0xfe534d42, // SMB2
  0x73757246, // sshfs / FUSE-based remote
  0x65735546, // FUSE
]);

/**
 * WAL must actually engage — a filesystem that cannot host it reports a
 * different resulting mode. This functional signal works on every platform,
 * unlike the magic-number check below, whose values are Linux-specific.
 */
function walEngaged(db: Database): boolean {
  const row = db.query('PRAGMA journal_mode = WAL').get() ?? {};
  return String(Object.values(row)[0]).toLowerCase() === 'wal';
}

function isLocal(path: string): boolean {
  try {
    const { type } = statfsSync(dirname(path));
    return !REMOTE_FS_TYPES.has(type);
  } catch {
    // Unknown is treated as local: refusing to index is worse than a
    // rebuildable index.
    return true;
  }
}

export function openIndex(path: string, enabled = true): IndexHandle {
  if (!enabled || process.env.KIRO_DISABLE_SESSION_SEARCH_INDEX === '1') {
    return { mode: 'titles', reason: 'disabled' };
  }
  if (!hasFts5()) return { mode: 'titles', reason: 'unavailable' };
  if (!isLocal(path)) return { mode: 'titles', reason: 'remote-filesystem' };

  let db: Database | null = null;
  try {
    mkdirSync(dirname(path), { recursive: true });
    try {
      if (lstatSync(path).isSymbolicLink()) {
        return { mode: 'titles', reason: 'unavailable' };
      }
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== 'ENOENT') throw err;
    }
    db = new Database(path, { create: true });
    if (!walEngaged(db)) {
      db.close();
      return { mode: 'titles', reason: 'remote-filesystem' };
    }
    db.run('PRAGMA synchronous = NORMAL');
    db.run('PRAGMA busy_timeout = 500');
    initializeSchema(db);
    return { mode: 'content', db, coverage: 'titles-and-prompts' };
  } catch {
    try {
      db?.close();
    } catch {
      /* already closed */
    }
    return { mode: 'titles', reason: 'unavailable' };
  }
}

function initializeSchema(db: Database): void {
  db.run('BEGIN EXCLUSIVE');
  try {
    const version = Number(
      Object.values(db.query('PRAGMA user_version').get() ?? {})[0] ?? 0
    );
    if (version !== SCHEMA_VERSION) {
      db.run('DROP TABLE IF EXISTS sessions');
      db.run('DROP TABLE IF EXISTS marks');
    }
    db.run(`CREATE VIRTUAL TABLE IF NOT EXISTS sessions USING fts5(
              sid UNINDEXED, title, prompts, tokenize='unicode61')`);
    db.run(`CREATE TABLE IF NOT EXISTS marks (
              sid TEXT PRIMARY KEY, bytes INTEGER NOT NULL,
              mtime_ms INTEGER NOT NULL, size INTEGER NOT NULL,
              ctime_ms INTEGER NOT NULL, title_hash TEXT NOT NULL,
              head_hash TEXT NOT NULL, tail_hash TEXT NOT NULL,
              capped INTEGER NOT NULL, prompt_count INTEGER NOT NULL DEFAULT 0,
              parse_complete INTEGER NOT NULL DEFAULT 0,
              revision INTEGER NOT NULL DEFAULT 0)`);
    if (version !== SCHEMA_VERSION) {
      db.run(`PRAGMA user_version = ${SCHEMA_VERSION}`);
    }
    db.run('COMMIT');
  } catch (err) {
    try {
      db.run('ROLLBACK');
    } catch {
      /* the transaction is already gone */
    }
    throw err;
  }
}

export function closeIndex(handle: IndexHandle): void {
  if (handle.mode !== 'content') return;
  try {
    handle.db.close();
  } catch {
    /* already closed */
  }
}

function readMarks(db: Database): Map<string, Mark> {
  const rows = db
    .query<
      {
        sid: string;
        revision: number;
        bytes: number;
        mtime_ms: number;
        size: number;
        ctime_ms: number;
        title_hash: string;
        head_hash: string;
        tail_hash: string;
        capped: number;
        parse_complete: number;
      },
      []
    >(
      'SELECT sid, revision, bytes, mtime_ms, size, ctime_ms, title_hash, head_hash, tail_hash, capped, parse_complete FROM marks'
    )
    .all();
  return new Map(
    rows.map((r) => [
      r.sid,
      {
        revision: r.revision,
        bytes: r.bytes,
        mtimeMs: r.mtime_ms,
        size: r.size,
        ctimeMs: r.ctime_ms,
        titleHash: r.title_hash,
        headHash: r.head_hash,
        tailHash: r.tail_hash,
        capped: !!r.capped,
        parseComplete: !!r.parse_complete,
      },
    ])
  );
}

function readRange(fd: number | null, from: number, length: number): Buffer {
  if (fd === null || length <= 0) return Buffer.alloc(0);
  return readFileRange(fd, from, length);
}

function appendTailWindow(current: Buffer, bytes: Buffer): Buffer {
  if (bytes.length >= TAIL_WINDOW) {
    return Buffer.from(bytes.subarray(bytes.length - TAIL_WINDOW));
  }
  const keep = Math.min(current.length, TAIL_WINDOW - bytes.length);
  return Buffer.concat([current.subarray(current.length - keep), bytes]);
}

function truncateUtf8(value: string, maxBytes: number): string {
  const bytes = Buffer.from(value);
  if (bytes.length <= maxBytes) return value;
  return bytes
    .subarray(0, maxBytes)
    .toString('utf-8')
    .replace(/\uFFFD+$/, '');
}

function appendCappedPrompts(
  current: string,
  prompts: string[],
  alreadyCapped: boolean
): { text: string; capped: boolean } {
  let text = current;
  let capped = alreadyCapped;
  for (const prompt of prompts) {
    if (capped) continue;
    const separator = text ? '\n' : '';
    const available = PROMPT_CAP_BYTES - Buffer.byteLength(text);
    const addition = separator + prompt;
    if (Buffer.byteLength(addition) <= available) {
      text += addition;
      continue;
    }
    text += truncateUtf8(addition, Math.max(available, 0));
    capped = true;
  }
  return { text, capped };
}

/**
 * Extracts user prompt text from V2 (`kind: Prompt`) and KAS (`type: user`)
 * lines. `parsedAll` is false when any line failed to parse — on a tail read
 * that means the append-only assumption is suspect and the file is re-read.
 */
function promptsFromLines(lines: string[]): {
  prompts: string[];
  parsedAll: boolean;
} {
  const out: string[] = [];
  let parsedAll = true;
  for (const line of lines) {
    let entry: unknown;
    try {
      entry = JSON.parse(line);
    } catch {
      parsedAll = false;
      continue;
    }
    const e = entry as {
      kind?: string;
      data?: { content?: unknown };
      payload?: { type?: string; content?: unknown };
      type?: string;
      content?: unknown;
    };
    if (e.kind === 'Prompt') {
      const text = flatten(e.data?.content);
      if (text) out.push(text);
      continue;
    }
    const payload = e.payload ?? e;
    if ((payload as { type?: string }).type === 'user') {
      const text = flatten((payload as { content?: unknown }).content);
      if (text) out.push(text);
    }
  }
  return { prompts: out, parsedAll };
}

function promptsFromCompleteBytes(bytes: Buffer): {
  prompts: string[];
  parsedAll: boolean;
} {
  const prompts: string[] = [];
  let parsedAll = true;
  let start = 0;
  while (start < bytes.length) {
    const end = bytes.indexOf(0x0a, start);
    if (end < 0) break;
    const length = end - start;
    if (length > JSONL_RECORD_MAX_BYTES) {
      parsedAll = false;
    } else if (length > 0) {
      const parsed = promptsFromLines([
        bytes.subarray(start, end).toString('utf-8'),
      ]);
      prompts.push(...parsed.prompts);
      parsedAll &&= parsed.parsedAll;
    }
    start = end + 1;
  }
  return { prompts, parsedAll };
}

function flatten(content: unknown): string {
  if (typeof content === 'string') return content.trim();
  if (!Array.isArray(content)) return '';
  return content
    .map((b) => {
      if (!b || typeof b !== 'object') return '';
      // V2 blocks: { kind: 'text', data: '…' }.
      if ((b as { kind?: string }).kind === 'text') {
        return String((b as { data?: unknown }).data ?? '');
      }
      // KAS blocks: { type: 'text', text: '…' }.
      if ((b as { type?: string }).type === 'text') {
        return String((b as { text?: unknown }).text ?? '');
      }
      return '';
    })
    .join(' ')
    .trim();
}

export interface ReconcileResult {
  added: number;
  updated: number;
  removed: number;
  skipped: 'busy' | 'conflict' | null;
  /** False when a byte budget stopped the pass early — call again to resume. */
  done: boolean;
}

type PendingPhase = 'validate-head' | 'validate-tail' | 'read';

interface FileIdentity {
  dev: bigint;
  ino: bigint;
  size: bigint;
  mtimeNs: bigint;
  ctimeNs: bigint;
}

interface PendingSession {
  readonly ref: SessionRef;
  readonly mark: Mark | undefined;
  fd: number | null;
  readonly stat: { size: number; mtimeMs: number } | null;
  readonly identity: FileIdentity | null;
  phase: PendingPhase;
  validationOffset: number;
  validationEnd: number;
  head: Buffer;
  validationTail: Buffer;
  appendOnly: boolean;
  useAppend: boolean;
  offset: number;
  completeEnd: number;
  fragment: Buffer;
  discardingOversizedRecord: boolean;
  oversizedTail: Buffer;
  tail: Buffer;
  promptText: string;
  promptCount: number;
  parsedAll: boolean;
  capped: boolean;
  contentInitialized: boolean;
}

export interface ReconcileState {
  readonly marks: Map<string, Mark>;
  readonly work: Array<{ ref: SessionRef; mark: Mark | undefined }>;
  readonly gone: string[];
  cursor: number;
  removalsApplied: boolean;
  pending: PendingSession | null;
}

function startRead(
  pending: PendingSession,
  from: number,
  useAppend: boolean,
  tail: Buffer
): void {
  pending.phase = 'read';
  pending.appendOnly = useAppend;
  pending.useAppend = useAppend;
  pending.offset = from;
  pending.completeEnd = from;
  pending.fragment = Buffer.alloc(0);
  pending.discardingOversizedRecord = false;
  pending.oversizedTail = Buffer.alloc(0);
  pending.tail = Buffer.from(tail);
  pending.promptText = '';
  pending.promptCount = 0;
  pending.parsedAll = true;
  pending.capped = false;
  pending.contentInitialized = false;
}

function readFileIdentity(path: string): FileIdentity | null {
  try {
    const stat = statSync(path, { bigint: true });
    return {
      dev: stat.dev,
      ino: stat.ino,
      size: stat.size,
      mtimeNs: stat.mtimeNs,
      ctimeNs: stat.ctimeNs,
    };
  } catch {
    return null;
  }
}

function readDescriptorIdentity(fd: number): FileIdentity | null {
  try {
    const stat = fstatSync(fd, { bigint: true });
    return {
      dev: stat.dev,
      ino: stat.ino,
      size: stat.size,
      mtimeNs: stat.mtimeNs,
      ctimeNs: stat.ctimeNs,
    };
  } catch {
    return null;
  }
}

function closePendingSession(pending: PendingSession | null): void {
  if (!pending || pending.fd === null) return;
  try {
    closeSync(pending.fd);
  } catch {
    // The descriptor may already be invalidated by an I/O failure.
  }
  pending.fd = null;
}

export function disposeReconcileState(state: ReconcileState): void {
  closePendingSession(state.pending);
  state.pending = null;
}

function nanosecondsToMilliseconds(value: bigint): number {
  return Number(value / 1_000_000n);
}

function sameFileIdentity(
  left: FileIdentity | null,
  right: FileIdentity | null
): boolean {
  return (
    left?.dev === right?.dev &&
    left?.ino === right?.ino &&
    left?.size === right?.size &&
    left?.mtimeNs === right?.mtimeNs &&
    left?.ctimeNs === right?.ctimeNs
  );
}

function createPendingSession(
  ref: SessionRef,
  mark: Mark | undefined
): PendingSession {
  let fd: number | null = null;
  let identity: FileIdentity | null = null;
  if (ref.transcriptState !== 'missing') {
    try {
      fd = openSync(ref.transcriptPath, 'r');
      identity = readDescriptorIdentity(fd);
      if (!identity) {
        closeSync(fd);
        fd = null;
      }
    } catch {
      if (fd !== null) closeSync(fd);
      fd = null;
    }
  }
  const stat = identity
    ? {
        size: Number(identity.size),
        mtimeMs: nanosecondsToMilliseconds(identity.mtimeNs),
      }
    : null;

  const canValidateAppend =
    stat !== null &&
    mark !== undefined &&
    stat.size >= mark.bytes &&
    mark.bytes <= HEAD_BYTES + TAIL_WINDOW;
  const pending: PendingSession = {
    ref,
    mark,
    fd,
    stat,
    identity,
    phase: canValidateAppend ? 'validate-head' : 'read',
    validationOffset: 0,
    validationEnd: canValidateAppend
      ? Math.min(HEAD_BYTES, stat?.size ?? 0)
      : 0,
    head: Buffer.alloc(0),
    validationTail: Buffer.alloc(0),
    appendOnly: false,
    useAppend: false,
    offset: 0,
    completeEnd: 0,
    fragment: Buffer.alloc(0),
    discardingOversizedRecord: false,
    oversizedTail: Buffer.alloc(0),
    tail: Buffer.alloc(0),
    promptText: '',
    promptCount: 0,
    parsedAll: true,
    capped: false,
    contentInitialized: false,
  };
  if (!canValidateAppend) startRead(pending, 0, false, Buffer.alloc(0));
  return pending;
}

export function createReconcileState(
  handle: IndexHandle,
  sessions: SessionRef[],
  marks?: Map<string, Mark>,
  removeMissing = true
): ReconcileState {
  const currentMarks =
    marks ?? (handle.mode === 'content' ? readMarks(handle.db) : new Map());
  const live = new Set(sessions.map((s) => s.id));
  const gone = removeMissing
    ? [...currentMarks.keys()].filter((id) => !live.has(id))
    : [];
  const work: ReconcileState['work'] = [];
  for (const ref of sessions) {
    if (ref.transcriptState === 'unknown') continue;
    const mark = currentMarks.get(ref.id);
    const titleHash = ref.titleHash ?? Bun.hash(ref.title).toString(36);
    if (
      !mark ||
      mark.mtimeMs !== ref.mtimeMs ||
      mark.size !== (ref.size ?? mark.size) ||
      mark.ctimeMs !== (ref.ctimeMs ?? mark.ctimeMs) ||
      mark.titleHash !== titleHash
    ) {
      work.push({ ref: { ...ref, titleHash }, mark });
    }
  }
  return {
    marks: currentMarks,
    work,
    gone,
    cursor: 0,
    removalsApplied: false,
    pending: null,
  };
}

/**
 * Brings the index in line with the sessions that exist. Idempotent, so a
 * window that cannot take the write lock loses nothing by declining, and a
 * budget-interrupted pass resumes exactly where it stopped (marks record
 * per-session progress).
 *
 * `byteBudget` caps how many transcript bytes one call may read, bounding
 * the stall a cold build can put on the calling thread. Removals are not
 * budgeted — they are cheap and must see the full listing.
 */
function isDatabaseBusy(error: unknown): boolean {
  const code = (error as { code?: unknown })?.code;
  const message = error instanceof Error ? error.message : String(error);
  return (
    code === 'SQLITE_BUSY' ||
    code === 'SQLITE_LOCKED' ||
    /database (?:is )?(?:busy|locked)/i.test(message)
  );
}

export function reconcile(
  handle: IndexHandle,
  sessions: SessionRef[],
  opts?: {
    byteBudget?: number;
    timeBudgetMs?: number;
    /** Delete indexed rows absent from this complete session listing. */
    removeMissing?: boolean;
    /** Reuse across slices to avoid rereading the full marks table. */
    marks?: Map<string, Mark>;
    /** Reused work plan and cursor for multi-slice reconciliation. */
    state?: ReconcileState;
  }
): ReconcileResult {
  const sliceStart = performance.now();
  const result: ReconcileResult = {
    added: 0,
    updated: 0,
    removed: 0,
    skipped: null,
    done: true,
  };
  if (handle.mode !== 'content') return result;
  const { db } = handle;

  const state =
    opts?.state ??
    createReconcileState(
      handle,
      sessions,
      opts?.marks,
      opts?.removeMissing ?? true
    );
  const { marks, work, gone } = state;
  const hasPendingRemovals = !state.removalsApplied && gone.length > 0;
  if (
    state.cursor >= work.length &&
    state.pending === null &&
    !hasPendingRemovals
  ) {
    return result;
  }

  try {
    db.run('BEGIN IMMEDIATE');
  } catch (error) {
    if (isDatabaseBusy(error)) {
      return { ...result, skipped: 'busy', done: false };
    }
    throw error;
  }

  try {
    const del = db.prepare('DELETE FROM sessions WHERE sid = ?');
    const ins = db.prepare(
      'INSERT INTO sessions(sid, title, prompts) VALUES(?, ?, ?)'
    );
    const insertMark = db.prepare(
      `INSERT INTO marks(sid, revision, bytes, mtime_ms, size, ctime_ms, title_hash, head_hash, tail_hash, capped, prompt_count, parse_complete)
       VALUES(?,1,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(sid) DO NOTHING`
    );
    const updateMark = db.prepare(
      `UPDATE marks SET revision=revision+1, bytes=?, mtime_ms=?, size=?, ctime_ms=?,
         title_hash=?, head_hash=?, tail_hash=?, capped=?, prompt_count=?, parse_complete=?
       WHERE sid=? AND revision=?`
    );
    const dropMark = db.prepare(
      'DELETE FROM marks WHERE sid = ? AND revision = ?'
    );
    const readPrompts = db.prepare<{ prompts: string }, [string]>(
      'SELECT prompts FROM sessions WHERE sid = ?'
    );
    const readPromptCount = db.prepare<{ prompt_count: number }, [string]>(
      'SELECT prompt_count FROM marks WHERE sid = ?'
    );

    const deletedMarks: string[] = [];
    const updatedMarks = new Map<string, Mark>();
    if (!state.removalsApplied) {
      for (const id of gone) {
        const mark = marks.get(id);
        if (!mark) continue;
        const removed = dropMark.run(id, mark.revision).changes > 0;
        if (!removed) continue;
        del.run(id);
        deletedMarks.push(id);
        result.removed++;
      }
    }

    const byteBudget =
      opts?.byteBudget === undefined
        ? Number.POSITIVE_INFINITY
        : Math.max(0, Math.floor(opts.byteBudget));
    let bytesRead = 0;
    let nextCursor = state.cursor;
    let stopped = false;

    while (nextCursor < work.length) {
      const item = work[nextCursor]!;
      const pending =
        state.pending ?? createPendingSession(item.ref, item.mark);
      state.pending = pending;

      if (pending.phase === 'validate-head') {
        if (pending.validationOffset < pending.validationEnd) {
          if (
            bytesRead >= byteBudget ||
            (opts?.timeBudgetMs !== undefined &&
              performance.now() - sliceStart >= opts.timeBudgetMs)
          ) {
            stopped = true;
            break;
          }
          const requested = Math.min(
            64 * 1024,
            pending.validationEnd - pending.validationOffset,
            byteBudget - bytesRead
          );
          const chunk = readRange(
            pending.fd,
            pending.validationOffset,
            requested
          );
          pending.validationOffset += chunk.length;
          bytesRead += chunk.length;
          pending.head = Buffer.concat([pending.head, chunk]);
          if (chunk.length < requested) {
            pending.validationEnd = pending.validationOffset;
          }
          continue;
        }

        const headMatches =
          Bun.hash(pending.head).toString(36) === pending.mark?.headHash;
        if (!headMatches) {
          startRead(pending, 0, false, Buffer.alloc(0));
          continue;
        }
        if (pending.mark?.bytes === 0) {
          startRead(pending, 0, false, Buffer.alloc(0));
          continue;
        }
        pending.phase = 'validate-tail';
        pending.validationOffset = Math.max(
          0,
          pending.mark!.bytes - TAIL_WINDOW
        );
        pending.validationEnd = pending.mark!.bytes;
        continue;
      }

      if (pending.phase === 'validate-tail') {
        if (pending.validationOffset < pending.validationEnd) {
          if (
            bytesRead >= byteBudget ||
            (opts?.timeBudgetMs !== undefined &&
              performance.now() - sliceStart >= opts.timeBudgetMs)
          ) {
            stopped = true;
            break;
          }
          const requested = Math.min(
            64 * 1024,
            pending.validationEnd - pending.validationOffset,
            byteBudget - bytesRead
          );
          const chunk = readRange(
            pending.fd,
            pending.validationOffset,
            requested
          );
          pending.validationOffset += chunk.length;
          bytesRead += chunk.length;
          pending.validationTail = Buffer.concat([
            pending.validationTail,
            chunk,
          ]);
          if (chunk.length < requested) {
            pending.validationEnd = pending.validationOffset;
          }
          continue;
        }

        const tailMatches =
          pending.validationTail.at(-1) === 0x0a &&
          Bun.hash(pending.validationTail).toString(36) ===
            pending.mark?.tailHash;
        if (tailMatches) {
          startRead(pending, pending.mark!.bytes, true, pending.validationTail);
        } else {
          startRead(pending, 0, false, Buffer.alloc(0));
        }
        continue;
      }

      if (!pending.contentInitialized) {
        if (pending.useAppend) {
          pending.promptText = readPrompts.get(pending.ref.id)?.prompts ?? '';
          pending.promptCount =
            readPromptCount.get(pending.ref.id)?.prompt_count ?? 0;
          pending.capped = pending.mark?.capped ?? false;
        }
        pending.contentInitialized = true;
      }

      const snapshotSize = pending.stat?.size ?? 0;
      if (pending.offset < snapshotSize) {
        if (
          bytesRead >= byteBudget ||
          (opts?.timeBudgetMs !== undefined &&
            performance.now() - sliceStart >= opts.timeBudgetMs)
        ) {
          stopped = true;
          break;
        }
        const requested = Math.min(
          64 * 1024,
          snapshotSize - pending.offset,
          byteBudget - bytesRead
        );
        const chunk = readRange(pending.fd, pending.offset, requested);
        const chunkStart = pending.offset;
        pending.offset += chunk.length;
        bytesRead += chunk.length;

        const requiredHeadBytes = Math.min(HEAD_BYTES, snapshotSize);
        if (pending.head.length < requiredHeadBytes) {
          const needed = requiredHeadBytes - pending.head.length;
          pending.head = Buffer.concat([
            pending.head,
            chunk.subarray(0, needed),
          ]);
        }

        let parseChunk = chunk;
        let parseChunkStart = chunkStart;
        if (pending.discardingOversizedRecord) {
          const newline = chunk.indexOf(0x0a);
          if (newline < 0) {
            pending.oversizedTail = appendTailWindow(
              pending.oversizedTail,
              chunk
            );
            continue;
          }
          pending.oversizedTail = appendTailWindow(
            pending.oversizedTail,
            chunk.subarray(0, newline + 1)
          );
          pending.tail = appendTailWindow(pending.tail, pending.oversizedTail);
          pending.completeEnd = chunkStart + newline + 1;
          pending.discardingOversizedRecord = false;
          pending.oversizedTail = Buffer.alloc(0);
          parseChunk = chunk.subarray(newline + 1);
          parseChunkStart = chunkStart + newline + 1;
        }

        if (parseChunk.length === 0) continue;
        const combined = pending.fragment.length
          ? Buffer.concat([pending.fragment, parseChunk])
          : parseChunk;
        const cut = combined.lastIndexOf(0x0a);
        if (cut >= 0) {
          const complete = combined.subarray(0, cut + 1);
          const parsed = promptsFromCompleteBytes(complete);
          const appended = appendCappedPrompts(
            pending.promptText,
            parsed.prompts,
            pending.capped
          );
          pending.promptText = appended.text;
          pending.capped = appended.capped;
          pending.promptCount += parsed.prompts.length;
          pending.parsedAll &&= parsed.parsedAll;
          pending.tail = appendTailWindow(pending.tail, complete);
          pending.completeEnd =
            parseChunkStart - pending.fragment.length + cut + 1;
          pending.fragment = Buffer.from(combined.subarray(cut + 1));
        } else {
          pending.fragment = Buffer.from(combined);
        }
        if (pending.fragment.length > JSONL_RECORD_MAX_BYTES) {
          pending.discardingOversizedRecord = true;
          pending.oversizedTail = appendTailWindow(
            Buffer.alloc(0),
            pending.fragment
          );
          pending.fragment = Buffer.alloc(0);
          pending.parsedAll = false;
        }
        if (chunk.length < requested) pending.offset = snapshotSize;
        continue;
      }

      if (pending.appendOnly && !pending.parsedAll) {
        startRead(pending, 0, false, Buffer.alloc(0));
        continue;
      }

      const descriptorIdentity =
        pending.fd === null ? null : readDescriptorIdentity(pending.fd);
      const pathIdentity = readFileIdentity(pending.ref.transcriptPath);
      if (
        !sameFileIdentity(pending.identity, descriptorIdentity) ||
        !sameFileIdentity(pending.identity, pathIdentity)
      ) {
        closePendingSession(pending);
        if (pathIdentity) {
          state.pending = createPendingSession(
            pending.ref,
            readMarks(db).get(pending.ref.id)
          );
          continue;
        }
        state.pending = null;
        nextCursor++;
        continue;
      }

      const head = pending.stat ? Bun.hash(pending.head).toString(36) : '';
      const tailHash =
        pending.completeEnd > 0 ? Bun.hash(pending.tail).toString(36) : '';
      const mtimeMs = pending.identity
        ? nanosecondsToMilliseconds(pending.identity.mtimeNs)
        : pending.ref.mtimeMs;
      const ctimeMs = pending.identity
        ? nanosecondsToMilliseconds(pending.identity.ctimeNs)
        : (pending.ref.ctimeMs ?? 0);
      const titleHash =
        pending.ref.titleHash ?? Bun.hash(pending.ref.title).toString(36);
      const parseComplete =
        pending.parsedAll &&
        pending.fragment.length === 0 &&
        !pending.discardingOversizedRecord;
      const markValues = [
        pending.completeEnd,
        mtimeMs,
        snapshotSize,
        ctimeMs,
        titleHash,
        head,
        tailHash,
        pending.capped ? 1 : 0,
        pending.promptCount,
        parseComplete ? 1 : 0,
      ] as const;
      const published = pending.mark
        ? updateMark.run(...markValues, pending.ref.id, pending.mark.revision)
            .changes > 0
        : insertMark.run(pending.ref.id, ...markValues).changes > 0;
      if (!published) {
        closePendingSession(pending);
        state.pending = null;
        result.skipped = 'conflict';
        stopped = true;
        break;
      }
      const revision = (pending.mark?.revision ?? 0) + 1;
      updatedMarks.set(pending.ref.id, {
        revision,
        bytes: pending.completeEnd,
        mtimeMs,
        size: snapshotSize,
        ctimeMs,
        titleHash,
        headHash: head,
        tailHash,
        capped: pending.capped,
        parseComplete,
      });
      del.run(pending.ref.id);
      ins.run(pending.ref.id, pending.ref.title, pending.promptText);
      if (pending.mark) result.updated++;
      else result.added++;

      closePendingSession(pending);
      state.pending = null;
      nextCursor++;
    }

    if (result.skipped === 'conflict') {
      db.run('ROLLBACK');
      return {
        ...result,
        added: 0,
        updated: 0,
        removed: 0,
        done: false,
      };
    }

    db.run('COMMIT');
    for (const id of deletedMarks) marks.delete(id);
    for (const [id, mark] of updatedMarks) marks.set(id, mark);
    state.cursor = nextCursor;
    state.removalsApplied = true;
    result.done =
      !stopped && state.cursor >= work.length && state.pending === null;
  } catch (error) {
    try {
      db.run('ROLLBACK');
    } catch {
      /* the transaction is already gone */
    }
    closePendingSession(state.pending);
    state.pending = null;
    throw error;
  }
  return result;
}

/** Operators are neutralised by quoting, so user text can never form a query. */
function toMatch(query: string): string | null {
  const terms = query
    .trim()
    .split(/\s+/)
    .map((t) => t.replace(/"/g, ''))
    .filter((t) => t.length > 0);
  if (!terms.length) return null;
  return terms.map((t) => `"${t}"*`).join(' AND ');
}

/** One definition of the content-query minimum — the UI must not restate it. */
export const MIN_CONTENT_QUERY = 3;

export function search(handle: IndexHandle, query: string, limit = 20): Hit[] {
  if (handle.mode !== 'content') return [];
  if (query.trim().length < MIN_CONTENT_QUERY) return [];
  const match = toMatch(query);
  if (!match) return [];
  try {
    return handle.db
      .query<{ sid: string; sn: string; score: number }, [string, number]>(
        `SELECT sid, snippet(sessions, 2, '', '', '…', 12) AS sn,
                bm25(sessions, 0, 10.0, 5.0) AS score
         FROM sessions WHERE sessions MATCH ? ORDER BY score LIMIT ?`
      )
      .all(match, limit)
      .map((r) => ({ id: r.sid, snippet: r.sn, score: r.score }));
  } catch {
    return [];
  }
}

export function forget(handle: IndexHandle, id: string): void {
  if (handle.mode !== 'content') return;
  try {
    handle.db.run('BEGIN IMMEDIATE');
    handle.db.prepare('DELETE FROM sessions WHERE sid = ?').run(id);
    handle.db.prepare('DELETE FROM marks WHERE sid = ?').run(id);
    handle.db.run('COMMIT');
  } catch {
    try {
      handle.db.run('ROLLBACK');
    } catch {
      /* nothing to undo */
    }
  }
}

/**
 * Read the full marks table into a map — for holding across reconcile
 * slices so each slice skips the 10K-row re-read.
 */
export function readAllMarks(handle: IndexHandle): Map<string, Mark> {
  if (handle.mode !== 'content') return new Map();
  try {
    return readMarks(handle.db);
  } catch {
    return new Map();
  }
}

export function promptlessIds(handle: IndexHandle): Set<string> {
  if (handle.mode !== 'content') return new Set();
  try {
    // Answer from the plain marks table: querying the FTS `sessions` table
    // materializes each row's prompt blob (up to 256KB) just to test
    // emptiness — tens of MB of reads at 10K sessions.
    return new Set(
      handle.db
        .query<{ sid: string }, []>(
          'SELECT sid FROM marks WHERE prompt_count = 0 AND parse_complete = 1'
        )
        .all()
        .map((r) => r.sid)
    );
  } catch {
    return new Set();
  }
}

/**
 * Leading slice of each indexed session's prompt text, for deriving a
 * display title when the stored title is a placeholder. Answered from the
 * index for both stores, so KAS-native rows title from their first prompt
 * without anyone re-reading transcripts.
 */
export function firstPromptHeads(handle: IndexHandle): Map<string, string> {
  if (handle.mode !== 'content') return new Map();
  try {
    return new Map(
      handle.db
        .query<{ sid: string; head: string }, []>(
          "SELECT sid, substr(prompts, 1, 300) AS head FROM sessions WHERE prompts != ''"
        )
        .all()
        .map((r) => [r.sid, r.head])
    );
  } catch {
    return new Map();
  }
}

/**
 * Per-session user-prompt counts from the marks table. Returns a map of
 * sessionId → count for every session with at least one prompt. Cheap: a
 * single indexed scan, no transcript reads.
 */
export function promptCounts(handle: IndexHandle): Map<string, number> {
  if (handle.mode !== 'content') return new Map();
  try {
    return new Map(
      handle.db
        .query<{ sid: string; prompt_count: number }, []>(
          'SELECT sid, prompt_count FROM marks WHERE prompt_count > 0'
        )
        .all()
        .map((r) => [r.sid, r.prompt_count])
    );
  } catch {
    return new Map();
  }
}
