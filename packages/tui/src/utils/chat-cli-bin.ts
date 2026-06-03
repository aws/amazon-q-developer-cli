/**
 * Single source of truth for locating the `chat_cli` binary.
 *
 * Production paths (auth callback, chat-internal subcommand
 * runners, session listing) use [`requireChatCliBinFromEnv`]: the
 * launcher always sets `KIRO_CHAT_CLI_BIN`, so falling back to a
 * repo-relative path would mask deployment misconfiguration.
 *
 * Tests, knight-rider, and dev scripts use [`resolveChatCliBin`] /
 * [`requireChatCliBin`] which add fallbacks to `CARGO_TARGET_DIR`
 * and the repo's `target/debug/chat_cli` so devs can run them
 * without exporting the env var.
 *
 * Every "binary not found" failure surfaces the same message:
 * `Failed to find the kiro-cli binary`.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const BINARY_NAME = process.platform === 'win32' ? 'chat_cli.exe' : 'chat_cli';
const NOT_FOUND_MESSAGE = 'Failed to find the kiro-cli binary';

/**
 * Path to the kiro-cli repo root. Computed once relative to this
 * file (`packages/tui/src/utils/`) so callers don't have to count
 * `../` segments.
 */
export const REPO_ROOT: string = (() => {
  let here: string;
  try {
    here = path.dirname(fileURLToPath(import.meta.url));
  } catch {
    here = __dirname;
  }
  return path.resolve(here, '..', '..', '..', '..');
})();

/**
 * Production contract: read `KIRO_CHAT_CLI_BIN` and throw with the
 * canonical not-found message if it's unset. No fallback.
 */
export function requireChatCliBinFromEnv(): string {
  const env = process.env.KIRO_CHAT_CLI_BIN;
  if (env && env.length > 0) return env;
  throw new Error(NOT_FOUND_MESSAGE);
}

/**
 * Resolve the path that the test/script *would* use for the debug
 * `chat_cli` binary. Honors `KIRO_CHAT_CLI_BIN`, then
 * `CARGO_TARGET_DIR`, then the repo's `target/debug/chat_cli`.
 * Does not check whether the file exists.
 */
export function resolveChatCliBin(): string {
  const explicit = process.env.KIRO_CHAT_CLI_BIN;
  if (explicit && explicit.length > 0) return explicit;
  const cargoTargetDir = process.env.CARGO_TARGET_DIR;
  if (cargoTargetDir && cargoTargetDir.length > 0) {
    return path.join(cargoTargetDir, 'debug', BINARY_NAME);
  }
  return path.join(REPO_ROOT, 'target', 'debug', BINARY_NAME);
}

/**
 * Resolve the path with dev fallback and assert it exists. Throws
 * the canonical not-found message when no resolved path lands on
 * disk - the most common cause of mysterious e2e failures.
 */
export function requireChatCliBin(): string {
  const bin = resolveChatCliBin();
  if (!fs.existsSync(bin)) {
    throw new Error(NOT_FOUND_MESSAGE);
  }
  return bin;
}
