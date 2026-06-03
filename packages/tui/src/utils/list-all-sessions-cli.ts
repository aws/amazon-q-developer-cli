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

import { requireChatCliBinFromEnv } from './chat-cli-bin';

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
}

/** Result envelope: `ok: true` with entries, or `ok: false` with a message. */
export type ListAllSessionsResult =
  | { ok: true; cwd: string; sessions: SessionEntry[] }
  | { ok: false; error: string };

/**
 * Spawn signature carved out as a parameter so unit tests can drive
 * deterministic stdout / exit codes without invoking a real binary.
 */
export type AsyncSpawner = (
  bin: string,
  args: string[]
) => Promise<{
  exitCode: number | null;
  stdout: string;
  stderr: string;
  error?: Error;
}>;

const DEFAULT_SPAWNER: AsyncSpawner = (bin, args) =>
  new Promise((resolve) => {
    const child = spawn(bin, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d.toString()));
    child.stderr.on('data', (d) => (stderr += d.toString()));
    child.on('error', (error) =>
      resolve({ exitCode: null, stdout, stderr, error })
    );
    child.on('close', (code) => resolve({ exitCode: code, stdout, stderr }));
  });

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
  let bin: string;
  try {
    bin = requireChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  const args = ['chat', '--list-sessions', '--format', 'json'];
  let result;
  try {
    result = await Promise.race([
      spawner(bin, args),
      new Promise<never>((_, reject) =>
        setTimeout(
          () =>
            reject(
              new Error(
                `kiro-cli chat --list-sessions did not complete within ${timeoutMs}ms`
              )
            ),
          timeoutMs
        )
      ),
    ]);
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
    });
  }
  return { ok: true, cwd, sessions };
}
