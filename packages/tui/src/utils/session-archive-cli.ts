/**
 * Thin wrapper around the hidden `kiro-cli chat _ export-session` and
 * `kiro-cli chat _ import-session` subcommands.
 *
 * These subcommands are the canonical surface for KAS session archive
 * operations - the algorithm lives in `crates/chat-cli-v2/src/agent/kas/`.
 * The TUI's `/chat save` and `/chat load` slash command handlers route
 * through here. The hidden `chat _` namespace is intentionally not in
 * `--help` so end users can't invoke it directly.
 *
 * # Output contract
 *
 * The binary prints exactly one JSON line on stdout, exit code mirrors
 * `success`. Both shapes go to stdout (never stderr) so callers always
 * parse with `JSON.parse(lastStdoutLine)` without disambiguating streams.
 *
 *   Success (exit 0): `{"success":true,"path":"<absolute path>"}`
 *   Failure (exit 1): `{"success":false,"error":"<message>"}`
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
 * Both functions accept an optional `SyncSpawner` as the second
 * argument. The default uses `Bun.spawnSync` (deliberately not
 * `node:child_process.spawnSync` - same rationale as
 * `utils/clipboard-image.ts`: keeps the helper unaffected by tests
 * elsewhere that globally mock `child_process` via `mock.module`).
 */

export interface ExportSessionInput {
  /** KAS session id to export. Required. */
  sessionId: string;
  /** Workspace path. Required - selects which workspace-hashed bucket to read from. */
  cwd: string;
  /** Output zip path. Required. */
  out: string;
  /** Sessions root override. Defaults to `$KIRO_HOME/.kiro/sessions`. */
  basePath?: string;
  /** Overwrite existing output file. */
  force?: boolean;
}

export interface ImportSessionInput {
  /** Absolute or expandable archive path to import. Required. */
  archivePath: string;
  /** Workspace path. Required - selects which workspace-hashed bucket to write to. */
  cwd: string;
  /** Sessions root override. Defaults to `$KIRO_HOME/.kiro/sessions`. */
  basePath?: string;
}

export type ArchiveResult =
  | { ok: true; path: string }
  | { ok: false; error: string };

/**
 * The shape this module needs from its spawn function. Compatible with
 * both `Bun.spawnSync` (after a small adapter) and
 * `node:child_process.spawnSync`. Tests inject a fake to assert argv
 * and synthesize JSON output.
 */
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

const DEFAULT_SPAWNER: SyncSpawner = (cmd, args) => {
  const r = Bun.spawnSync([cmd, ...args], {
    stdin: 'ignore',
    stdout: 'pipe',
    stderr: 'pipe',
  });
  return {
    status: r.exitCode,
    stdout: Buffer.from(r.stdout),
    stderr: Buffer.from(r.stderr),
    // Bun.spawnSync does not surface a top-level error field on ENOENT,
    // it returns an exit code of 1 and a stderr message. We map that to
    // the `error` field here only for parity with child_process.spawnSync
    // when the upstream contract changes; for now we just leave error
    // unset and let parseResult fall through to a generic message.
  };
};

function resolveBinary(): string | null {
  const fromEnv = process.env.KIRO_CHAT_CLI_BIN;
  return fromEnv && fromEnv.length > 0 ? fromEnv : null;
}

function parseLastJsonLine(stdout: string): ArchiveResult {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    return { ok: false, error: 'No output from kiro-cli' };
  }
  const last = lines[lines.length - 1]!;
  let parsed: { success?: unknown; path?: unknown; error?: unknown };
  try {
    parsed = JSON.parse(last);
  } catch {
    return { ok: false, error: `Unexpected output from kiro-cli: ${last}` };
  }
  if (parsed.success === true && typeof parsed.path === 'string') {
    return { ok: true, path: parsed.path };
  }
  if (typeof parsed.error === 'string') {
    return { ok: false, error: parsed.error };
  }
  return {
    ok: false,
    error: `Malformed kiro-cli response: ${JSON.stringify(parsed)}`,
  };
}

function runArchiveCommand(
  args: string[],
  spawner: SyncSpawner
): ArchiveResult {
  const bin = resolveBinary();
  if (!bin) {
    return {
      ok: false,
      error:
        'KIRO_CHAT_CLI_BIN is not set; cannot locate kiro-cli for session archive',
    };
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
  return parseLastJsonLine(stdout);
}

export function exportSession(
  input: ExportSessionInput,
  spawner: SyncSpawner = DEFAULT_SPAWNER
): ArchiveResult {
  const args = [
    'chat',
    '_',
    'export-session',
    '--id',
    input.sessionId,
    '--cwd',
    input.cwd,
    '--out',
    input.out,
  ];
  if (input.basePath) args.push('--base-path', input.basePath);
  if (input.force) args.push('--force');
  return runArchiveCommand(args, spawner);
}

export function importSession(
  input: ImportSessionInput,
  spawner: SyncSpawner = DEFAULT_SPAWNER
): ArchiveResult {
  const args = [
    'chat',
    '_',
    'import-session',
    '--archive',
    input.archivePath,
    '--cwd',
    input.cwd,
  ];
  if (input.basePath) args.push('--base-path', input.basePath);
  return runArchiveCommand(args, spawner);
}
