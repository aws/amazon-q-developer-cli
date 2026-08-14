/**
 * Shell out to `kiro-cli chat --list-sessions --format json` and
 * parse the merged V1 + V2 + KAS listing for the current cwd.
 *
 * Used by the cross-engine resume and `/chat` picker paths, which
 * need a single source of truth that sees every session on disk
 * regardless of the active engine. The Rust binary already merges
 * the three sources for the text-mode output; this helper is the
 * structured-JSON consumer of that same merge.
 *
 * Output contract from the binary (see
 * `crates/chat-cli/src/cli/chat/cli/persist.rs::SessionListingJson`):
 *
 *   `[{"cwd": <abs>, "sessions": [{"sessionId", "source",
 *      "title", "updatedAt", "messageCount"?}, ...]}]`
 *
 * Single-element array today; the wrapper leaves room for a future
 * `--all-cwds` flag.
 */

import { spawn } from 'node:child_process';

import { resolveChatCliBinFromEnv } from './chat-cli-bin';

/** Engine that backs a session entry. */
export type SessionSource = 'classic' | 'v2' | 'v3';

/** One merged session entry as the binary serializes it. */
export interface SessionEntry {
  sessionId: string;
  source: SessionSource;
  title: string;
  /** RFC3339 millis-precision timestamp, e.g. `2026-05-31T18:00:00.000Z`. */
  updatedAt: string;
  /** Omitted by the binary when the source did not report a count (KAS). */
  messageCount?: number;
  /**
   * WHERE the session's agent runs (`'local'` | `'cloud-sandbox'`), from the V3
   * row's `_meta.kiro.executionTarget.kind`. Omitted by the binary for local
   * rows (V1/V2, and V3 without the field), so absence == local. Drives the
   * picker's cloud WHERE tag. Remove this field once every row carries
   * `executionTarget`; until then it is harmless (absence == local).
   */
  executionTarget?: string;
  /**
   * Coarse activity status snapshot (`_meta.kiro.status`): `idle` |
   * `in_progress` | `waiting_on_user` | `completed` | `failed` | `provisioning`.
   * Omitted by the binary for rows without one (V1/V2, and V3 rows lacking it).
   * Drives the picker's state column when a cloud row is present.
   */
  status?: string;
}

/** Result envelope: `ok: true` with entries, or `ok: false` with a message. */
export type ListAllSessionsResult =
  | { ok: true; cwd: string; sessions: SessionEntry[] }
  | { ok: false; error: string };

/** All-cwds result: one `{cwd, sessions}` envelope per workspace. */
export type ListAllCwdsResult =
  | {
      ok: true;
      envelopes: { cwd: string; sessions: SessionEntry[] }[];
      complete: boolean;
    }
  | { ok: false; error: string };

/**
 * Spawn signature carved out as a parameter so unit tests can drive
 * deterministic stdout / exit codes without invoking a real binary.
 * The optional signal aborts the child process (timeout enforcement).
 */
export type AsyncSpawner = (
  bin: string,
  args: string[],
  signal?: AbortSignal
) => Promise<{
  exitCode: number | null;
  stdout: string;
  stderr: string;
  error?: Error;
}>;

let listAllSessionsOverrideForTests:
  | (() => Promise<ListAllSessionsResult>)
  | undefined;

export function __setListAllSessionsOverrideForTests(
  override: (() => Promise<ListAllSessionsResult>) | undefined
): void {
  listAllSessionsOverrideForTests = override;
}

const MAX_CHILD_OUTPUT_BYTES = 4 * 1024 * 1024;

