/**
 * Locating the `chat_cli` binary for tests, knight-rider, and dev scripts.
 *
 * CI prioritizes `KIRO_CHAT_CLI_BIN` so certification uses its downloaded
 * artifact. Local runs prefer workspace builds to avoid inheriting the
 * installed binary from a parent Kiro session.
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
 * Ordered candidate paths for the `chat_cli` binary.
 */
function candidateBins(): string[] {
  const candidates: string[] = [];
  const env = process.env.KIRO_CHAT_CLI_BIN;
  const isCI = process.env.CI !== undefined && process.env.CI !== 'false';
  if (isCI && env) {
    candidates.push(env);
  }

  const cargoTargetDir = process.env.CARGO_TARGET_DIR;
  if (cargoTargetDir) {
    candidates.push(path.join(cargoTargetDir, 'debug', BINARY_NAME));
  }
  candidates.push(path.join(REPO_ROOT, 'target', 'debug', BINARY_NAME));
  if (!isCI && env) {
    candidates.push(env);
  }
  return candidates;
}

/**
 * Resolve the first available test binary.
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
