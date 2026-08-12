/**
 * On-disk session store layout — the single owner of path knowledge shared
 * by the dashboard's scan/search/mutation/lock modules.
 *
 * Two stores live under the sessions root:
 * - V2 (Rust agent): flat `cli/{id}.json` metadata + `cli/{id}.jsonl` log.
 * - KAS-native: `{workspace-hash}/{session-dir}/session.json` + transcripts.
 */

import {
  existsSync,
  lstatSync,
  readdirSync,
  realpathSync,
  statSync,
} from 'node:fs';
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';
import { SESSION_METADATA_MAX_BYTES, readBoundedJson } from './bounded-json.js';
import { kiroHomePath } from './kiro-home.js';

/** Root of both stores (test override via KIRO_TEST_SESSIONS_ROOT). */
export function sessionsRoot(): string {
  return process.env.KIRO_TEST_SESSIONS_ROOT ?? kiroHomePath('sessions');
}

/** One session, several id spellings: normalize KAS's optional prefix. */
export function normalizeSessionId(id: string): string {
  return id.replace(/^sess_/, '');
}

/** Bind KAS metadata to the physical directory that destructive actions use. */
export function validatedKasSessionId(
  dirName: string,
  metadataId: unknown
): string | null {
  if (!isValidSessionId(dirName) || !isValidSessionId(metadataId)) return null;
  const physicalId = normalizeSessionId(dirName);
  return normalizeSessionId(metadataId) === physicalId ? physicalId : null;
}

/**
 * Session ids cross a filesystem boundary, so they are restricted to one
 * portable path segment. In particular, separators and traversal spellings
 * must never reach path construction, even when metadata is corrupt.
 */
export function isValidSessionId(id: unknown): id is string {
  if (typeof id !== 'string') return false;
  const normalized = normalizeSessionId(id);
  return (
    normalized.length > 0 &&
    normalized.length <= 256 &&
    /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(normalized) &&
    !normalized.includes('..')
  );
}

/** Resolve a descendant and reject lexical or symlink escapes. */
export function containedPath(root: string, ...parts: string[]): string | null {
  const lexicalBase = resolve(root);
  const lexicalCandidate = resolve(lexicalBase, ...parts);
  const lexicalRel = relative(lexicalBase, lexicalCandidate);
  if (
    lexicalRel !== '' &&
    (lexicalRel.startsWith('..') || isAbsolute(lexicalRel))
  ) {
    return null;
  }

  try {
    const canonicalBase = realpathSync(lexicalBase);
    let existing = lexicalCandidate;
    const missingParts: string[] = [];
    while (!existsSync(existing)) {
      const parent = dirname(existing);
      if (parent === existing) return null;
      missingParts.unshift(basename(existing));
      existing = parent;
    }
    const canonicalCandidate = resolve(realpathSync(existing), ...missingParts);
    const canonicalRel = relative(canonicalBase, canonicalCandidate);
    if (
      canonicalRel === '' ||
      (!canonicalRel.startsWith('..') && !isAbsolute(canonicalRel))
    ) {
      return canonicalCandidate;
    }
  } catch {
    return null;
  }
  return null;
}

/** Resolve a descendant without following symbolic links below the root. */
function containedEntryPath(
  root: string,
  parts: string[],
  allowMissing: boolean,
  expected?: 'file' | 'directory'
): string | null {
  const lexicalBase = resolve(root);
  const candidate = resolve(lexicalBase, ...parts);
  const rel = relative(lexicalBase, candidate);
  if (rel !== '' && (rel.startsWith('..') || isAbsolute(rel))) return null;

  let current = lexicalBase;
  const segments = rel === '' ? [] : rel.split(sep);
  for (let index = 0; index < segments.length; index++) {
    current = join(current, segments[index]!);
    try {
      const stat = lstatSync(current);
      if (stat.isSymbolicLink()) return null;
      if (index < segments.length - 1 && !stat.isDirectory()) return null;
      if (index === segments.length - 1) {
        if (expected === 'file' && !stat.isFile()) return null;
        if (expected === 'directory' && !stat.isDirectory()) return null;
      }
    } catch {
      return allowMissing ? candidate : null;
    }
  }
  return containedPath(lexicalBase, ...parts) ? candidate : null;
}