const DEFAULT_SPAWNER: AsyncSpawner = (bin, args, signal) =>
  new Promise((resolve) => {
    const child = spawn(bin, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      signal,
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const finish = (result: Awaited<ReturnType<AsyncSpawner>>) => {
      if (settled) return;
      settled = true;
      resolve(result);
    };
    const append = (current: string, chunk: Buffer): string => {
      const next = current + chunk.toString();
      if (Buffer.byteLength(next) > MAX_CHILD_OUTPUT_BYTES) {
        child.kill();
        finish({
          exitCode: null,
          stdout,
          stderr,
          error: new Error('kiro-cli output exceeded 4 MiB'),
        });
        return current;
      }
      return next;
    };
    child.stdout.on('data', (chunk: Buffer) => {
      stdout = append(stdout, chunk);
    });
    child.stderr.on('data', (chunk: Buffer) => {
      stderr = append(stderr, chunk);
    });
    child.on('error', (error) =>
      finish({ exitCode: null, stdout, stderr, error })
    );
    child.on('close', (code) => finish({ exitCode: code, stdout, stderr }));
  });

/** Run a spawner under a deadline that also KILLS the child — a timeout
 *  that only stops waiting leaks a whole kiro-cli → KAS process tree. */
async function spawnWithDeadline(
  spawner: AsyncSpawner,
  bin: string,
  args: string[],
  timeoutMs: number,
  label: string
): Promise<Awaited<ReturnType<AsyncSpawner>>> {
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), timeoutMs);
  const timeoutError = new Error(
    `${label} did not complete within ${timeoutMs}ms`
  );
  // Backstop for spawners that ignore the abort signal: give the kill a
  // grace second to surface through the child's close event, then bail.
  const backstop = new Promise<never>((_, reject) => {
    ac.signal.addEventListener(
      'abort',
      () => setTimeout(() => reject(timeoutError), 1000),
      { once: true }
    );
  });
  try {
    const result = await Promise.race([
      spawner(bin, args, ac.signal),
      backstop,
    ]);
    if (ac.signal.aborted) throw timeoutError;
    return result;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Execute `kiro-cli chat --list-sessions --format json` and parse
 * the response.
 *
 * Honors `KIRO_CHAT_CLI_BIN` to locate the binary - the same env
 * var used by every other shell-out path in the TUI.
 *
 * `timeoutMs` (default 10s) bounds the spawn. The binary spawns a
 * KAS child internally for the listing, which can take 2-3s on a
 * cold start; 10s leaves headroom without pinning the picker if
 * something is wrong.
 */
export async function listAllSessions(
  spawner: AsyncSpawner = DEFAULT_SPAWNER,
  timeoutMs: number = 10_000
): Promise<ListAllSessionsResult> {
  if (listAllSessionsOverrideForTests) {
    return listAllSessionsOverrideForTests();
  }

  let bin: string;
  try {
    bin = resolveChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  const args = ['chat', '--list-sessions', '--format', 'json'];
  let result;
  try {
    result = await spawnWithDeadline(
      spawner,
      bin,
      args,
      timeoutMs,
      'kiro-cli chat --list-sessions'
    );
  } catch (e) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
    };
  }
  if (result.error) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${result.error.message}`,
    };
  }
  if (result.exitCode !== 0) {
    return {
      ok: false,
      error: `kiro-cli exited ${result.exitCode}: ${result.stderr.trim() || result.stdout.trim()}`,
    };
  }
  return parseListing(result.stdout);
}

/**
 * Execute `kiro-cli chat --list-sessions --all-cwds --format json` and parse
 * every envelope: local (V1 + V2) sessions across all workspaces, one
 * `{cwd, sessions}` group per directory. KAS rows are absent by contract —
 * the caller merges its own live KAS listing.
 *
 * The V2 walk reads thousands of metadata files, so the default timeout is
 * generous; callers treat this as an async enrichment, never a gate on
 * first paint.
 */
export async function listAllSessionsAllCwds(
  spawner: AsyncSpawner = DEFAULT_SPAWNER,
  timeoutMs: number = 30_000
): Promise<ListAllCwdsResult> {
  let bin: string;
  try {
    bin = resolveChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  const args = ['chat', '--list-sessions', '--all-cwds', '--format', 'json'];
  let result;
  try {
    result = await spawnWithDeadline(
      spawner,
      bin,
      args,
      timeoutMs,
      'kiro-cli chat --list-sessions --all-cwds'
    );
  } catch (e) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
    };
  }
  if (result.error) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${result.error.message}`,
    };
  }
  if (result.exitCode !== 0) {
    return {
      ok: false,
      error: `kiro-cli exited ${result.exitCode}: ${result.stderr.trim() || result.stdout.trim()}`,
    };
  }
  return parseAllCwdsListing(result.stdout);
}

