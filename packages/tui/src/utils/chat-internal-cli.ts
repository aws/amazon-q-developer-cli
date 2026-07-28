/**
 * Shared spawn-and-parse contract for the hidden `kiro-cli chat _ ...`
 * subcommand surface.
 *
 * Each subcommand wrapper (`session-archive-cli`, `ensure-session-cli`,
 * etc.) sits on top of this helper and only contributes the argv shape
 * specific to its subcommand. Binary resolution, spawner injection,
 * exit-code verification, and last-line JSON parsing live here.
 *
 * # Output contract
 *
 * The binary prints exactly one JSON line on stdout. Wire shape is
 * the typeshare-shared {@link CliInternalOutput} discriminated union:
 *
 *   `{"kind": "<subcommand>", "data": {...}}`  - subcommand success
 *   `{"kind": "error",        "data": {message, code?}}`  - failure
 *
 * Exit code mirrors the variant: `error` -> 1, anything else -> 0.
 *
 * The binary may emit Rust log lines on stdout before the JSON line
 * when `KIRO_LOG_LEVEL` etc. are active; this module extracts the LAST
 * non-empty line as the contract payload.
 *
 * # Binary resolution
 *
 * The path to `chat_cli` is read from `KIRO_CHAT_CLI_BIN`. The Rust
 * launcher injects this when spawning the TUI; tests set it explicitly
 * (typically to `target/debug/chat_cli` for the ACP integ harness).
 * If unset, the helper returns a structured error rather than throwing
 * so handlers can surface it as an alert.
 *
 * # Spawner injection
 *
 * Defaults use `Bun.spawn` / `Bun.spawnSync` (deliberately not
 * `node:child_process` - same rationale as `utils/clipboard-image.ts`:
 * keeps the helper unaffected by tests elsewhere that globally mock
 * `child_process` via `mock.module`). Tests inject a fake spawner to
 * assert argv and synthesize JSON output.
 */

import type { CliInternalOutput } from '../types/generated/chat-internal';
import { requireChatCliBinFromEnv } from './chat-cli-bin';
import { logger } from './logger';

/** Spawn function shape compatible with `Bun.spawn`-style async APIs. */
export type AsyncSpawner = (
  cmd: string,
  args: string[],
  options?: { signal?: AbortSignal }
) => Promise<{
  status: number | null;
  stdout: string;
  stderr: string;
  /** Set when the spawn itself fails (e.g. ENOENT). */
  error?: Error;
}>;

/** Spawn function shape compatible with `Bun.spawnSync` / `node:child_process.spawnSync`. */
export type SyncSpawner = (
  cmd: string,
  args: string[]
) => {
  status: number | null;
  stdout: string | Buffer;
  stderr: string | Buffer;
  /** Set when the spawn itself fails (e.g. ENOENT). Mirrors child_process.spawnSync. */
  error?: Error;
};

/**
 * Result of running a `chat _` subcommand. The success branch carries
 * the typed {@link CliInternalOutput} discriminated union; callers
 * narrow on `output.kind` to handle subcommand-specific data and the
 * shared `error` variant. The failure branch covers host-side issues
 * (binary not found, spawn failure, parse failure, timeout).
 */
export type RunResult =
  | { ok: true; output: CliInternalOutput }
  | { ok: false; message: string };

