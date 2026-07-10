/**
 * Locating the `chat_cli` binary for tests, knight-rider, and dev scripts.
 *
 * Prefers the workspace build (`CARGO_TARGET_DIR/debug/chat_cli`, then the
 * repo's `target/debug/chat_cli`) so tests exercise locally-built changes.
 * `KIRO_CHAT_CLI_BIN` is honored only as a last resort, when no workspace
 * build is present: the launcher always sets that variable while Kiro is
 * running, so prioritizing it would silently hijack tests spawned from
 * within a Kiro session into exercising the installed binary instead of the
 * workspace build.
 *
 * Not for production use - production code reads `KIRO_CHAT_CLI_BIN`
 * directly via `requireChatCliBinFromEnv` in `src/utils/chat-cli-bin.ts`.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

import { NOT_FOUND_MESSAGE } from '../utils/chat-cli-bin';

const BINARY_NAME = process.platform === 'win32' ? 'chat_cli.exe' : 'chat_cli';

/**
 * Path to the kiro-cli repo root. Computed once relative to this file
 * (`packages/tui/src/test-utils/`) so callers don't have to count `../`
 * segments.
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
 * Ordered candidate paths for the debug `chat_cli` binary: the workspace
 * build first, then `KIRO_CHAT_CLI_BIN` as a last resort.
 */
function candidateBins(): string[] {
  const candidates: string[] = [];
  const cargoTargetDir = process.env.CARGO_TARGET_DIR;
  if (cargoTargetDir && cargoTargetDir.length > 0) {
    candidates.push(path.join(cargoTargetDir, 'debug', BINARY_NAME));
  }
  candidates.push(path.join(REPO_ROOT, 'target', 'debug', BINARY_NAME));
  const env = process.env.KIRO_CHAT_CLI_BIN;
  if (env && env.length > 0) {
    candidates.push(env);
  }
  return candidates;
}

/**
 * Resolve the debug `chat_cli` binary path, preferring the workspace build.
 * Returns the first candidate that exists on disk; if none exist, returns
 * the repo `target/debug` path so callers surface a clear not-found error.
 */
export function resolveChatCliBin(): string {
  const repoTarget = path.join(REPO_ROOT, 'target', 'debug', BINARY_NAME);
  return candidateBins().find((p) => fs.existsSync(p)) ?? repoTarget;
}

/**
 * Resolve the binary path and assert it exists, throwing the canonical
 * not-found message otherwise - the most common cause of mysterious e2e
 * failures.
 */
export function requireChatCliBin(): string {
  const bin = resolveChatCliBin();
  if (!fs.existsSync(bin)) {
    throw new Error(NOT_FOUND_MESSAGE);
  }
  return bin;
}
