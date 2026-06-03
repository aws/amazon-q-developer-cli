/**
 * Shared spawn-and-parse contract for the hidden `kiro-cli chat _ ...`
 * subcommand surface.
 *
 * Each subcommand wrapper (`session-archive-cli`, `ensure-session-cli`,
 * etc.) sits on top of this helper and only contributes the argv shape
 * specific to its subcommand. The output contract, binary resolution,
 * spawner injection, and last-line JSON extraction live here.
 *
 * # Output contract
 *
 * The binary prints exactly one JSON line on stdout, exit code mirrors
 * `success`. Both shapes go to stdout (never stderr) so callers always
 * parse with `JSON.parse(lastStdoutLine)` without disambiguating streams.
 *
 *   Success (exit 0): `{"success": true, ...subcommand-specific fields}`
 *   Failure (exit 1): `{"success": false, "error": "<message>"}`
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

import { requireChatCliBinFromEnv } from './chat-cli-bin';

/** Spawn function shape compatible with `Bun.spawn`-style async APIs. */
export type AsyncSpawner = (
  cmd: string,
  args: string[]
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

/** Result of parsing the binary's stdout. The success branch carries
 * whatever subcommand-specific fields the binary returned, minus
 * `success`. */
export type RunResult<T extends object> =
  | ({ ok: true } & T)
  | { ok: false; error: string; code?: string };

const DEFAULT_ASYNC_SPAWNER: AsyncSpawner = async (cmd, args) => {
  const proc = Bun.spawn([cmd, ...args], {
    stdin: 'ignore',
    stdout: 'pipe',
    stderr: 'pipe',
  });
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

function parseLastJsonLine<T extends object>(
  stdout: string,
  pickSuccessFields: (parsed: Record<string, unknown>) => T | null
): RunResult<T> {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    return { ok: false, error: 'No output from kiro-cli' };
  }
  const last = lines[lines.length - 1]!;
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(last);
  } catch {
    return { ok: false, error: `Unexpected output from kiro-cli: ${last}` };
  }
  if (parsed.success === true) {
    const picked = pickSuccessFields(parsed);
    if (picked === null) {
      return {
        ok: false,
        error: `Malformed kiro-cli response: ${JSON.stringify(parsed)}`,
      };
    }
    return { ok: true, ...picked };
  }
  if (typeof parsed.error === 'string') {
    const code = typeof parsed.code === 'string' ? parsed.code : undefined;
    return { ok: false, error: parsed.error, ...(code ? { code } : {}) };
  }
  return {
    ok: false,
    error: `Malformed kiro-cli response: ${JSON.stringify(parsed)}`,
  };
}

/**
 * Run a `chat _ <subcommand>` invocation asynchronously. `pickFields`
 * extracts the subcommand-specific success-shape fields from the
 * parsed JSON; returning `null` from `pickFields` produces a
 * "Malformed kiro-cli response" error.
 *
 * `timeoutMs` (default 30s) bounds the spawn. A timeout returns a
 * structured error rather than hanging the caller indefinitely.
 */
export async function runChatInternalAsync<T extends object>(
  args: string[],
  pickFields: (parsed: Record<string, unknown>) => T | null,
  spawner: AsyncSpawner = DEFAULT_ASYNC_SPAWNER,
  timeoutMs: number = 30_000
): Promise<RunResult<T>> {
  let bin: string;
  try {
    bin = requireChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  let result;
  try {
    result = await Promise.race([
      spawner(bin, args),
      new Promise<never>((_, reject) =>
        setTimeout(
          () =>
            reject(
              new Error(
                `kiro-cli ${args.slice(0, 3).join(' ')} did not complete within ${timeoutMs}ms`
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
  return parseLastJsonLine(result.stdout, pickFields);
}

/** Synchronous variant of {@link runChatInternalAsync}. */
export function runChatInternalSync<T extends object>(
  args: string[],
  pickFields: (parsed: Record<string, unknown>) => T | null,
  spawner: SyncSpawner = DEFAULT_SYNC_SPAWNER
): RunResult<T> {
  let bin: string;
  try {
    bin = requireChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  let result;
  try {
    result = spawner(bin, args);
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
  const stdout =
    typeof result.stdout === 'string'
      ? result.stdout
      : result.stdout.toString('utf-8');
  return parseLastJsonLine(stdout, pickFields);
}