/** Existing regular file contained beneath a root without symlink traversal. */
export function containedRegularFile(
  root: string,
  ...parts: string[]
): string | null {
  return containedEntryPath(root, parts, false, 'file');
}

/** Existing directory contained beneath a root without symlink traversal. */
export function containedDirectory(
  root: string,
  ...parts: string[]
): string | null {
  return containedEntryPath(root, parts, false, 'directory');
}

/** File path that may be absent but whose existing ancestors are safe. */
export function containedFilePath(
  root: string,
  ...parts: string[]
): string | null {
  return containedEntryPath(root, parts, true, 'file');
}

/** Safely construct a V2 session file path. */
export function v2SessionPath(
  root: string,
  sessionId: string,
  extension: '.json' | '.jsonl' | '.history' | '.lock'
): string | null {
  if (!isValidSessionId(sessionId)) return null;
  return containedFilePath(root, 'cli', `${sessionId}${extension}`);
}

/** KAS workspace-hash directories under a sessions root (excludes V2). */
export function kasHashDirs(root: string): string[] {
  let entries: string[];
  try {
    entries = readdirSync(root);
  } catch {
    return [];
  }
  return entries.filter((d) => {
    if (d === 'cli') return false;
    try {
      return lstatSync(join(root, d)).isDirectory();
    } catch {
      return false;
    }
  });
}

function kasSessionPath(
  root: string,
  hash: string,
  dirName: string
): string | null {
  const lexicalPath = join(root, hash, dirName);
  try {
    if (!lstatSync(lexicalPath).isDirectory()) return null;
  } catch {
    return null;
  }
  const candidate = containedDirectory(root, hash, dirName);
  return candidate && containedRegularFile(candidate, 'session.json')
    ? candidate
    : null;
}

export type KasSessionPlacement = 'local' | 'remote';

export interface ValidatedKasSessionCopy {
  dir: string;
  sessionId: string;
  metadata: Record<string, unknown>;
  placement: KasSessionPlacement;
  hasWorkspace: boolean;
  activityMs: number;
}

export interface ValidatedKasSessionCopies {
  copies: ValidatedKasSessionCopy[];
  complete: boolean;
}

function kasPlacement(
  metadata: Record<string, unknown>
): KasSessionPlacement | null {
  if (!Object.prototype.hasOwnProperty.call(metadata, 'executionTarget')) {
    return 'local';
  }
  const target = metadata.executionTarget;
  if (!target || typeof target !== 'object' || Array.isArray(target)) {
    return null;
  }
  const kind = (target as { kind?: unknown }).kind;
  if (typeof kind !== 'string' || !kind) return null;
  return kind === 'local' ? 'local' : 'remote';
}

function kasActivityMs(dir: string, metadata: Record<string, unknown>): number {
  let newest = 0;
  for (const value of [metadata.lastModifiedAt, metadata.createdAt]) {
    if (typeof value !== 'string') continue;
    const parsed = Date.parse(value);
    if (Number.isFinite(parsed)) newest = Math.max(newest, parsed);
  }
  for (const path of [join(dir, 'session.json'), join(dir, 'messages.jsonl')]) {
    try {
      newest = Math.max(newest, statSync(path).mtimeMs);
    } catch {
      /* absent files contribute no activity */
    }
  }
  return newest;
}

function readKasSessionCopy(
  root: string,
  hash: string,
  dirName: string
): ValidatedKasSessionCopy | null {
  const dir = kasSessionPath(root, hash, dirName);
  if (!dir) return null;
  const metaPath = containedRegularFile(dir, 'session.json');
  if (!metaPath) return null;
  try {
    const value = readBoundedJson(metaPath, SESSION_METADATA_MAX_BYTES);
    if (!value || typeof value !== 'object' || Array.isArray(value))
      return null;
    const metadata = value as Record<string, unknown>;
    const sessionId = validatedKasSessionId(dirName, metadata.id);
    const placement = kasPlacement(metadata);
    if (!sessionId || !placement) return null;
    const workspacePaths = metadata.workspacePaths;
    return {
      dir,
      sessionId,
      metadata,
      placement,
      hasWorkspace:
        Array.isArray(workspacePaths) &&
        workspacePaths.some(
          (workspace) => typeof workspace === 'string' && workspace.trim()
        ),
      activityMs: kasActivityMs(dir, metadata),
    };
  } catch {
    return null;
  }
}