const DEFAULT_ASYNC_SPAWNER: AsyncSpawner = async (cmd, args, options) => {
  const proc = Bun.spawn([cmd, ...args], {
    stdin: 'ignore',
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const onAbort = () => {
    try {
      proc.kill();
    } catch {
      // already exited
    }
  };
  options?.signal?.addEventListener('abort', onAbort);
  try {
    const [stdout, stderr] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    await proc.exited;
    return {
      status: proc.exitCode,
      stdout,
      stderr,
    };
  } finally {
    options?.signal?.removeEventListener('abort', onAbort);
  }
};

const DEFAULT_SYNC_SPAWNER: SyncSpawner = (cmd, args) => {
  const r = Bun.spawnSync([cmd, ...args], {
    stdin: 'ignore',
    stdout: 'pipe',
    stderr: 'pipe',
  });
  return {
    status: r.exitCode,
    stdout: Buffer.from(r.stdout),
    stderr: Buffer.from(r.stderr),
  };
};

function tail(s: string, n: number): string {
  const lines = s.split('\n').filter((l) => l.length > 0);
  return lines.slice(-n).join('\n');
}

/**
 * Extract the last non-empty stdout line and parse as `CliInternalOutput`.
 * The Rust subcommand prints exactly one JSON line, but may emit
 * tracing log lines first when `KIRO_LOG_LEVEL` is active.
 */
function parseLastJsonLine(stdout: string): RunResult {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    return { ok: false, message: 'No output from kiro-cli' };
  }
  const last = lines[lines.length - 1]!;
  let parsed: unknown;
  try {
    parsed = JSON.parse(last);
  } catch {
    return { ok: false, message: `Unexpected output from kiro-cli: ${last}` };
  }
  if (
    typeof parsed !== 'object' ||
    parsed === null ||
    typeof (parsed as { kind: unknown }).kind !== 'string' ||
    typeof (parsed as { data: unknown }).data !== 'object' ||
    (parsed as { data: unknown }).data === null
  ) {
    return {
      ok: false,
      message: `Malformed kiro-cli response: ${last}`,
    };
  }
  return { ok: true, output: parsed as CliInternalOutput };
}

function verifyExitCode(
  argv: string[],
  status: number | null,
  output: CliInternalOutput,
  stderr: string
): void {
  const expected = output.kind === 'error' ? 1 : 0;
  if (status !== expected) {
    logger.warn('[chat _] exit-code mismatch', {
      argv: argv.slice(0, 3),
      status,
      kind: output.kind,
      stderr: tail(stderr, 4),
    });
  }
}

/**
 * Run a `chat _ <subcommand>` invocation asynchronously.
 *
 * `timeoutMs` (default 30s) bounds the spawn. A timeout returns a
 * structured error rather than hanging the caller indefinitely.
 */
export async function runChatInternalAsync(
  args: string[],
  spawner: AsyncSpawner = DEFAULT_ASYNC_SPAWNER,
  timeoutMs: number = 30_000,
  signal?: AbortSignal
): Promise<RunResult> {
  let bin: string;
  try {
    bin = requireChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
  // Already-aborted caller: don't spawn at all.
  if (signal?.aborted) {
    return { ok: false, message: 'kiro-cli invocation aborted before spawn' };
  }
  const start = performance.now();
  const controller = new AbortController();
  // Propagate an external abort (e.g. the caller unmounted) to the spawn so
  // the child process is killed rather than left running to completion.
  const onExternalAbort = () => controller.abort();
  signal?.addEventListener('abort', onExternalAbort, { once: true });
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  let result: Awaited<ReturnType<AsyncSpawner>>;
  try {
    result = await Promise.race([
      spawner(bin, args, { signal: controller.signal }),
      new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => {
          controller.abort();
          reject(
            new Error(
              `kiro-cli ${args.slice(0, 3).join(' ')} did not complete within ${timeoutMs}ms`
            )
          );
        }, timeoutMs);
      }),
    ]);
  } catch (e) {
    return {
      ok: false,
      message: `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
    };
  } finally {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
    signal?.removeEventListener('abort', onExternalAbort);
  }
  if (result.error) {
    return {
      ok: false,
      message: `Failed to spawn kiro-cli: ${result.error.message}`,
    };
  }
  const durationMs = Math.round(performance.now() - start);
  const parsed = parseLastJsonLine(result.stdout);
  if (parsed.ok) {
    verifyExitCode(args, result.status, parsed.output, result.stderr);
    logger.info('[chat _] success', {
      argv: args.slice(0, 3),
      durationMs,
      kind: parsed.output.kind,
    });
  } else {
    logger.error('[chat _] parse failed', {
      argv: args.slice(0, 3),
      status: result.status,
      durationMs,
      stderr: tail(result.stderr, 4),
    });
  }
  return parsed;
}

/** Synchronous variant of {@link runChatInternalAsync}. */
export function runChatInternalSync(
  args: string[],
  spawner: SyncSpawner = DEFAULT_SYNC_SPAWNER
): RunResult {
  let bin: string;
  try {
    bin = requireChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
  const start = performance.now();
  let result: ReturnType<SyncSpawner>;
  try {
    result = spawner(bin, args);
  } catch (e) {
    return {
      ok: false,
      message: `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
    };
  }
  if (result.error) {
    return {
      ok: false,
      message: `Failed to spawn kiro-cli: ${result.error.message}`,
    };
  }
  const durationMs = Math.round(performance.now() - start);
  const stdout =
    typeof result.stdout === 'string'
      ? result.stdout
      : result.stdout.toString('utf-8');
  const stderr =
    typeof result.stderr === 'string'
      ? result.stderr
      : result.stderr.toString('utf-8');
  const parsed = parseLastJsonLine(stdout);
  if (parsed.ok) {
    verifyExitCode(args, result.status, parsed.output, stderr);
    logger.info('[chat _] success', {
      argv: args.slice(0, 3),
      durationMs,
      kind: parsed.output.kind,
    });
  } else {
    logger.error('[chat _] parse failed', {
      argv: args.slice(0, 3),
      status: result.status,
      durationMs,
      stderr: tail(stderr, 4),
    });
  }
  return parsed;
}
