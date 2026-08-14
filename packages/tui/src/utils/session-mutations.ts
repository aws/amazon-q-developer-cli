/**
 * Session mutations — delete and GC for the on-disk session stores.
 *
 * Safety model (destructive operations demand belt-and-suspenders):
 * - V2 store (`cli/{id}.json|.jsonl`): a session with a `.lock` file whose
 *   PID is alive is OPEN in another window — never touched.
 * - KAS-native store (`{hash}/sess_{id}/`): a `.lock` in the session dir
 *   (written on load) is honored the same way, and a recency guard backs it
 *   up — sessions modified within the last hour are treated as
 *   possibly-live and skipped.
 * - The ACTIVE session (the one the user is in) is always excluded by the
 *   caller passing `activeSessionId`.
 * - Bookmarked sessions are never GC'd (an explicit user mark outranks
 *   the emptiness heuristic).
 * - GC always runs as a dry-run scan first; deletion happens only on an
 *   explicit second call with the scanned ids.
 */

import {
  existsSync,
  lstatSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from 'node:fs';
import { basename, join } from 'node:path';
import {
  LOCK_METADATA_MAX_BYTES,
  SESSION_METADATA_MAX_BYTES,
  readBoundedJson,
} from './bounded-json.js';
import {
  containedDirectory,
  containedRegularFile,
  findKasSessionDirs,
  isValidSessionId,
  kasHashDirs,
  normalizeSessionId,
  sessionsRoot,
  v2SessionPath,
  validatedKasSessionId,
} from './session-store.js';
import {
  acquireSessionDeletionLocks,
  isKasDirLocked,
  releaseSessionDeletionLocks,
} from './session-lock.js';
import {
  SessionBookmarkStore,
  getSessionBookmarkStore,
} from './session-bookmarks.js';
import { logger } from './logger.js';

/** Cleanup skips KAS-native sessions modified more recently than this. */
const KAS_RECENCY_GUARD_MS = 60 * 60 * 1000;
/** An explicit delete only refuses a session that looks live right now. */
const KAS_LIVE_GUARD_MS = 5 * 60 * 1000;

export type DeleteOutcome =
  | { ok: true; store: 'v2' | 'kas' }
  | {
      ok: false;
      reason:
        | 'not-found'
        | 'invalid-id'
        | 'locked'
        | 'active'
        | 'recent'
        | 'cloud'
        | 'error';
    };

/** A present lock fails closed unless it contains a valid dead PID. */
function isV2Locked(root: string, sessionId: string): boolean {
  const lockPath = v2SessionPath(root, sessionId, '.lock');
  if (!lockPath || !existsSync(lockPath)) return false;
  try {
    const lock = readBoundedJson(lockPath, LOCK_METADATA_MAX_BYTES) as {
      pid?: unknown;
    };
    const pid = Number(lock.pid);
    if (!Number.isSafeInteger(pid) || pid <= 0) return true;
    try {
      process.kill(pid, 0);
      return true;
    } catch (err) {
      return (err as NodeJS.ErrnoException).code === 'EPERM';
    }
  } catch {
    return true;
  }
}

/**
 * Delete one session from whichever store holds it.
 * Refuses: the active session, sessions with a live lock in either store,
 * and KAS-native sessions modified within the recency guard window.
 */
export function deleteSession(
  sessionId: string,
  activeSessionId?: string | null,
  root: string = sessionsRoot(),
  store?: 'v2' | 'kas'
): DeleteOutcome {
  if (!isValidSessionId(sessionId)) {
    return { ok: false, reason: 'invalid-id' };
  }
  const normalizedId =
    store === 'v2' ? sessionId : normalizeSessionId(sessionId);
  const activeId =
    activeSessionId && store !== 'v2'
      ? normalizeSessionId(activeSessionId)
      : activeSessionId;
  if (activeId === normalizedId) {
    return { ok: false, reason: 'active' };
  }

  // V2 store first (flat files), with every path contained beneath cli/.
  const v2Extensions = ['.json', '.jsonl', '.history', '.lock'] as const;
  const v2Paths = v2Extensions.map((extension) =>
    v2SessionPath(root, normalizedId, extension)
  );
  const v2Meta = v2Paths[0];
  if (store !== 'kas' && v2Meta && existsSync(v2Meta)) {
    const safePaths = v2Paths.filter((path): path is string => path !== null);
    if (safePaths.length !== v2Extensions.length) {
      return { ok: false, reason: 'invalid-id' };
    }
    let deletionLocks;
    try {
      deletionLocks = acquireSessionDeletionLocks([safePaths[3]!]);
    } catch {
      return { ok: false, reason: 'locked' };
    }
    try {
      const finalPaths = v2Extensions.map((extension) =>
        v2SessionPath(root, normalizedId, extension)
      );
      if (finalPaths.some((path) => path === null)) {
        return { ok: false, reason: 'invalid-id' };
      }
      if (!existsSync(finalPaths[0]!)) {
        return { ok: false, reason: 'not-found' };
      }
      for (const path of finalPaths) rmSync(path!, { force: true });
      return { ok: true, store: 'v2' };
    } catch (err) {
      logger.warn(
        `[session-mutations] v2 delete failed for ${normalizedId}:`,
        err
      );
      return { ok: false, reason: 'error' };
    } finally {
      releaseSessionDeletionLocks(deletionLocks);
    }
  }

  // KAS-native store (per-workspace dirs). One id can live in several hash
  // dirs — deleting only one would resurrect the session on the next
  // refresh, so guards are evaluated across ALL copies and all are removed.
  const kasDirs = store === 'v2' ? [] : findKasSessionDirs(root, normalizedId);
  if (kasDirs.length > 0) {
    try {
      const metadata = kasDirs.map((dir) =>
        readValidatedKasMetadata(dir, normalizedId)
      );
      if (metadata.some((meta) => meta === null)) {
        return { ok: false, reason: 'error' };
      }
      // A cloud-placed session's local record is only a pointer — removing
      // it would orphan the backend copy, not delete the session. Cloud
      // deletion must go through the agent.
      const placements = metadata.map((meta) => kasPlacement(meta!));
      if (placements.includes('unknown')) {
        return { ok: false, reason: 'error' };
      }
      if (placements.includes('remote')) {
        return { ok: false, reason: 'cloud' };
      }
      let deletionLocks;
      try {
        deletionLocks = acquireSessionDeletionLocks(
          kasDirs.map((dir) => join(dir, '.lock'))
        );
      } catch {
        return { ok: false, reason: 'locked' };
      }
      try {
        const finalDirs = findKasSessionDirs(root, normalizedId);
        if (
          finalDirs.length !== kasDirs.length ||
          finalDirs.some((dir) => !kasDirs.includes(dir))
        ) {
          return { ok: false, reason: 'error' };
        }
        const finalMetadata = finalDirs.map((dir) =>
          readValidatedKasMetadata(dir, normalizedId)
        );
        if (finalMetadata.some((meta) => meta === null)) {
          return { ok: false, reason: 'error' };
        }
        const finalPlacements = finalMetadata.map((meta) =>
          kasPlacement(meta!)
        );
        if (finalPlacements.includes('unknown')) {
          return { ok: false, reason: 'error' };
        }
        if (finalPlacements.includes('remote')) {
          return { ok: false, reason: 'cloud' };
        }
        const activity = Math.max(...finalDirs.map(kasActivityMs));
        if (activity > 0 && Date.now() - activity < KAS_LIVE_GUARD_MS) {
          return { ok: false, reason: 'recent' };
        }
        for (const dir of finalDirs) {
          rmSync(dir, { recursive: true, force: true });
        }
        return { ok: true, store: 'kas' };
      } finally {
        releaseSessionDeletionLocks(deletionLocks);
      }
    } catch (err) {
      logger.warn(
        `[session-mutations] kas delete failed for ${sessionId}:`,
        err
      );
      return { ok: false, reason: 'error' };
    }
  }

  const invalidKasCandidate =
    store !== 'v2' &&
    kasHashDirs(root).some((hash) =>
      [normalizedId, `sess_${normalizedId}`].some(
        (dirName) => containedDirectory(root, hash, dirName) !== null
      )
    );
  return invalidKasCandidate
    ? { ok: false, reason: 'error' }
    : { ok: false, reason: 'not-found' };
}

type KasDeleteMetadata = {
  id?: unknown;
  workspacePaths?: unknown;
  parentSessionId?: unknown;
  executionTarget?: unknown;
};

function readValidatedKasMetadata(
  kasDir: string,
  expectedId: string
): KasDeleteMetadata | null {
  try {
    if (!lstatSync(kasDir).isDirectory()) return null;
    const metaPath = containedRegularFile(kasDir, 'session.json');
    if (!metaPath) return null;
    const meta = readBoundedJson(
      metaPath,
      SESSION_METADATA_MAX_BYTES
    ) as KasDeleteMetadata;
    return validatedKasSessionId(basename(kasDir), meta?.id) === expectedId
      ? meta
      : null;
  } catch {
    return null;
  }
}

type KasPlacement = 'local' | 'remote' | 'unknown';

function kasPlacement(meta: KasDeleteMetadata): KasPlacement {
  if (!Object.prototype.hasOwnProperty.call(meta, 'executionTarget')) {
    return 'local';
  }
  const target = meta.executionTarget;
  if (!target || typeof target !== 'object' || Array.isArray(target)) {
    return 'unknown';
  }
  const kind = (target as { kind?: unknown }).kind;
  if (kind === 'local') return 'local';
  return typeof kind === 'string' ? 'remote' : 'unknown';
}

/** Hold every local KAS lock from final validation through agent deletion. */
export async function deleteLocalKasSessionWithAgent(
  sessionId: string,
  activeSessionId: string | null | undefined,
  agentDelete: (sessionId: string) => Promise<boolean>,
  root: string = sessionsRoot()
): Promise<DeleteOutcome> {
  if (!isValidSessionId(sessionId)) {
    return { ok: false, reason: 'invalid-id' };
  }
  const normalizedId = normalizeSessionId(sessionId);
  if (activeSessionId && normalizeSessionId(activeSessionId) === normalizedId) {
    return { ok: false, reason: 'active' };
  }
  const dirs = findKasSessionDirs(root, normalizedId);
  if (dirs.length === 0) return { ok: false, reason: 'not-found' };

  let deletionLocks;
  try {
    deletionLocks = acquireSessionDeletionLocks(
      dirs.map((dir) => join(dir, '.lock'))
    );
  } catch {
    return { ok: false, reason: 'locked' };
  }

  try {
    const finalDirs = findKasSessionDirs(root, normalizedId);
    if (
      finalDirs.length !== dirs.length ||
      finalDirs.some((dir) => !dirs.includes(dir))
    ) {
      return { ok: false, reason: 'error' };
    }
    const metadata = finalDirs.map((dir) =>
      readValidatedKasMetadata(dir, normalizedId)
    );
    if (metadata.some((meta) => meta === null)) {
      return { ok: false, reason: 'error' };
    }
    const placements = metadata.map((meta) => kasPlacement(meta!));
    if (placements.some((placement) => placement !== 'local')) {
      return {
        ok: false,
        reason: placements.includes('remote') ? 'cloud' : 'error',
      };
    }
    // Same recency backstop as the direct delete path: the lock alone is not
    // enough — acquiring one is a no-op for a session directory another
    // terminal created moments ago and has not locked yet.
    const activity = Math.max(...finalDirs.map(kasActivityMs));
    if (activity > 0 && Date.now() - activity < KAS_LIVE_GUARD_MS) {
      return { ok: false, reason: 'recent' };
    }
    return (await agentDelete(normalizedId).catch(() => false))
      ? { ok: true, store: 'kas' }
      : { ok: false, reason: 'error' };
  } finally {
    releaseSessionDeletionLocks(deletionLocks);
  }
}

/** One GC candidate found by the scan. */
export interface GcCandidate {
  sessionId: string;
  store: 'v2' | 'kas';
  workspace: string;
}

/** Dry-run result: what WOULD be deleted, and what was skipped and why. */
export interface GcScan {
  candidates: GcCandidate[];
  skipped: {
    locked: number;
    recent: number;
    userTouched: number;
    active: number;
    hasParent: number;
  };
}

// A transcript larger than this always contains a prompt in practice —
// config-only/empty logs are tiny. Skipping the read caps scan I/O.
const EMPTY_SIZE_CUTOFF = 256 * 1024;

/** True when a validated V2 JSONL log contains no conversation content. */
function v2IsEmpty(root: string, sessionId: string): boolean {
  const logPath = v2SessionPath(root, sessionId, '.jsonl');
  const lexicalLogPath = join(root, 'cli', `${sessionId}.jsonl`);
  if (!existsSync(lexicalLogPath)) return true;
  if (!logPath) return false;
  try {
    if (statSync(logPath).size > EMPTY_SIZE_CUTOFF) return false;
    for (const line of readFileSync(logPath, 'utf-8').split('\n')) {
      if (!line.trim()) continue;
      try {
        const e = JSON.parse(line);
        // Any conversation-bearing entry counts as content — Compaction in
        // particular carries a whole imported-V1 messages snapshot even when
        // no Prompt line survives. Only control markers (e.g. Clear) and
        // unknown kinds leave a session classifiable as empty.
        if (
          e?.kind === 'Prompt' ||
          e?.kind === 'AssistantMessage' ||
          e?.kind === 'ToolResults' ||
          e?.kind === 'Compaction'
        ) {
          return false;
        }
      } catch {
        return false; // corrupt content could hide a conversation
      }
    }
    return true;
  } catch {
    return false; // unreadable → don't classify as empty (fail safe)
  }
}

/**
 * True when a KAS session directory holds no conversation.
 *
 * The title is not a proxy for this: it stays at the `New Session` placeholder
 * whenever the first-prompt derivation never ran, which does not imply the
 * transcript is empty.
 */
function kasIsEmpty(sessDirPath: string): boolean {
  const lexicalLogPath = join(sessDirPath, 'messages.jsonl');
  if (!existsSync(lexicalLogPath)) return true;
  const logPath = containedRegularFile(sessDirPath, 'messages.jsonl');
  if (!logPath) return false;
  try {
    const size = statSync(logPath).size;
    if (size === 0) return true;
    if (size > EMPTY_SIZE_CUTOFF) return false;
    for (const line of readFileSync(logPath, 'utf-8').split('\n')) {
      if (!line.trim()) continue;
      try {
        const e = JSON.parse(line);
        const type = e?.type ?? e?.payload?.type;
        if (type === 'turn_start' || type === 'user') return false;
      } catch {
        return false; // corrupt content could hide a conversation
      }
    }
    return true;
  } catch {
    return false; // unreadable → don't classify as empty (fail safe)
  }
}

/**
 * Newest activity time in a KAS session directory.
 *
 * `session.json` alone is not a liveness signal — a turn appends to
 * `messages.jsonl` and `sub-executions/` without rewriting metadata, so a
 * session in active use can present an hours-old `session.json` mtime.
 */
function kasActivityMs(sessDirPath: string): number {
  let newest = 0;
  const bump = (p: string) => {
    try {
      newest = Math.max(newest, statSync(p).mtimeMs);
    } catch {
      /* missing file contributes nothing */
    }
  };
  bump(join(sessDirPath, 'session.json'));
  bump(join(sessDirPath, 'messages.jsonl'));
  const subDir = join(sessDirPath, 'sub-executions');
  try {
    for (const f of readdirSync(subDir)) bump(join(subDir, f));
  } catch {
    /* no sub-executions */
  }
  return newest;
}

/**
 * Scan both stores for empty (prompt-less) sessions. Pure read — the
 * dry-run that precedes {@link gcEmptySessions}.
 *
 * `userTouchedIds` (any session the user bookmarked, tagged, or archived)
 * are exempt; the active session is exempt; V2 locked and KAS
 * recently-modified sessions are counted as skipped.
 */
export async function gcScan(
  activeSessionId?: string | null,
  userTouchedIds: ReadonlySet<string> = new Set(),
  root: string = sessionsRoot(),
  workspaceFilter?: string
): Promise<GcScan> {
  // Yield to the event loop on a time budget: the scan stats/reads thousands
  // of files of wildly varying cost, and a count-based cadence lets a run of
  // expensive files stall keyboard input.
  let sliceStart = performance.now();
  const tick = async () => {
    if (performance.now() - sliceStart >= 12) {
      await new Promise((r) => setImmediate(r));
      sliceStart = performance.now();
    }
  };
  const candidates: GcCandidate[] = [];
  const normalizedActiveId =
    activeSessionId && isValidSessionId(activeSessionId)
      ? normalizeSessionId(activeSessionId)
      : null;
  const skipped = {
    locked: 0,
    recent: 0,
    userTouched: 0,
    active: 0,
    hasParent: 0,
  };

  // V2 store.
  const cliDir = containedDirectory(root, 'cli');
  if (cliDir) {
    let files: string[];
    try {
      files = readdirSync(cliDir).filter((f) => f.endsWith('.json'));
    } catch {
      files = [];
    }
    for (const file of files) {
      await tick();
      const sessionId = file.slice(0, -'.json'.length);
      if (!isValidSessionId(sessionId)) continue;
      let workspace: string;
      let hasParent: boolean;
      try {
        const metaPath = containedRegularFile(cliDir, file);
        if (!metaPath) continue;
        const meta = readBoundedJson(metaPath, SESSION_METADATA_MAX_BYTES) as {
          cwd?: unknown;
          parent_session_id?: unknown;
        };
        workspace = typeof meta.cwd === 'string' ? meta.cwd : '';
        hasParent = isValidSessionId(meta.parent_session_id);
      } catch {
        continue; // unreadable metadata — leave it alone
      }
      if (workspaceFilter && workspace !== workspaceFilter) continue;
      if (!v2IsEmpty(root, sessionId)) continue;
      if (sessionId === normalizedActiveId) {
        skipped.active++;
      } else if (hasParent) {
        // A derived session (subagent, fork) belongs to its parent's story —
        // never a cleanup candidate even when its own transcript is empty.
        skipped.hasParent++;
      } else if (userTouchedIds.has(sessionId)) {
        skipped.userTouched++;
      } else if (isV2Locked(root, sessionId)) {
        skipped.locked++;
      } else {
        candidates.push({ sessionId, store: 'v2', workspace });
      }
    }
  }

  // KAS-native store. "Empty" = no user prompt in the transcript, same fact
  // the display filter uses.
  const seenKas = new Set<string>();
  for (const hash of kasHashDirs(root)) {
    const hashDir = join(root, hash);
    let sessDirs: string[];
    try {
      sessDirs = readdirSync(hashDir);
    } catch {
      continue;
    }
    for (const sessDir of sessDirs) {
      await tick();
      const sessDirPath = containedDirectory(root, hash, sessDir);
      if (!sessDirPath) continue;
      const metaPath = containedRegularFile(sessDirPath, 'session.json');
      if (!metaPath) continue;
      let firstMeta: KasDeleteMetadata;
      try {
        firstMeta = readBoundedJson(
          metaPath,
          SESSION_METADATA_MAX_BYTES
        ) as KasDeleteMetadata;
      } catch {
        continue;
      }
      const sessionId = validatedKasSessionId(sessDir, firstMeta.id);
      if (!sessionId || seenKas.has(sessionId)) continue;
      seenKas.add(sessionId);

      const copies = findKasSessionDirs(root, sessionId).map((dir) => ({
        dir,
        meta: readValidatedKasMetadata(dir, sessionId),
      }));
      if (copies.length === 0 || copies.some((copy) => copy.meta === null)) {
        continue;
      }
      const workspaceCopies = copies.filter((copy) => {
        const paths = copy.meta!.workspacePaths;
        const workspace =
          Array.isArray(paths) && typeof paths[0] === 'string' ? paths[0] : '';
        return !workspaceFilter || workspace === workspaceFilter;
      });
      if (workspaceCopies.length === 0) continue;
      if (copies.some((copy) => kasPlacement(copy.meta!) !== 'local')) continue;
      if (copies.some((copy) => !kasIsEmpty(copy.dir))) continue;

      const paths = workspaceCopies[0]!.meta!.workspacePaths;
      const workspace =
        Array.isArray(paths) && typeof paths[0] === 'string' ? paths[0] : '';
      if (sessionId === normalizedActiveId) {
        skipped.active++;
      } else if (copies.some((copy) => Boolean(copy.meta!.parentSessionId))) {
        skipped.hasParent++;
      } else if (userTouchedIds.has(sessionId)) {
        skipped.userTouched++;
      } else if (copies.some((copy) => isKasDirLocked(copy.dir))) {
        skipped.locked++;
      } else {
        const activity = Math.max(
          ...copies.map((copy) => kasActivityMs(copy.dir))
        );
        if (activity === 0 || Date.now() - activity < KAS_RECENCY_GUARD_MS) {
          skipped.recent++;
          continue;
        }
        candidates.push({ sessionId, store: 'kas', workspace });
      }
    }
  }

  return { candidates, skipped };
}

/**
 * Delete the sessions found by a prior {@link gcScan}. Each deletion is
 * independent; failures are tolerated and counted. Re-applies the per-session
 * guards (lock/recency/active) at deletion time in case state changed since
 * the scan.
 *
 * Emptiness is re-verified per candidate too: an arbitrary amount of user
 * review time passes between the scan and the confirmation, and a session that
 * gained a prompt in that window is no longer a husk.
 */
export async function gcEmptySessions(
  candidates: readonly GcCandidate[],
  activeSessionId?: string | null,
  root: string = sessionsRoot(),
  agentDelete?: (sessionId: string) => Promise<boolean>
): Promise<{ deleted: number; failed: number; stale: number }> {
  let deleted = 0;
  let failed = 0;
  let stale = 0;
  let opCount = 0;
  const activeKasId = activeSessionId
    ? normalizeSessionId(activeSessionId)
    : null;
  const bookmarkStore =
    root === sessionsRoot()
      ? getSessionBookmarkStore()
      : new SessionBookmarkStore(join(root, 'dashboard-meta.json'));
  for (const c of candidates) {
    if (++opCount % 10 === 0) {
      await new Promise((r) => setImmediate(r));
    }
    const guarded = await bookmarkStore.runIfFreshUntouched(c.sessionId, () =>
      deleteGcCandidate(c, activeSessionId, activeKasId, root, agentDelete)
    );
    if (!guarded.ok) {
      if (guarded.reason === 'user-touched') stale++;
      else failed++;
      continue;
    }
    if (guarded.value === 'deleted') deleted++;
    else if (guarded.value === 'stale') stale++;
    else failed++;
  }
  return { deleted, failed, stale };
}

type GcDeleteResult = 'deleted' | 'failed' | 'stale';

async function deleteGcCandidate(
  candidate: GcCandidate,
  activeSessionId: string | null | undefined,
  activeKasId: string | null,
  root: string,
  agentDelete?: (sessionId: string) => Promise<boolean>
): Promise<GcDeleteResult> {
  if (!stillEmpty(candidate, root)) return 'stale';

  if (candidate.store === 'kas') {
    const dirs = findKasSessionDirs(root, candidate.sessionId);
    const metadata = dirs.map((dir) =>
      readValidatedKasMetadata(dir, candidate.sessionId)
    );
    if (
      dirs.length === 0 ||
      metadata.some(
        (meta) =>
          meta === null ||
          kasPlacement(meta) !== 'local' ||
          Boolean(meta.parentSessionId)
      )
    ) {
      return 'failed';
    }
    if (dirs.some((dir) => isKasDirLocked(dir))) return 'failed';
    const activity = Math.max(...dirs.map(kasActivityMs));
    if (activity === 0 || Date.now() - activity < KAS_RECENCY_GUARD_MS) {
      return 'failed';
    }
    if (
      agentDelete &&
      normalizeSessionId(candidate.sessionId) !== activeKasId
    ) {
      let deletionLocks;
      try {
        deletionLocks = acquireSessionDeletionLocks(
          dirs.map((dir) => join(dir, '.lock'))
        );
      } catch {
        return 'failed';
      }
      try {
        const lockedDirs = findKasSessionDirs(root, candidate.sessionId);
        const lockedMetadata = lockedDirs.map((dir) =>
          readValidatedKasMetadata(dir, candidate.sessionId)
        );
        if (
          lockedDirs.length !== dirs.length ||
          lockedDirs.some((dir) => !dirs.includes(dir)) ||
          lockedMetadata.some(
            (meta) =>
              meta === null ||
              kasPlacement(meta) !== 'local' ||
              Boolean(meta.parentSessionId)
          )
        ) {
          return 'failed';
        }
        if (!stillEmpty(candidate, root)) return 'stale';
        const lockedActivity = Math.max(...lockedDirs.map(kasActivityMs));
        // Same condition as the pre-lock check: no readable timestamp is as
        // disqualifying as a recent one, or this re-validation would be the
        // weaker of the two.
        if (
          lockedActivity === 0 ||
          Date.now() - lockedActivity < KAS_RECENCY_GUARD_MS
        ) {
          return 'failed';
        }
        // An answered refusal stops the deletion — the filesystem must not
        // override the agent. A call the agent cannot serve at all throws,
        // and falls through to the store-level deletion below.
        let refused = false;
        try {
          if (await agentDelete(candidate.sessionId)) return 'deleted';
          refused = true;
        } catch (err) {
          logger.debug(
            '[session-mutations] agent delete unavailable; using the store',
            err
          );
        }
        if (refused) return 'failed';
      } finally {
        releaseSessionDeletionLocks(deletionLocks);
      }
    }
  }
  return deleteSession(
    candidate.sessionId,
    activeSessionId,
    root,
    candidate.store
  ).ok
    ? 'deleted'
    : 'failed';
}

/** Re-run the store-appropriate emptiness test for one candidate. A session
 *  counts as empty only when EVERY on-disk copy is empty. */
function stillEmpty(c: GcCandidate, root: string): boolean {
  if (c.store === 'v2') return v2IsEmpty(root, c.sessionId);
  const dirs = findKasSessionDirs(root, c.sessionId);
  return dirs.length > 0 && dirs.every(kasIsEmpty);
}