/** Enumerate validated KAS copies and report whether every candidate was readable. */
export function listValidatedKasSessionCopies(
  root: string
): ValidatedKasSessionCopies {
  const copies: ValidatedKasSessionCopy[] = [];
  let complete = true;
  let hashes: string[];
  try {
    hashes = readdirSync(root).filter((entry) => entry !== 'cli');
  } catch {
    return { copies, complete: !existsSync(root) };
  }
  for (const hash of hashes.sort()) {
    const lexicalHash = join(root, hash);
    let hashStat;
    try {
      hashStat = lstatSync(lexicalHash);
    } catch {
      complete = false;
      continue;
    }
    if (hashStat.isSymbolicLink()) {
      complete = false;
      continue;
    }
    if (!hashStat.isDirectory()) continue;
    const hashDir = containedDirectory(root, hash);
    if (!hashDir) {
      complete = false;
      continue;
    }
    let dirNames: string[];
    try {
      dirNames = readdirSync(hashDir);
    } catch {
      complete = false;
      continue;
    }
    for (const dirName of dirNames.sort()) {
      const copy = readKasSessionCopy(root, hash, dirName);
      if (copy) {
        copies.push(copy);
        continue;
      }
      // A subdir with no session.json is a sibling artifact (e.g. a
      // `workflows` dir), not a malformed session — skip it without
      // reporting the listing incomplete. Only a session.json that is
      // present but unreadable/uncontained counts as an incomplete copy.
      if (existsSync(join(root, hash, dirName, 'session.json'))) {
        complete = false;
      }
    }
  }
  return { copies, complete };
}

/** Pick one deterministic validated copy for every read-side surface. */
export function resolveCanonicalKasSessionCopy(
  copies: readonly ValidatedKasSessionCopy[]
): ValidatedKasSessionCopy | null {
  return (
    [...copies].sort(
      (a, b) =>
        Number(b.hasWorkspace) - Number(a.hasWorkspace) ||
        b.activityMs - a.activityMs ||
        a.dir.localeCompare(b.dir)
    )[0] ?? null
  );
}

/**
 * Validated copies of ONE session id, probed directly under each hash dir
 * instead of reading every session.json in the store. Locating a single
 * session (preview, turn tree, mutations) must not cost a whole-store scan —
 * on a large store that read+parse of thousands of files is a per-highlight
 * stall. At most two candidate dir names (bare and `sess_`-prefixed) are
 * checked per hash, and only the ones that exist are read.
 */
function listKasSessionCopiesById(
  root: string,
  sessionId: string
): ValidatedKasSessionCopy[] {
  const normalized = normalizeSessionId(sessionId);
  const dirNames = [normalized, `sess_${normalized}`];
  let hashes: string[];
  try {
    hashes = readdirSync(root).filter((entry) => entry !== 'cli');
  } catch {
    return [];
  }
  const copies: ValidatedKasSessionCopy[] = [];
  for (const hash of hashes) {
    for (const dirName of dirNames) {
      const copy = readKasSessionCopy(root, hash, dirName);
      if (copy && copy.sessionId === normalized) copies.push(copy);
    }
  }
  return copies;
}

/** Locate the canonical validated KAS-native session copy by id. */
export function findKasSessionCopy(
  root: string,
  sessionId: string
): ValidatedKasSessionCopy | null {
  if (!isValidSessionId(sessionId)) return null;
  return resolveCanonicalKasSessionCopy(
    listKasSessionCopiesById(root, sessionId)
  );
}

/** Locate a KAS-native session directory by id; null when absent/invalid. */
export function findKasSessionDir(
  root: string,
  sessionId: string
): string | null {
  return findKasSessionCopy(root, sessionId)?.dir ?? null;
}

/** Every validated directory holding this session across workspace buckets. */
export function findKasSessionDirs(root: string, sessionId: string): string[] {
  if (!isValidSessionId(sessionId)) return [];
  return listKasSessionCopiesById(root, sessionId)
    .map((copy) => copy.dir)
    .sort();
}
