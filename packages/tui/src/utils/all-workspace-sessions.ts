/**
 * Cross-workspace session discovery from both on-disk stores and the live
 * listing. Entries merge by normalized session ID and engine.
 */

import { existsSync, readdirSync, statSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import {
  OversizedJsonFileError,
  SESSION_METADATA_MAX_BYTES,
  readBoundedJson,
} from './bounded-json.js';
import { logger } from './logger.js';
import { listAllSessionsAllCwds } from './list-all-sessions-cli.js';
import {
  normalizeWorkspace,
  sessionIdentityKey,
  type SessionListingInput,
} from './session-dashboard.js';
import {
  containedDirectory,
  containedRegularFile,
  isValidSessionId,
  listValidatedKasSessionCopies,
  normalizeSessionId,
  resolveCanonicalKasSessionCopy,
  sessionsRoot as defaultSessionsRoot,
} from './session-store.js';

export interface SessionListingResult {
  sessions: SessionListingInput[];
  complete: boolean;
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function normalizeListingEntry(value: unknown): SessionListingInput | null {
  const input = record(value);
  if (!input || !isValidSessionId(input.sessionId)) return null;
  const messageCount =
    typeof input.messageCount === 'number' &&
    Number.isFinite(input.messageCount) &&
    input.messageCount >= 0
      ? input.messageCount
      : undefined;
  const executionTarget = record(input.executionTarget);
  const targetKind = executionTarget?.kind;
  let validTarget: SessionListingInput['executionTarget'];
  if (targetKind === 'local') validTarget = { kind: 'local' };
  else if (targetKind === 'cloud-sandbox') {
    validTarget = { kind: 'cloud-sandbox' };
  } else if (targetKind === 'remote-control') {
    validTarget = {
      kind: 'remote-control',
      ...(executionTarget && 'host' in executionTarget
        ? { host: executionTarget.host }
        : {}),
    };
  }
  const status =
    typeof input.status === 'string' &&
    [
      'idle',
      'in_progress',
      'waiting_on_user',
      'provisioning',
      'completed',
      'failed',
    ].includes(input.status)
      ? (input.status as SessionListingInput['status'])
      : undefined;
  const source =
    input.source === 'local' || input.source === 'remote'
      ? input.source
      : undefined;
  const engine =
    input.engine === 'classic' || input.engine === 'v2' || input.engine === 'v3'
      ? input.engine
      : undefined;
  const parentSessionId = isValidSessionId(input.parentSessionId)
    ? engine === 'v3' || engine === undefined
      ? normalizeSessionId(input.parentSessionId)
      : input.parentSessionId
    : undefined;
  const createdReason =
    input.createdReason === 'subagent' ||
    input.createdReason === 'rewind' ||
    input.createdReason === 'tangent'
      ? input.createdReason
      : undefined;
  const sessionId =
    engine === 'v3' || engine === undefined
      ? normalizeSessionId(input.sessionId)
      : input.sessionId;
  return {
    sessionId,
    cwd: normalizeWorkspace(input.cwd),
    ...(optionalString(input.title) !== undefined
      ? { title: optionalString(input.title) }
      : {}),
    ...(optionalString(input.updatedAt) !== undefined
      ? { updatedAt: optionalString(input.updatedAt) }
      : {}),
    ...(messageCount !== undefined ? { messageCount } : {}),
    ...(validTarget ? { executionTarget: validTarget } : {}),
    ...(source ? { source } : {}),
    ...(status ? { status } : {}),
    ...(parentSessionId ? { parentSessionId } : {}),
    ...(createdReason ? { createdReason } : {}),
    ...(engine ? { engine } : {}),
  };
}

/** Read the V2 store: flat `{uuid}.json` files with a `cwd` field. */
async function readV2Store(cliDir: string): Promise<SessionListingResult> {
  if (!existsSync(cliDir)) return { sessions: [], complete: true };
  const entries: SessionListingInput[] = [];
  let complete = true;
  let files: string[];
  try {
    files = readdirSync(cliDir).filter((f) => f.endsWith('.json'));
  } catch {
    return { sessions: [], complete: false };
  }
  for (let i = 0; i < files.length; i++) {
    // Yield every 50 files so the event loop can process input/render.
    if (i > 0 && i % 50 === 0) await yieldTick();
    const file = files[i]!;
    const sessionId = file.slice(0, -'.json'.length);
    if (!isValidSessionId(sessionId)) continue;
    try {
      const metaPath = containedRegularFile(cliDir, file);
      if (!metaPath) {
        complete = false;
        continue;
      }
      let data: Record<string, unknown> | null;
      try {
        data = record(readBoundedJson(metaPath, SESSION_METADATA_MAX_BYTES));
      } catch (err) {
        // Oversized metadata (old V2 files embed whole conversations) must
        // not hide the session or poison catalog completeness: list a
        // degraded row — the title chain and content index fill in the rest.
        if (err instanceof OversizedJsonFileError) {
          const entry = normalizeListingEntry({
            sessionId,
            cwd: '',
            updatedAt: statSync(metaPath).mtime.toISOString(),
            engine: 'v2',
          });
          if (entry) entries.push(entry);
          continue;
        }
        throw err;
      }
      if (!data) throw new Error('Session metadata must be an object');
      const parentSessionId = isValidSessionId(data.parent_session_id)
        ? data.parent_session_id
        : undefined;
      const reason = data.session_created_reason;
      // The Rust writer serde-defaults this field to 'subagent' even for
      // user-created sessions, and its own predicate requires a parent id
      // too — a reason without a parent is the default, not a derivation.
      const createdReason =
        parentSessionId && (reason === 'subagent' || reason === 'rewind')
          ? reason
          : undefined;
      const entry = normalizeListingEntry({
        sessionId,
        cwd: data.cwd,
        title: data.title,
        updatedAt: data.updated_at,
        engine: 'v2',
        parentSessionId,
        createdReason,
      });
      if (entry) entries.push(entry);
    } catch {
      complete = false;
    }
  }
  return { sessions: entries, complete };
}

/** Yield the event loop for one tick. */
function yieldTick(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

/**
 * Read the KAS-native store: `{hash}/{session-dir}/session.json`. Directory
 * names are session ids in several shapes (bare UUID, `sess_…`, `cli_…`) —
 * membership is decided by session.json, never by the name.
 */
async function readKasStore(
  sessionsRoot: string
): Promise<SessionListingResult> {
  if (!existsSync(sessionsRoot)) return { sessions: [], complete: true };
  const scanned = await listValidatedKasSessionCopies(sessionsRoot);
  const bySession = new Map<string, typeof scanned.copies>();
  for (const copy of scanned.copies) {
    const copies = bySession.get(copy.sessionId) ?? [];
    copies.push(copy);
    bySession.set(copy.sessionId, copies);
  }

  const entries: SessionListingInput[] = [];
  let complete = scanned.complete;
  let count = 0;
  for (const copies of bySession.values()) {
    if (++count % 50 === 0) await yieldTick();
    const copy = resolveCanonicalKasSessionCopy(copies);
    if (!copy) {
      complete = false;
      continue;
    }
    const data = copy.metadata;
    const workspacePaths = Array.isArray(data.workspacePaths)
      ? data.workspacePaths
      : [];
    const parentSessionId = isValidSessionId(data.parentSessionId)
      ? normalizeSessionId(data.parentSessionId)
      : undefined;
    const entry = normalizeListingEntry({
      sessionId: copy.sessionId,
      cwd: workspacePaths[0],
      title: data.title,
      updatedAt: data.lastModifiedAt ?? data.createdAt,
      parentSessionId,
      // KAS `session.json` records this as `createdReason` (camelCase); older
      // field name kept as a fallback. Feeds tangent/subagent/rewind nesting.
      createdReason: data.createdReason ?? data.sessionCreatedReason,
      executionTarget: data.executionTarget,
      source: copy.placement === 'remote' ? 'remote' : 'local',
      engine: 'v3',
    });
    if (entry) entries.push(entry);
    else complete = false;
  }
  return { sessions: entries, complete };
}

/**
 * List sessions across all workspaces from the on-disk stores.
 * `sessionsRoot` is injectable for tests; defaults to `~/.kiro/sessions`.
 */
export async function listAllWorkspaceSessionsFromDiskDetailed(
  sessionsRoot?: string
): Promise<SessionListingResult> {
  const root = sessionsRoot ?? defaultSessionsRoot();
  try {
    const cliDir = containedDirectory(root, 'cli');
    const [v2, kas] = await Promise.all([
      cliDir
        ? readV2Store(cliDir)
        : Promise.resolve({
            sessions: [],
            complete: !existsSync(join(root, 'cli')),
          }),
      readKasStore(root),
    ]);
    return {
      sessions: [...v2.sessions, ...kas.sessions],
      complete: v2.complete && kas.complete,
    };
  } catch (err) {
    logger.debug('[all-workspace-sessions] disk scan failed:', err);
    return { sessions: [], complete: false };
  }
}

export async function listAllWorkspaceSessionsFromDisk(
  sessionsRoot?: string
): Promise<SessionListingInput[]> {
  return (await listAllWorkspaceSessionsFromDiskDetailed(sessionsRoot))
    .sessions;
}

// The scan reads thousands of small JSON files (~300ms cold). The dashboard
// opens instantly with the last snapshot; a fresh scan refreshes it after.
let cachedListing: SessionListingInput[] | null = null;
let cachedListingComplete = false;
let retainedCompleteListing: SessionListingInput[] | null = null;
let scanInFlight: Promise<SessionListingResult> | null = null;
let scanGeneration = 0;

/** Last completed scan result, or empty before the first scan finishes. */
export function getCachedAllWorkspaceSessions(): SessionListingInput[] {
  return cachedListing ?? [];
}

export function getCachedAllWorkspaceSessionsResult(): SessionListingResult {
  return {
    sessions: cachedListing ?? [],
    complete: cachedListing !== null && cachedListingComplete,
  };
}

/**
 * Run the disk scan off the current tick (never blocks the toggle keypress)
 * and cache the result. Concurrent calls share one in-flight scan.
 */
export function scanAllWorkspaceSessionsDetailed(
  sessionsRoot?: string
): Promise<SessionListingResult> {
  if (scanInFlight) return scanInFlight;
  const generation = scanGeneration;
  const promise = (async () => {
    const result = await listAllWorkspaceSessionsFromDiskDetailed(sessionsRoot);
    const published =
      !result.complete && retainedCompleteListing
        ? {
            sessions: mergeSessionListings(
              result.sessions,
              retainedCompleteListing
            ),
            complete: false,
          }
        : result;
    if (generation === scanGeneration) {
      cachedListing = published.sessions;
      cachedListingComplete = published.complete;
      if (result.complete) retainedCompleteListing = result.sessions;
    }
    return published;
  })().finally(() => {
    if (scanInFlight === promise) scanInFlight = null;
  });
  scanInFlight = promise;
  return promise;
}

export async function scanAllWorkspaceSessions(
  sessionsRoot?: string
): Promise<SessionListingInput[]> {
  return (await scanAllWorkspaceSessionsDetailed(sessionsRoot)).sessions;
}

/** Invalidate active generations while retaining the last complete snapshots. */
export function invalidateAllWorkspaceSessionsCache(): void {
  scanGeneration++;
  cachedListing = null;
  cachedListingComplete = false;
  scanInFlight = null;
  classicGeneration++;
  cachedClassic = null;
  cachedClassicComplete = false;
  classicInFlight = null;
}

/** Fully clear listing state for process teardown and test isolation. */
export function resetAllWorkspaceSessionsCache(): void {
  invalidateAllWorkspaceSessionsCache();
  retainedCompleteListing = null;
  retainedCompleteClassic = null;
}

// Classic (V1) sessions live in the Rust binary's SQLite store, unreadable
// from TypeScript. The binary's `--list-sessions --all-cwds` merge is the
// sanctioned reader; only its classic rows are consumed — V2 rows duplicate
// the (faster, lineage-bearing) disk scan and KAS rows come from the live
// client. Cached like the disk scan: the last result renders instantly,
// a background refresh replaces it.
let cachedClassic: SessionListingInput[] | null = null;
let cachedClassicComplete = false;
let retainedCompleteClassic: SessionListingInput[] | null = null;
let classicInFlight: Promise<SessionListingResult> | null = null;
let classicGeneration = 0;

/** Last completed classic fetch, or empty before the first completes. */
export function getCachedClassicSessions(): SessionListingInput[] {
  return cachedClassic ?? [];
}

export function getCachedClassicSessionsResult(): SessionListingResult {
  return {
    sessions: cachedClassic ?? [],
    complete: cachedClassic !== null && cachedClassicComplete,
  };
}

/**
 * Fetch classic (V1) sessions across every workspace by shelling out to the
 * binary's merged listing. Failures resolve to the last cached result — the
 * dashboard treats classic rows as an enrichment, never a gate.
 */
export function fetchClassicSessionsDetailed(
  fetchListing: typeof listAllSessionsAllCwds = listAllSessionsAllCwds
): Promise<SessionListingResult> {
  if (classicInFlight) return classicInFlight;
  const generation = classicGeneration;
  const fallback = (): SessionListingResult => ({
    sessions: cachedClassic ?? retainedCompleteClassic ?? [],
    complete: false,
  });
  const promise = (async () => {
    try {
      const r = await fetchListing();
      if (!r.ok) {
        logger.debug('[all-workspace-sessions] classic fetch failed:', r.error);
        return fallback();
      }
      const rows: SessionListingInput[] = [];
      let complete = r.complete;
      for (const env of r.envelopes) {
        for (const s of env.sessions) {
          if (s.source !== 'classic') continue;
          const row = normalizeListingEntry({
            sessionId: s.sessionId,
            cwd: env.cwd,
            title: s.title,
            updatedAt: s.updatedAt,
            messageCount: s.messageCount,
            engine: 'classic',
          });
          if (row) rows.push(row);
          else complete = false;
        }
      }
      const result = { sessions: rows, complete };
      const published =
        !complete && retainedCompleteClassic
          ? {
              sessions: mergeSessionListings(rows, retainedCompleteClassic),
              complete: false,
            }
          : result;
      if (generation === classicGeneration) {
        cachedClassic = published.sessions;
        cachedClassicComplete = published.complete;
        if (complete) retainedCompleteClassic = rows;
      }
      return published;
    } catch (err) {
      logger.debug('[all-workspace-sessions] classic fetch failed:', err);
      return fallback();
    }
  })().finally(() => {
    if (classicInFlight === promise) classicInFlight = null;
  });
  classicInFlight = promise;
  return promise;
}

export async function fetchClassicSessions(): Promise<SessionListingInput[]> {
  return (await fetchClassicSessionsDetailed()).sessions;
}

function normalizeListingResult(
  values: readonly unknown[],
  complete: boolean
): SessionListingResult {
  const sessions: SessionListingInput[] = [];
  for (const value of values) {
    const entry = normalizeListingEntry(value);
    if (entry) sessions.push(entry);
    else complete = false;
  }
  return { sessions, complete };
}

/** Prefer an all-workspace live listing and retain cwd-scoped compatibility. */
export async function listLiveDashboardSessionsDetailed(
  sessionLister: {
    listAllWorkspaceSessions?(): Promise<{
      sessions: SessionListingInput[];
      failed?: boolean;
      complete?: boolean;
    }>;
    listSessions(cwd: string): Promise<{
      sessions: SessionListingInput[];
      failed?: boolean;
    }>;
  },
  cwd: string
): Promise<SessionListingResult> {
  if (sessionLister.listAllWorkspaceSessions) {
    try {
      const all = await sessionLister.listAllWorkspaceSessions();
      if (!all.failed) {
        return normalizeListingResult(all.sessions, all.complete !== false);
      }
    } catch (err) {
      logger.debug(
        '[all-workspace-sessions] all-workspace listing failed:',
        err
      );
    }
  }
  try {
    const scoped = await sessionLister.listSessions(cwd);
    return normalizeListingResult(scoped.sessions, false);
  } catch (err) {
    logger.debug('[all-workspace-sessions] live listing failed:', err);
    return { sessions: [], complete: false };
  }
}

export async function listLiveDashboardSessions(
  sessionLister: Parameters<typeof listLiveDashboardSessionsDetailed>[0],
  cwd: string
): Promise<SessionListingInput[]> {
  return (await listLiveDashboardSessionsDetailed(sessionLister, cwd)).sessions;
}

/**
 * Merge the live all-workspace listing with the disk scan.
 * Live entries win on sessionId conflicts — they carry status and
 * execution-target metadata the disk records lack.
 */
export function mergeSessionListings(
  live: SessionListingInput[],
  disk: SessionListingInput[]
): SessionListingInput[] {
  // One session, several id spellings: the disk scan and live listing can
  // disagree on the `sess_` prefix. All producers normalize now, but the
  // live RPC listing is outside our control — key on the normalized id
  // (duplicate ids also break the renderer's keys, leaving ghost rows).
  const dedupeKey = (entry: SessionListingInput): string =>
    sessionIdentityKey(entry);
  const byId = new Map<string, SessionListingInput>();
  for (const value of disk) {
    const entry = normalizeListingEntry(value);
    if (!entry) continue;
    const key = dedupeKey(entry);
    const existing = byId.get(key);
    byId.set(
      key,
      existing ? mergeListingEntry(existing, entry) : canonicalEntry(entry)
    );
  }
  for (const value of live) {
    const entry = normalizeListingEntry(value);
    if (!entry) continue;
    const key = dedupeKey(entry);
    const existing = byId.get(key);
    byId.set(
      key,
      existing ? mergeListingEntry(existing, entry) : canonicalEntry(entry)
    );
  }
  return [...byId.values()];
}

function canonicalEntry(entry: SessionListingInput): SessionListingInput {
  return { ...entry, cwd: normalizeWorkspace(entry.cwd ?? '') };
}

function cwdQuality(cwd: string | undefined): number {
  const raw = cwd?.trim() ?? '';
  if (!raw || raw === '.') return 0;
  if (raw === '~') return 1;
  const normalized = normalizeWorkspace(raw);
  if (normalized === normalizeWorkspace('~')) return 1;
  return isAbsolute(normalized) || /^[A-Za-z]:[\\/]/.test(normalized) ? 2 : 1;
}

function mergeListingEntry(
  existing: SessionListingInput,
  incoming: SessionListingInput
): SessionListingInput {
  const merged = {
    ...existing,
    ...pruneUndefined(incoming),
  } as SessionListingInput;
  merged.cwd =
    cwdQuality(incoming.cwd) >= cwdQuality(existing.cwd)
      ? normalizeWorkspace(incoming.cwd ?? '')
      : normalizeWorkspace(existing.cwd ?? '');
  return merged;
}

function pruneUndefined<T extends object>(obj: T): Partial<T> {
  const out: Partial<T> = {};
  for (const [k, v] of Object.entries(obj)) {
    if (v !== undefined) (out as Record<string, unknown>)[k] = v;
  }
  return out;
}
