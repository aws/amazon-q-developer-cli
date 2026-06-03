/**
 * ACP capability for `_kiro/auth/getAccessToken`.
 *
 * KAS launches with `--auth=acp-callback` and calls back to this host
 * whenever it needs a fresh OIDC access token (first acquisition,
 * pre-expiry refresh, or hard-expired refresh). The host owns the
 * refresh token; KAS only ever sees access tokens.
 *
 * KAS sends an empty payload (`{}`); the host's behavior is identical
 * regardless of why the call was made, so KAS forwards no hints. The
 * handler shells out to the hidden `kiro-cli chat _ get-kas-token`
 * subcommand which:
 *   - acquires the cross-process refresh lock,
 *   - resolves the highest-priority cached token (External -> Builder -> Social),
 *   - refreshes it via OIDC if it has crossed expiry,
 *   - prints `{success, accessToken, expiresAt, profileArn?}`.
 *
 * The handler strips the host-internal `success` field and returns
 * `{accessToken, expiresAt, profileArn?}` to KAS. On failure (not logged
 * in, refresh fault) it throws; the KAS-side `AcpCallbackAuthProvider`
 * translates that into a `TokenExpiredError` surfaced to the caller.
 *
 * # Binary resolution
 *
 * The path to `chat_cli` is read from `KIRO_CHAT_CLI_BIN`. The Rust
 * launcher injects this when spawning the TUI (`launch.rs`). Without it
 * the handler throws a structured error rather than silently failing.
 *
 * # Spawner injection
 *
 * The factory accepts an optional `AsyncSpawner`. The default uses
 * `Bun.spawn` (true async, doesn't block the event loop while the OIDC
 * refresh round-trips). Tests inject a fake to assert argv and synthesize
 * stdout / exit codes.
 */

import type {
  ClientCapability,
  GetAccessTokenResponse,
} from '@kiro/acp-type-covenant';

import { requireChatCliBinFromEnv } from '../utils/chat-cli-bin';
/**
 * Method name for the agent->client extension. Matches the typed entry
 * in `@kiro/acp-type-covenant/client-capabilities/index.ts`. Centralised
 * here so the registration site and the type covenant can't drift.
 */
export const GET_ACCESS_TOKEN_METHOD = '_kiro/auth/getAccessToken';

/**
 * Wire request from KAS. Empty.
 */
export type GetAccessTokenRequest = Record<string, never>;

/**
 * The shape this module needs from its spawn function. Compatible with
 * both `Bun.spawn` (after a small adapter) and `node:child_process.spawn`
 * + a stdout-collecting wrapper. Tests inject a fake to assert argv and
 * synthesize JSON output.
 */
export type AsyncSpawner = (
  cmd: string,
  args: string[]
) => Promise<{
  status: number | null;
  stdout: string | Buffer;
  stderr: string | Buffer;
  /** Set when the spawn itself fails (e.g. ENOENT). Mirrors child_process behavior. */
  error?: Error;
}>;

const DEFAULT_SPAWNER: AsyncSpawner = async (cmd, args) => {
  // Bun.spawn rather than child_process so tests that globally mock
  // child_process via mock.module don't accidentally hijack this path.
  // Mirrors the rationale documented in `utils/clipboard-image.ts`.
  const proc = Bun.spawn([cmd, ...args], {
    stdin: 'ignore',
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return {
    status: exitCode,
    stdout,
    stderr,
  };
};

/**
 * Extract the last non-empty stdout line and parse as JSON. The Rust
 * subcommand prints exactly one JSON line as its contract, but may emit
 * tracing log lines first when `KIRO_LOG_LEVEL` is active.
 */
function parseLastJsonLine(stdout: string): Record<string, unknown> {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0) {
    throw new Error('No output from kiro-cli');
  }
  const last = lines[lines.length - 1]!;
  let parsed: unknown;
  try {
    parsed = JSON.parse(last);
  } catch {
    throw new Error(`Unexpected output from kiro-cli: ${last}`);
  }
  if (typeof parsed !== 'object' || parsed === null) {
    throw new Error(`Malformed kiro-cli response: ${last}`);
  }
  return parsed as Record<string, unknown>;
}

async function runGetKasToken(
  _request: GetAccessTokenRequest,
  spawner: AsyncSpawner
): Promise<GetAccessTokenResponse> {
  const bin = requireChatCliBinFromEnv();

  // KAS sends `{}` for `_kiro/auth/getAccessToken`; the wire contract has
  // no hints to forward to the host. The argv is fixed.
  const args: string[] = ['chat', '_', 'get-kas-token'];

  let result: Awaited<ReturnType<AsyncSpawner>>;
  try {
    result = await spawner(bin, args);
  } catch (e) {
    throw new Error(
      `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
      { cause: e }
    );
  }
  if (result.error) {
    throw new Error(`Failed to spawn kiro-cli: ${result.error.message}`, {
      cause: result.error,
    });
  }

  const stdout =
    typeof result.stdout === 'string'
      ? result.stdout
      : result.stdout.toString('utf-8');
  const parsed = parseLastJsonLine(stdout);

  // Error envelope from chat-cli: surface its message directly so the
  // user sees the "not logged in" string from the Rust side rather than
  // a generic ACP error.
  if (parsed.success === false) {
    const message =
      typeof parsed.error === 'string'
        ? parsed.error
        : 'Unknown error from kiro-cli';
    throw new Error(message);
  }

  if (
    typeof parsed.accessToken !== 'string' ||
    parsed.accessToken.length === 0
  ) {
    throw new Error(
      `kiro-cli response missing accessToken: ${JSON.stringify(parsed)}`
    );
  }
  if (typeof parsed.expiresAt !== 'string' || parsed.expiresAt.length === 0) {
    throw new Error(
      `kiro-cli response missing expiresAt: ${JSON.stringify(parsed)}`
    );
  }
  if (typeof parsed.profileArn !== 'string' || parsed.profileArn.length === 0) {
    throw new Error(
      `kiro-cli response missing profileArn: ${JSON.stringify(parsed)}`
    );
  }

  // Project to the wire shape. Strips the host-internal `success` field;
  // KAS accepts extra fields but the minimal shape keeps the contract clean.
  return {
    accessToken: parsed.accessToken,
    expiresAt: parsed.expiresAt,
    profileArn: parsed.profileArn,
  };
}

/**
 * Build the `_kiro/auth/getAccessToken` capability for registration with
 * `KiroClient`. Pass to `KiroClient`'s `capabilities` array. The factory
 * accepts an optional spawner (used in tests) and otherwise wires the
 * production `Bun.spawn` path.
 */
export function createGetAccessTokenCapability(
  spawner: AsyncSpawner = DEFAULT_SPAWNER
): ClientCapability<
  typeof GET_ACCESS_TOKEN_METHOD,
  GetAccessTokenRequest,
  GetAccessTokenResponse
> {
  return {
    type: 'other',
    key: 'authCallback',
    value: true,
    method: GET_ACCESS_TOKEN_METHOD,
    handler: (request: GetAccessTokenRequest) =>
      runGetKasToken(request, spawner),
  };
}
