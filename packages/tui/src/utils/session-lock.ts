/** Cross-process KAS session locks with transactional ownership transfer. */

import {
  closeSync,
  existsSync,
  linkSync,
  lstatSync,
  mkdirSync,
  openSync,
  readdirSync,
  rmdirSync,
  unlinkSync,
} from 'node:fs';
import { join } from 'node:path';
import {
  LOCK_METADATA_MAX_BYTES,
  readBoundedJson,
  writeFileFully,
} from './bounded-json.js';
import {
  findKasSessionDirs,
  isValidSessionId,
  sessionsRoot,
  v2SessionPath,
} from './session-store.js';

interface LockPayload {
  pid: number;
  startedAt: string;
}

type LockState =
  | { kind: 'absent' }
  | { kind: 'owned'; payload: LockPayload }
  | { kind: 'live'; payload: LockPayload }
  | { kind: 'stale'; payload: LockPayload }
  | { kind: 'unknown' };

function kasLockPath(sessionDir: string): string {
  return join(sessionDir, '.lock');
}

function v2LockPath(sessionId: string): string | null {
  return v2SessionPath(sessionsRoot(), sessionId, '.lock');
}

function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return (err as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** Malformed/unreadable files are unknown and fail closed. */
function readLockState(path: string): LockState {
  if (!existsSync(path)) return { kind: 'absent' };
  try {
    const raw = readBoundedJson(
      path,
      LOCK_METADATA_MAX_BYTES
    ) as Partial<LockPayload>;
    const pid = Number(raw.pid);
    if (!Number.isSafeInteger(pid) || pid <= 0) return { kind: 'unknown' };
    const payload = {
      pid,
      startedAt: typeof raw.startedAt === 'string' ? raw.startedAt : '',
    };
    if (pid === process.pid) return { kind: 'owned', payload };
    return isPidAlive(pid)
      ? { kind: 'live', payload }
      : { kind: 'stale', payload };
  } catch {
    return { kind: 'unknown' };
  }
}

export interface SessionLockInfo {
  locked: true;
  /** Absent when the lock exists but is malformed or unreadable. */
  pid?: number;
  startedAt: string;
  state: 'live' | 'unknown';
}

function lockInfo(state: LockState): SessionLockInfo | null {
  if (state.kind === 'live') {
    return {
      locked: true,
      pid: state.payload.pid,
      startedAt: state.payload.startedAt,
      state: 'live',
    };
  }
  if (state.kind === 'unknown') {
    return { locked: true, startedAt: '', state: 'unknown' };
  }
  return null;
}

export function formatSessionLockOwner(info: SessionLockInfo): string {
  return info.pid == null ? 'an unreadable lock' : `PID ${info.pid}`;
}

export function isSessionLocked(
  sessionId: string,
  identity?: {
    engine?: 'classic' | 'v2' | 'v3';
    source?: 'local' | 'remote';
  }
): SessionLockInfo | null {
  if (!isValidSessionId(sessionId)) {
    return { locked: true, startedAt: '', state: 'unknown' };
  }
  if (identity?.source === 'remote' || identity?.engine === 'classic') {
    return null;
  }
  if (identity?.engine !== 'v2') {
    for (const kasDir of findKasSessionDirs(sessionsRoot(), sessionId)) {
      const info = isKasDirLocked(kasDir);
      if (info) return info;
    }
  }
  if (identity?.engine === 'v3') return null;
  const v2Path = v2LockPath(sessionId);
  return v2Path ? lockInfo(readLockState(v2Path)) : null;
}

export function isKasDirLocked(sessionDir: string): SessionLockInfo | null {
  return lockInfo(readLockState(kasLockPath(sessionDir)));
}

interface HeldSessionLock {
  sessionId: string;
  paths: readonly string[];
}

let heldLock: HeldSessionLock | null = null;
const pendingPaths = new Set<string>();
let tempCounter = 0;

function removeOwned(path: string): void {
  if (readLockState(path).kind !== 'owned') return;
  try {
    unlinkSync(path);
  } catch {
    /* ownership was already released */
  }
}

/** Publish the complete payload with an exclusive hard link, never an empty file. */
function createLock(path: string): void {
  const payload = JSON.stringify({
    pid: process.pid,
    startedAt: new Date().toISOString(),
  } satisfies LockPayload);
  const temp = `${path}.${process.pid}.${Date.now()}.${tempCounter++}.tmp`;
  const fd = openSync(temp, 'wx', 0o600);
  try {
    try {
      writeFileFully(fd, Buffer.from(payload));
    } finally {
      closeSync(fd);
    }
    linkSync(temp, path);
  } finally {
    try {
      unlinkSync(temp);
    } catch {
      /* temporary link was already removed */
    }
  }
}

function acquirePathGuarded(path: string): boolean {
  let state = readLockState(path);
  if (state.kind === 'owned') return false;
  if (state.kind === 'live') {
    throw new Error(
      `Session is open in another terminal (PID ${state.payload.pid})`
    );
  }
  if (state.kind === 'unknown') {
    throw new Error('Session lock is malformed or unreadable');
  }
  if (state.kind === 'stale') {
    // Re-read immediately before unlinking: another implementation (the
    // Rust agent reclaims stale locks too) may have replaced this lock
    // since it was read, and unlinking would steal its fresh lock.
    const recheck = readLockState(path);
    if (
      recheck.kind !== 'stale' ||
      recheck.payload.pid !== state.payload.pid ||
      recheck.payload.startedAt !== state.payload.startedAt
    ) {
      throw new Error('Session lock changed while acquiring it');
    }
    try {
      unlinkSync(path);
    } catch {
      state = readLockState(path);
      if (state.kind !== 'absent') {
        throw new Error('Session lock changed while acquiring it');
      }
    }
  }
  try {
    createLock(path);
    return true;
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== 'EEXIST') throw err;
    state = readLockState(path);
    if (state.kind === 'live') {
      throw new Error(
        `Session is open in another terminal (PID ${state.payload.pid})`,
        { cause: err }
      );
    }
    if (state.kind === 'owned') return false;
    if (state.kind === 'unknown') {
      throw new Error('Session lock is malformed or unreadable', {
        cause: err,
      });
    }
    throw new Error('Session lock changed while acquiring it', { cause: err });
  }
}

