/**
 * Session search — listing metadata in memory, content search on disk.
 *
 * Two layers with different residency:
 * 1. A small in-memory metadata map (title, prompt count, entry count) built
 *    from the V2 store, used to enrich dashboard rows and classify empties.
 * 2. An on-disk FTS5 index over titles and user prompts (both stores), which
 *    answers content queries without holding transcript text in memory.
 *
 * The index is built lazily on first dashboard open in byte-budgeted slices,
 * then incrementally updated: unchanged transcripts are skipped by mark, and
 * grown ones are tail-read from where extraction last stopped.
 */

import { existsSync, readdirSync, statSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import {
  OversizedJsonFileError,
  SESSION_METADATA_MAX_BYTES,
  readBoundedJson,
} from './bounded-json.js';
import { kiroHomePath } from './kiro-home.js';
import { logger } from './logger.js';
import { conversationKey } from './session-dashboard.js';
import {
  containedDirectory,
  containedRegularFile,
  listValidatedKasSessionCopies,
  resolveCanonicalKasSessionCopy,
  v2SessionPath,
} from './session-store.js';
import {
  closeIndex,
  createReconcileState,
  disposeReconcileState,
  forget,
  openIndex,
  promptlessIds,
  firstPromptHeads,
  promptCounts,
  readAllMarks,
  reconcile,
  search as contentSearch,
  type Coverage,
  type IndexHandle,
  type SessionRef,
} from './session-content-index.js';

/**
 * Listing metadata extracted from a session's store files. Deliberately
 * carries no transcript text — content lives in the on-disk index.
 */
export interface SessionDocument {
  sessionId: string;
  /** Full workspace path (cwd) for the session. */
  workspace: string;
  /** Session title (first user prompt or explicit title). */
  title: string;
  /** RFC3339 timestamp of last update. */
  updatedAt: string;
  /** Number of log entries. */
  entryCount: number;
  /** Number of user prompts — what the Messages column shows. */
  promptCount: number;
  /** True when the transcript read was capped — promptCount is a floor. */
  countCapped?: boolean;
  /** Source-file mtimes at index time — unchanged files skip re-reads. */
  metaMtimeMs?: number;
  logMtimeMs?: number;
}

/**
 * A search result with relevance score and context snippet.
 */
export interface SessionSearchResult {
  sessionId: string;
  engine: 'v2' | 'v3';
  /** Relevance rank score — meaningful only relative to other results. */
  score: number;
  /** Text snippet showing where the match occurred. */
  snippet: string;
  /** Which field matched (title, prompt). */
  matchField: 'title' | 'prompt';
}

/**
 * Index status for progress reporting.
 */
export interface IndexStatus {
  state: 'idle' | 'indexing' | 'ready' | 'error';
  /** Number of sessions indexed so far. */
  indexed: number;
  /** Total sessions to index. */
  total: number;
  /** Error message if state is 'error'. */
  error?: string;
}

type IndexStatusListener = (status: IndexStatus) => void;

/** Transcript bytes one reconcile slice may read before yielding to the UI. */
const RECONCILE_BYTE_BUDGET = 2 * 1024 * 1024;
/** Wall-time cap per reconcile slice — bytes don't bound time when the
 *  store is thousands of tiny files. */
const RECONCILE_TIME_BUDGET_MS = 25;

/** Grapheme cap for a derived title — code-unit slicing can split emoji. */
const TITLE_MAX_GRAPHEMES = 150;
const UUID_SESSION_ID =
  /^(?:sess_)?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function qualifiedSearchId(engine: 'v2' | 'v3', sessionId: string): string {
  return `${engine}\u0000${sessionId}`;
}

function unqualifiedSearchId(id: string): string {
  const separator = id.indexOf('\u0000');
  return separator >= 0 ? id.slice(separator + 1) : id;
}

function qualifiedSearchEngine(id: string): 'v2' | 'v3' | undefined {
  if (id.startsWith('v2\u0000')) return 'v2';
  if (id.startsWith('v3\u0000')) return 'v3';
  return undefined;
}

function isRecognizedBridgeId(sessionId: string): boolean {
  return (
    UUID_SESSION_ID.test(sessionId) || conversationKey(sessionId) !== sessionId
  );
}

// Segmenter construction is the expensive part — one per module, not per call.
const titleSegmenter = new Intl.Segmenter(undefined, {
  granularity: 'grapheme',
});

/**
 * Derive a one-line title from prompt text: the first non-empty line outside
 * a code fence, trimmed to a grapheme budget.
 */
export function fallbackTitleFromPrompt(prompt: string): string {
  let inFence = false;
  let line = '';
  for (const raw of prompt.split('\n')) {
    const t = raw.trim();
    if (t.startsWith('```')) {
      inFence = !inFence;
      continue;
    }
    if (inFence || !t) continue;
    line = t;
    break;
  }
  if (!line) return '';
  let out = '';
  let count = 0;
  for (const { segment } of titleSegmenter.segment(line)) {
    if (count >= TITLE_MAX_GRAPHEMES) break;
    out += segment;
    count++;
  }
  return out;
}

/**
 * Session search index over both on-disk stores.
 *
 * Lifecycle:
 * 1. `build()` — metadata pass over the V2 store, then budgeted FTS5
 *    reconcile over V2 + KAS transcripts
 * 2. `search(query)` — FTS5 content query (titles + prompts)
 * 3. `update(sessionId)` — re-index a single session (after new content)
 */
export class SessionSearchIndex {
  private documents = new Map<string, SessionDocument>();
  private status: IndexStatus = {
    state: 'idle',
    indexed: 0,
    total: 0,
  };
  private listeners = new Set<IndexStatusListener>();
  private sessionsDir: string;
  private buildPromise: Promise<void> | null = null;
  private refreshPending = false;
  private conflictRetry = false;
  // Set by abort() to stop an in-flight reconcile at the next slice boundary.
  // The dashboard aborts on unmount so a long store reconcile cannot keep
  // starving the event loop once the user is back in the chat.
  private aborted = false;
  private handle: IndexHandle | null = null;
  private searchEngines = new Map<string, 'v2' | 'v3'>();
  // One snapshot of the per-session fact tables. The underlying helpers each
  // run a full table scan; calling them per row turns an 8K-session
  // enrichment pass into ~35K full scans (minutes of render-thread stalls).
  private factsCache: {
    heads: Map<string, string>;
    counts: Map<string, number>;
    promptless: Set<string>;
  } | null = null;

  private facts(): NonNullable<SessionSearchIndex['factsCache']> {
    if (!this.factsCache) {
      const handle = this.openHandle();
      this.factsCache = {
        heads: firstPromptHeads(handle),
        counts: promptCounts(handle),
        promptless: promptlessIds(handle),
      };
    }
    return this.factsCache;
  }

  private invalidateFacts(): void {
    this.factsCache = null;
  }

  constructor(sessionsDir?: string) {
    this.sessionsDir =
      sessionsDir ??
      process.env.KIRO_TEST_SESSIONS_DIR ??
      kiroHomePath('sessions', 'cli');
  }

  getStatus(): IndexStatus {
    return { ...this.status };
  }

  onStatusChange(listener: IndexStatusListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emitStatus(): void {
    for (const listener of this.listeners) {
      listener(this.status);
    }
  }

  /** The store root when this looks like a real store, else the dir itself. */
  private storeRoot(): string {
    return basename(this.sessionsDir) === 'cli'
      ? dirname(this.sessionsDir)
      : this.sessionsDir;
  }

  private openHandle(retryUnavailable = false): IndexHandle {
    if (
      retryUnavailable &&
      this.handle?.mode === 'titles' &&
      this.handle.reason === 'unavailable'
    ) {
      this.handle = null;
      this.invalidateFacts();
    }
    if (!this.handle) {
      this.handle = openIndex(join(this.storeRoot(), 'dashboard-search.db'));
    }
    return this.handle;
  }

  private indexedId(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): string {
    if (engine === 'classic') return sessionId;
    if (!engine) return sessionId;
    const qualified = qualifiedSearchId(engine, sessionId);
    if (this.searchEngines.has(qualified)) return qualified;
    const unqualifiedEngine = this.searchEngines.get(sessionId);
    return unqualifiedEngine && unqualifiedEngine !== engine
      ? qualified
      : sessionId;
  }

  private factsId(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): string | null {
    if (engine === 'classic') return null;
    const resolvedEngine = engine ?? this.searchEngines.get(sessionId);
    if (!resolvedEngine) return sessionId;
    const qualified = qualifiedSearchId(resolvedEngine, sessionId);
    if (this.searchEngines.has(qualified)) return qualified;
    const unqualifiedEngine = this.searchEngines.get(sessionId);
    if (!unqualifiedEngine || unqualifiedEngine === resolvedEngine) {
      return sessionId;
    }
    return qualified;
  }

  /** Close the on-disk index (for testing/teardown). */
  close(): void {
    if (this.handle) {
      closeIndex(this.handle);
      this.handle = null;
      this.invalidateFacts();
    }
  }

  /**
   * What search actually covers right now — drives the field's coverage
   * label, which must state coverage rather than where the last match came
   * from.
   */
  getCoverage(): Coverage {
    const handle = this.openHandle();
    return handle.mode === 'content' ? handle.coverage : 'titles-only';
  }

  /**
   * Build the index from all session files.
   * Safe to call multiple times — returns the same promise if already building.
   */
  async build(): Promise<void> {
    if (this.buildPromise) return this.buildPromise;
    if (this.status.state === 'ready') return;
    return this.startBuild();
  }

  /**
   * Bring the index up to date. Unlike {@link build} this runs even when the
   * index is already ready — the dashboard calls it on every open, which is
   * the only moment the store is reconciled (nothing watches it in between).
   */
  async refresh(): Promise<void> {
    if (this.buildPromise) {
      this.refreshPending = true;
      return this.buildPromise;
    }
    return this.startBuild();
  }

  /**
   * Stop an in-flight reconcile at the next slice boundary. The store scan
   * is resumable (marks record per-session progress), so aborting only
   * yields the event loop back — the next {@link refresh} picks up where
   * this left off. Safe to call when nothing is building.
   */
  abort(): void {
    if (this.buildPromise) this.aborted = true;
  }

  private startBuild(): Promise<void> {
    const p = (async () => {
      this.aborted = false;
      // Conflict passes back off exponentially: another live process is
      // reconciling the same store, and an immediate full re-pass turns two
      // dashboards into a permanent CPU hot-loop that starves keyboard input.
      let conflictDelayMs = 0;
      do {
        this.refreshPending = false;
        this.conflictRetry = false;
        await this.doBuild();
        if (this.aborted) break;
        if (this.conflictRetry) {
          conflictDelayMs = Math.min(
            Math.max(conflictDelayMs * 2, 2_000),
            30_000
          );
          await new Promise((r) => setTimeout(r, conflictDelayMs));
        } else {
          conflictDelayMs = 0;
        }
      } while (this.refreshPending);
    })().finally(() => {
      if (this.buildPromise === p) this.buildPromise = null;
    });
    this.buildPromise = p;
    return p;
  }

  private async doBuild(): Promise<void> {
    try {
      this.status = { state: 'indexing', indexed: 0, total: 0 };
      this.emitStatus();

      const listing = await this.listSessionFiles();
      const jsonFiles = listing.files;
      let listingComplete = listing.complete;
      this.status.total = jsonFiles.length;
      this.emitStatus();

      const handle = this.openHandle(true);
      const marks = readAllMarks(handle);
      for (const id of marks.keys()) {
        const engine = qualifiedSearchEngine(id);
        if (engine) this.searchEngines.set(id, engine);
      }
      const indexedHeads = firstPromptHeads(handle);
      const indexedCounts = promptCounts(handle);
      const indexedPromptless = promptlessIds(handle);

      const nextDocs = new Map<string, SessionDocument>();
      const prevDocs = this.documents;
      let sliceStart = performance.now();
      for (const file of jsonFiles) {
        const sessionId = file.replace('.json', '');
        const doc = this.indexSession(
          sessionId,
          indexedHeads,
          indexedCounts,
          indexedPromptless,
          () => {
            listingComplete = false;
          },
          this.indexedId(sessionId, 'v2'),
          prevDocs.get(sessionId)
        );
        if (doc) {
          nextDocs.set(sessionId, doc);
        }
        this.status.indexed++;
        if (performance.now() - sliceStart >= RECONCILE_TIME_BUDGET_MS) {
          this.emitStatus();
          await new Promise((resolve) => setImmediate(resolve));
          if (this.aborted) return;
          sliceStart = performance.now();
        }
      }
      this.emitStatus();
      this.documents = nextDocs;

      // Content pass — reconcile the on-disk index in byte-budgeted slices
      // so a cold build never puts one long stall on the UI thread.
      if (handle.mode === 'content') {
        const collected = await this.collectRefs();
        if (this.aborted) return;
        const refs = collected.refs;
        const discoveredEngines = new Map(
          refs.map((ref) => [ref.id, ref.engine ?? 'v2'] as const)
        );
        if (listingComplete && collected.complete) {
          this.searchEngines = discoveredEngines;
        } else {
          for (const [id, engine] of discoveredEngines) {
            this.searchEngines.set(id, engine);
          }
        }
        const state = createReconcileState(
          handle,
          refs,
          marks,
          listingComplete && collected.complete
        );
        try {
          for (;;) {
            const r = reconcile(handle, refs, {
              byteBudget: RECONCILE_BYTE_BUDGET,
              timeBudgetMs: RECONCILE_TIME_BUDGET_MS,
              state,
            });
            if (r.skipped === 'busy') {
              throw new Error(
                'Session search index is busy in another process'
              );
            }
            if (r.skipped === 'conflict') {
              this.refreshPending = true;
              this.conflictRetry = true;
              // Partial reconcile work may have landed; stale fact
              // snapshots must not survive the retry backoff.
              this.invalidateFacts();
              return;
            }
            if (r.done) break;
            await new Promise((resolve) => setImmediate(resolve));
            if (this.aborted) return;
          }
        } finally {
          disposeReconcileState(state);
        }
        const heads = firstPromptHeads(handle);
        const counts = promptCounts(handle);
        const promptless = promptlessIds(handle);
        let refineStart = performance.now();
        for (const [id, doc] of this.documents) {
          const factsId = this.indexedId(id, 'v2');
          const promptCount = counts.get(factsId) ?? 0;
          const title =
            doc.title === '(no title)'
              ? fallbackTitleFromPrompt(heads.get(factsId) ?? '') || doc.title
              : doc.title;
          this.documents.set(id, {
            ...doc,
            title,
            entryCount: promptCount,
            promptCount,
            countCapped: !counts.has(factsId) && !promptless.has(factsId),
          });
          // Title derivation segments graphemes per doc — unyielded, this
          // pass alone stalls the loop for most of a second at 10K docs.
          if (performance.now() - refineStart >= RECONCILE_TIME_BUDGET_MS) {
            await new Promise((resolve) => setImmediate(resolve));
            if (this.aborted) return;
            refineStart = performance.now();
          }
        }
      }

      this.status = {
        state: 'ready',
        indexed: this.documents.size,
        total: jsonFiles.length,
      };
      this.invalidateFacts();
      this.emitStatus();
    } catch (err) {
      this.status = {
        state: 'error',
        indexed: this.documents.size,
        total: this.status.total,
        error: err instanceof Error ? err.message : String(err),
      };
      this.invalidateFacts();
      this.emitStatus();
      logger.warn('[session-search] Build failed:', err);
    }
  }

  private async listSessionFiles(): Promise<{
    files: string[];
    complete: boolean;
  }> {
    if (!existsSync(this.sessionsDir)) return { files: [], complete: true };
    if (
      basename(this.sessionsDir) === 'cli' &&
      !containedDirectory(this.storeRoot(), 'cli')
    ) {
      return { files: [], complete: false };
    }
    try {
      let complete = true;
      const files: string[] = [];
      let sliceStart = performance.now();
      for (const file of readdirSync(this.sessionsDir)) {
        if (!file.endsWith('.json')) continue;
        // Containment lstat-walks every path segment; at 10K files this
        // filter alone blocks the loop for hundreds of ms without yields.
        if (containedRegularFile(this.sessionsDir, file)) files.push(file);
        else complete = false;
        if (performance.now() - sliceStart >= RECONCILE_TIME_BUDGET_MS) {
          await new Promise((resolve) => setImmediate(resolve));
          if (this.aborted) return { files, complete: false };
          sliceStart = performance.now();
        }
      }
      return { files, complete };
    } catch {
      return { files: [], complete: false };
    }
  }

  /** Transcript refs for the content index, spanning the V2 and KAS stores.
   *  Time-sliced: the stat/read walk over the whole store otherwise blocks
   *  the event loop for hundreds of ms on every dashboard open. */
  private async collectRefs(): Promise<{
    refs: SessionRef[];
    complete: boolean;
  }> {
    const refs: SessionRef[] = [];
    let complete = true;
    let sliceStart = performance.now();
    const tick = async () => {
      if (performance.now() - sliceStart >= RECONCILE_TIME_BUDGET_MS) {
        await new Promise((resolve) => setImmediate(resolve));
        sliceStart = performance.now();
      }
    };

    // V2 store: metadata docs already carry titles. A session with no
    // transcript still gets a ref — its title must be searchable.
    const refIdx = new Map<string, number>();
    for (const doc of this.documents.values()) {
      await tick();
      const lexicalTranscriptPath =
        basename(this.sessionsDir) === 'cli'
          ? v2SessionPath(this.storeRoot(), doc.sessionId, '.jsonl')
          : join(this.sessionsDir, `${doc.sessionId}.jsonl`);
      if (!lexicalTranscriptPath) {
        complete = false;
        continue;
      }
      const transcriptPath = containedRegularFile(
        this.sessionsDir,
        `${doc.sessionId}.jsonl`
      );
      if (!transcriptPath && existsSync(lexicalTranscriptPath)) {
        complete = false;
        continue;
      }
      const safeTranscriptPath = transcriptPath ?? lexicalTranscriptPath;
      let mtimeMs = 0;
      let size = 0;
      let ctimeMs = 0;
      let transcriptState: SessionRef['transcriptState'] = 'missing';
      if (transcriptPath) {
        try {
          const stat = statSync(transcriptPath);
          mtimeMs = stat.mtimeMs;
          size = stat.size;
          ctimeMs = stat.ctimeMs;
          transcriptState = 'present';
        } catch {
          transcriptState = 'unknown';
          complete = false;
        }
      }
      refIdx.set(doc.sessionId, refs.length);
      refs.push({
        id: doc.sessionId,
        transcriptPath: safeTranscriptPath,
        title: doc.title,
        mtimeMs,
        size,
        ctimeMs,
        titleHash: Bun.hash(doc.title).toString(36),
        transcriptState,
        engine: 'v2',
      });
    }

    // KAS store: use the same validated canonical copy as listing/preview.
    const root = this.storeRoot();
    if (root === this.sessionsDir) return { refs, complete };
    const scanned = listValidatedKasSessionCopies(root);
    complete &&= scanned.complete;
    const bySession = new Map<string, typeof scanned.copies>();
    for (const copy of scanned.copies) {
      const copies = bySession.get(copy.sessionId) ?? [];
      copies.push(copy);
      bySession.set(copy.sessionId, copies);
    }
    for (const copies of bySession.values()) {
      await tick();
      const copy = resolveCanonicalKasSessionCopy(copies);
      if (!copy || copy.placement !== 'local') continue;
      const lexicalTranscriptPath = join(copy.dir, 'messages.jsonl');
      const transcriptPath = containedRegularFile(copy.dir, 'messages.jsonl');
      if (!transcriptPath && existsSync(lexicalTranscriptPath)) {
        complete = false;
        continue;
      }
      let mtimeMs = 0;
      let size = 0;
      let ctimeMs = 0;
      let transcriptState: SessionRef['transcriptState'] = 'missing';
      if (transcriptPath) {
        try {
          const stat = statSync(transcriptPath);
          mtimeMs = stat.mtimeMs;
          size = stat.size;
          ctimeMs = stat.ctimeMs;
          transcriptState = 'present';
        } catch {
          transcriptState = 'unknown';
          complete = false;
        }
      }
      const title =
        typeof copy.metadata.title === 'string' ? copy.metadata.title : '';
      const ref = {
        id: copy.sessionId,
        transcriptPath: transcriptPath ?? lexicalTranscriptPath,
        title,
        mtimeMs,
        size,
        ctimeMs,
        titleHash: Bun.hash(title).toString(36),
        transcriptState,
        engine: 'v3' as const,
      };
      const existing = refIdx.get(ref.id);
      if (existing != null) {
        const existingRef = refs[existing]!;
        if (
          existingRef.engine === 'v2' &&
          !isRecognizedBridgeId(copy.sessionId)
        ) {
          existingRef.id = qualifiedSearchId('v2', copy.sessionId);
          ref.id = qualifiedSearchId('v3', copy.sessionId);
          refIdx.set(copy.sessionId, refs.length);
          refs.push(ref);
          continue;
        }
        refs[existing] = ref;
        continue;
      }
      refIdx.set(ref.id, refs.length);
      refs.push(ref);
    }
    return { refs, complete };
  }

  /** Index one V2 metadata record using transcript facts persisted in SQLite. */
  private indexSession(
    sessionId: string,
    heads: ReadonlyMap<string, string>,
    counts: ReadonlyMap<string, number>,
    promptless: ReadonlySet<string>,
    onError?: () => void,
    factsId = sessionId,
    prevDoc?: SessionDocument
  ): SessionDocument | null {
    try {
      const metaPath = containedRegularFile(
        this.sessionsDir,
        `${sessionId}.json`
      );
      if (!metaPath) {
        onError?.();
        return null;
      }
      const lexicalLogPath = join(this.sessionsDir, `${sessionId}.jsonl`);
      const safeLogPath = containedRegularFile(
        this.sessionsDir,
        `${sessionId}.jsonl`
      );
      if (!safeLogPath && existsSync(lexicalLogPath)) {
        onError?.();
        return null;
      }
      const logPath = safeLogPath ?? lexicalLogPath;
      const metaStat = statSync(metaPath);
      let logMtime = 0;
      try {
        logMtime = statSync(logPath).mtimeMs;
      } catch {
        /* no transcript yet */
      }
      // Reopening the dashboard re-runs the whole metadata pass. Reading and
      // parsing every session.json each time dominates the cost on a large
      // store (some files are multiple MiB). When neither the metadata nor
      // the transcript changed since the last build, reuse the prior record
      // and skip the read — the fact-derived fields are refreshed downstream.
      if (
        prevDoc &&
        prevDoc.metaMtimeMs === metaStat.mtimeMs &&
        prevDoc.logMtimeMs === logMtime
      ) {
        return prevDoc;
      }
      const promptCount = counts.get(factsId) ?? 0;
      const hasTranscriptFacts = counts.has(factsId) || promptless.has(factsId);
      const firstLine = fallbackTitleFromPrompt(heads.get(factsId) ?? '');
      let meta: {
        cwd?: unknown;
        title?: unknown;
        updated_at?: unknown;
        parent_session_id?: unknown;
        session_created_reason?: unknown;
      };
      try {
        meta = readBoundedJson(
          metaPath,
          SESSION_METADATA_MAX_BYTES
        ) as typeof meta;
      } catch (err) {
        if (!(err instanceof OversizedJsonFileError)) throw err;
        // Oversized metadata still names a real session — index a degraded
        // row (prompt-derived title, mtime recency) rather than dropping it
        // and marking the whole listing incomplete forever.
        return {
          sessionId,
          workspace: '',
          title: firstLine || '(no title)',
          updatedAt: new Date(metaStat.mtimeMs).toISOString(),
          entryCount: promptCount,
          promptCount,
          countCapped: !hasTranscriptFacts,
          metaMtimeMs: metaStat.mtimeMs,
          logMtimeMs: logMtime,
        };
      }
      if (
        meta.parent_session_id &&
        meta.session_created_reason === 'subagent'
      ) {
        return null;
      }

      return {
        sessionId,
        workspace: typeof meta.cwd === 'string' ? meta.cwd : '',
        title:
          (typeof meta.title === 'string' ? meta.title : '') ||
          firstLine ||
          '(no title)',
        updatedAt: typeof meta.updated_at === 'string' ? meta.updated_at : '',
        entryCount: promptCount,
        promptCount,
        countCapped: !hasTranscriptFacts,
        metaMtimeMs: metaStat.mtimeMs,
        logMtimeMs: logMtime,
      };
    } catch (err) {
      onError?.();
      logger.debug(
        `[session-search] Failed to index session ${sessionId}:`,
        err
      );
      return null;
    }
  }

  /** Re-index one physical session after a mutation. */
  update(sessionId: string, engine?: 'classic' | 'v2' | 'v3'): void {
    if (engine === 'classic') return;
    const handle = this.openHandle();
    const indexedId = this.indexedId(sessionId, engine);
    if (engine === 'v3') {
      forget(handle, indexedId);
      this.searchEngines.delete(indexedId);
      this.invalidateFacts();
      void this.refresh();
      return;
    }
    const facts = this.facts();
    let doc = this.indexSession(
      sessionId,
      facts.heads,
      facts.counts,
      facts.promptless,
      undefined,
      indexedId
    );
    this.invalidateFacts();
    if (!doc) {
      this.documents.delete(sessionId);
      forget(handle, indexedId);
      this.searchEngines.delete(indexedId);
      return;
    }
    this.documents.set(sessionId, doc);
    this.searchEngines.set(indexedId, 'v2');
    if (handle.mode !== 'content') return;
    const lexicalTranscriptPath = join(this.sessionsDir, `${sessionId}.jsonl`);
    const transcriptPath = containedRegularFile(
      this.sessionsDir,
      `${sessionId}.jsonl`
    );
    if (!transcriptPath && existsSync(lexicalTranscriptPath)) return;
    try {
      const safeTranscriptPath = transcriptPath ?? lexicalTranscriptPath;
      const stat = transcriptPath ? statSync(transcriptPath) : null;
      const result = reconcile(
        handle,
        [
          {
            id: indexedId,
            transcriptPath: safeTranscriptPath,
            title: doc.title,
            mtimeMs: stat?.mtimeMs ?? 0,
            size: stat?.size ?? 0,
            ctimeMs: stat?.ctimeMs ?? 0,
            titleHash: Bun.hash(doc.title).toString(36),
            transcriptState: transcriptPath ? 'present' : 'missing',
            engine: 'v2',
          },
        ],
        { removeMissing: false }
      );
      if (result.skipped === 'conflict') {
        void this.refresh();
        return;
      }
      doc = this.indexSession(
        sessionId,
        firstPromptHeads(handle),
        promptCounts(handle),
        promptlessIds(handle),
        undefined,
        indexedId
      );
      if (doc) this.documents.set(sessionId, doc);
    } catch {
      /* no transcript — nothing for the content index */
    }
  }

  /**
   * Content search over titles and user prompts via the on-disk index.
   * Results arrive ranked; scores are relative ranks, not absolute values.
   */
  search(query: string, limit = 20): SessionSearchResult[] {
    const q = query.trim();
    if (!q) return [];
    const handle = this.openHandle();
    const hits = contentSearch(handle, q, limit);
    return hits.map((h) => {
      const sessionId = unqualifiedSearchId(h.id);
      const engine = this.searchEngines.get(h.id) ?? 'v2';
      return {
        sessionId,
        engine,
        // bm25 returns lower-is-better; expose higher-is-better like before.
        score: -h.score,
        snippet:
          h.snippet || (this.getDocument(sessionId, engine)?.title ?? ''),
        matchField: h.snippet ? ('prompt' as const) : ('title' as const),
      };
    });
  }

  /** Whether this exact physical session has no indexed user prompts. */
  isPromptless(sessionId: string, engine?: 'classic' | 'v2' | 'v3'): boolean {
    const id = this.factsId(sessionId, engine);
    if (!id) return false;
    const facts = this.facts();
    const count = facts.counts.get(id);
    return facts.promptless.has(id) && !(count != null && count > 0);
  }

  /** First user prompt converted to a display title for this physical row. */
  getPromptTitle(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): string | undefined {
    const id = this.factsId(sessionId, engine);
    if (!id) return undefined;
    const head = this.facts().heads.get(id);
    const title = head ? fallbackTitleFromPrompt(head) : '';
    return title || undefined;
  }

  /** Exact indexed user-prompt count for this physical row. */
  getPromptCount(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): number | undefined {
    const id = this.factsId(sessionId, engine);
    return id ? this.facts().counts.get(id) : undefined;
  }

  /** Number of indexed V2 metadata documents. */
  get size(): number {
    return this.documents.size;
  }

  /** Get the V2 metadata document for an exact physical session. */
  getDocument(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): SessionDocument | undefined {
    if (engine === 'classic' || engine === 'v3') return undefined;
    if (engine === undefined && this.searchEngines.get(sessionId) === 'v3') {
      return undefined;
    }
    return this.documents.get(sessionId);
  }
}

/**
 * Singleton index instance. Created on first dashboard open.
 */
let globalIndex: SessionSearchIndex | null = null;

/** Get or create the global session search index without starting I/O. */
export function getSessionSearchIndex(): SessionSearchIndex {
  if (!globalIndex) globalIndex = new SessionSearchIndex();
  return globalIndex;
}

/**
 * Reset the global index (for testing).
 */
export function resetSessionSearchIndex(): void {
  globalIndex?.close();
  globalIndex = null;
}