/** Parse the multi-envelope stdout of `--all-cwds`. */
function parseAllCwdsListing(stdout: string): ListAllCwdsResult {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    return { ok: false, error: 'No output from kiro-cli --list-sessions' };
  }
  const last = lines[lines.length - 1]!;
  let parsed: unknown;
  try {
    parsed = JSON.parse(last);
  } catch {
    return {
      ok: false,
      error: `Unexpected output from kiro-cli --list-sessions: ${last}`,
    };
  }
  if (!Array.isArray(parsed)) {
    return {
      ok: false,
      error: `kiro-cli --list-sessions returned an unexpected shape: ${last}`,
    };
  }
  const envelopes: { cwd: string; sessions: SessionEntry[] }[] = [];
  let complete = true;
  for (const raw of parsed) {
    const env = raw as Record<string, unknown>;
    if (typeof env.cwd !== 'string' || !Array.isArray(env.sessions)) {
      return {
        ok: false,
        error: `kiro-cli --list-sessions envelope missing cwd or sessions: ${last}`,
      };
    }
    const sessions: SessionEntry[] = [];
    if (env.complete === false) complete = false;
    for (const rawEntry of env.sessions) {
      const entry = rawEntry as Record<string, unknown>;
      if (
        typeof entry.sessionId !== 'string' ||
        typeof entry.source !== 'string' ||
        typeof entry.title !== 'string' ||
        typeof entry.updatedAt !== 'string'
      ) {
        complete = false;
        continue;
      }
      sessions.push({
        sessionId: entry.sessionId,
        source: entry.source as SessionSource,
        title: entry.title,
        updatedAt: entry.updatedAt,
        ...(typeof entry.messageCount === 'number'
          ? { messageCount: entry.messageCount }
          : {}),
        ...(typeof entry.executionTarget === 'string'
          ? { executionTarget: entry.executionTarget }
          : {}),
        ...(typeof entry.status === 'string' ? { status: entry.status } : {}),
      });
    }
    envelopes.push({ cwd: env.cwd, sessions });
  }
  return { ok: true, envelopes, complete };
}

/**
 * Delete a classic (V1) session through the binary — its SQLite store is
 * unreadable from TypeScript, so the CLI's own delete flag is the only
 * correct writer.
 */
export async function deleteClassicSession(
  sessionId: string,
  spawner: AsyncSpawner = DEFAULT_SPAWNER,
  timeoutMs: number = 10_000
): Promise<{ ok: boolean; error?: string }> {
  let bin: string;
  try {
    bin = resolveChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  const args = [
    'chat',
    '--delete-session',
    sessionId,
    '--session-source',
    'v1',
  ];
  try {
    const result = await spawnWithDeadline(
      spawner,
      bin,
      args,
      timeoutMs,
      'delete-session'
    );
    if (result.error) return { ok: false, error: result.error.message };
    if (result.exitCode !== 0) {
      return {
        ok: false,
        error: result.stderr.trim() || `exit ${result.exitCode}`,
      };
    }
    return { ok: true };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/**
 * Parse the binary's stdout. The contract is one JSON value (an
 * array of envelopes) on the LAST non-empty stdout line; earlier
 * lines may be diagnostic output the binary writes to stderr in
 * normal cases but stdout in some edge cases (e.g. log forwarding).
 */
function parseListing(stdout: string): ListAllSessionsResult {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    return { ok: false, error: 'No output from kiro-cli --list-sessions' };
  }
  const last = lines[lines.length - 1]!;
  let parsed: unknown;
  try {
    parsed = JSON.parse(last);
  } catch {
    return {
      ok: false,
      error: `Unexpected output from kiro-cli --list-sessions: ${last}`,
    };
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    return {
      ok: false,
      error: `kiro-cli --list-sessions returned an unexpected shape: ${last}`,
    };
  }
  const envelope = parsed[0] as Record<string, unknown>;
  const cwd = typeof envelope.cwd === 'string' ? envelope.cwd : null;
  const sessionsRaw = Array.isArray(envelope.sessions)
    ? envelope.sessions
    : null;
  if (cwd === null || sessionsRaw === null) {
    return {
      ok: false,
      error: `kiro-cli --list-sessions envelope missing cwd or sessions: ${last}`,
    };
  }
  const sessions: SessionEntry[] = [];
  for (const raw of sessionsRaw) {
    const entry = raw as Record<string, unknown>;
    if (
      typeof entry.sessionId !== 'string' ||
      typeof entry.source !== 'string' ||
      typeof entry.title !== 'string' ||
      typeof entry.updatedAt !== 'string'
    ) {
      return {
        ok: false,
        error: `kiro-cli --list-sessions entry missing required field: ${JSON.stringify(entry)}`,
      };
    }
    sessions.push({
      sessionId: entry.sessionId,
      source: entry.source as SessionSource,
      title: entry.title,
      updatedAt: entry.updatedAt,
      ...(typeof entry.messageCount === 'number'
        ? { messageCount: entry.messageCount }
        : {}),
      ...(typeof entry.executionTarget === 'string'
        ? { executionTarget: entry.executionTarget }
        : {}),
      ...(typeof entry.status === 'string' ? { status: entry.status } : {}),
    });
  }
  return { ok: true, cwd, sessions };
}