function acquirePath(path: string): boolean {
  const guardDir = `${path}.acquire`;
  try {
    mkdirSync(guardDir);
  } catch (err) {
    if (
      (err as NodeJS.ErrnoException).code !== 'EEXIST' ||
      !lstatSync(guardDir).isDirectory() ||
      lstatSync(guardDir).isSymbolicLink()
    ) {
      throw new Error(
        'Session lock acquisition guard is malformed or unreadable',
        {
          cause: err,
        }
      );
    }
  }
  const contender = join(
    guardDir,
    `${process.pid}-${Date.now()}-${tempCounter++}.lock`
  );
  createLock(contender);
  try {
    for (const entry of readdirSync(guardDir)) {
      const candidate = join(guardDir, entry);
      if (candidate === contender) continue;
      const state = readLockState(candidate);
      if (state.kind === 'stale') {
        try {
          unlinkSync(candidate);
        } catch {
          /* another contender already reclaimed it */
        }
        continue;
      }
      if (state.kind === 'absent') continue;
      if (state.kind === 'live' || state.kind === 'owned') {
        throw new Error('Session lock acquisition is already in progress');
      }
      throw new Error(
        'Session lock acquisition guard is malformed or unreadable'
      );
    }
    return acquirePathGuarded(path);
  } finally {
    removeOwned(contender);
    try {
      rmdirSync(guardDir);
    } catch {
      /* another contender still owns the guard directory */
    }
  }
}

export interface SessionLockTransfer {
  readonly sessionId: string;
  readonly targetPaths: readonly string[];
  readonly previous: HeldSessionLock | null;
  readonly acquiredPaths: readonly string[];
  settled: boolean;
}

/** Acquire a local target while retaining prior ownership until RPC success. */
export function beginSessionLockTransfer(
  sessionId: string,
  source: 'local' | 'remote' = 'local'
): SessionLockTransfer {
  if (!isValidSessionId(sessionId)) throw new Error('Invalid session id');
  const targetPaths =
    source === 'remote'
      ? []
      : [
          ...new Set(
            findKasSessionDirs(sessionsRoot(), sessionId).map((dir) =>
              kasLockPath(dir)
            )
          ),
        ].sort();
  const previous = heldLock;
  const previousPaths = new Set(previous?.paths ?? []);
  const acquiredPaths: string[] = [];
  try {
    for (const path of targetPaths) {
      if (previousPaths.has(path)) continue;
      if (!acquirePath(path)) {
        throw new Error('Session lock is already held by this process');
      }
      acquiredPaths.push(path);
      pendingPaths.add(path);
    }
  } catch (error) {
    for (const path of acquiredPaths) {
      pendingPaths.delete(path);
      removeOwned(path);
    }
    throw error;
  }
  return {
    sessionId,
    targetPaths,
    previous,
    acquiredPaths,
    settled: false,
  };
}

export function commitSessionLockTransfer(transfer: SessionLockTransfer): void {
  if (transfer.settled) return;
  transfer.settled = true;
  for (const path of transfer.acquiredPaths) pendingPaths.delete(path);
  heldLock =
    transfer.targetPaths.length > 0
      ? { sessionId: transfer.sessionId, paths: transfer.targetPaths }
      : null;
  const targetPaths = new Set(transfer.targetPaths);
  for (const path of transfer.previous?.paths ?? []) {
    if (!targetPaths.has(path)) removeOwned(path);
  }
}

export function rollbackSessionLockTransfer(
  transfer: SessionLockTransfer
): void {
  if (transfer.settled) return;
  transfer.settled = true;
  for (const path of transfer.acquiredPaths) {
    pendingPaths.delete(path);
    removeOwned(path);
  }
}

/** Immediate acquisition for startup/new-session paths. */
export function acquireSessionLock(sessionId: string): void {
  const transfer = beginSessionLockTransfer(sessionId);
  commitSessionLockTransfer(transfer);
}

export interface SessionDeletionLocks {
  readonly acquiredPaths: readonly string[];
  released: boolean;
}

/** Hold exclusive path ownership until a destructive operation finishes. */
export function acquireSessionDeletionLocks(
  lockPaths: readonly string[]
): SessionDeletionLocks {
  const acquiredPaths: string[] = [];
  try {
    for (const path of [...new Set(lockPaths)].sort()) {
      if (!acquirePath(path)) {
        throw new Error('Session lock is already held by this process');
      }
      acquiredPaths.push(path);
    }
    return { acquiredPaths, released: false };
  } catch (err) {
    for (const path of acquiredPaths.reverse()) removeOwned(path);
    throw err;
  }
}

export function releaseSessionDeletionLocks(locks: SessionDeletionLocks): void {
  if (locks.released) return;
  locks.released = true;
  for (const path of locks.acquiredPaths) removeOwned(path);
}

export function releaseSessionLock(): void {
  for (const path of heldLock?.paths ?? []) removeOwned(path);
  heldLock = null;
  for (const path of pendingPaths) removeOwned(path);
  pendingPaths.clear();
}

let exitRegistered = false;
export function registerLockCleanup(): void {
  if (exitRegistered) return;
  exitRegistered = true;
  process.on('exit', () => releaseSessionLock());
  for (const [sig, code] of [
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ] as const) {
    if (process.listenerCount(sig) === 0) {
      process.on(sig, () => {
        releaseSessionLock();
        process.exit(code);
      });
    }
  }
}
