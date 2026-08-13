/**
 * Locating the `chat_cli` binary for production code paths.
 *
 * Production paths (auth callback, chat-internal subcommand runners,
 * session listing) use [`resolveChatCliBinFromEnv`]: the launcher always
 * sets `KIRO_CHAT_CLI_BIN`, so an unset variable indicates deployment
 * misconfiguration and throws rather than guessing a path.
 *
 * The env var is resolved once at session start from the launcher's own
 * executable path, which under version-managed installs points into a
 * versioned directory that an auto-update can delete mid-session. A
 * long-lived session would then fail every subsequent spawn (most
 * critically the auth token refresh) with ENOENT until restarted. When
 * the env path no longer exists on disk, resolution falls back through
 * absolute candidates that survive updates in place:
 *
 *   1. The same program the env named, under `~/.local/bin` (the Linux
 *      install layout places the chat binary there).
 *   2. The `kiro-cli` launcher in `~/.local/bin`, `/usr/local/bin`,
 *      then `/opt/homebrew/bin`. The launcher locates the installed
 *      chat binary and forwards `chat` argv to it verbatim, so it
 *      answers the same subcommand surface at the cost of launcher
 *      startup - acceptable on this recovery-only path.
 *
 * Candidates are always absolute: this module's result can end up
 * serving auth token requests, and a bare command name would let the
 * OS spawn search (which may consider the current directory on
 * Windows) select an untrusted executable. If no candidate exists,
 * the env path is returned unchanged so the spawn failure names the
 * path the launcher originally supplied.
 *
 * Test/dev binary resolution (workspace build with fallbacks) lives in
 * `src/test-utils/chat-cli-bin.ts` and must not be imported by production
 * code.
 */

import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, join } from 'node:path';

export const NOT_FOUND_MESSAGE = 'Failed to find the kiro-cli binary';

/**
 * Filesystem probes, injectable for tests. Production callers omit this
 * and get real `fs` / `os` behavior.
 */
export interface ResolveProbes {
  exists: (path: string) => boolean;
  homeDir: () => string;
}

const defaultProbes: ResolveProbes = {
  exists: existsSync,
  // Prefer HOME / USERPROFILE over os.homedir(): Bun's homedir() can ignore
  // an overridden HOME (passwd-based), which would break redirection by
  // tests and sandboxed environments. Matches the TUI-wide convention.
  homeDir: () => process.env.HOME || process.env.USERPROFILE || homedir(),
};

const LAUNCHER_NAME = 'kiro-cli';

/**
 * Resolve the `chat_cli` binary path for spawning.
 *
 * Reads `KIRO_CHAT_CLI_BIN` and throws the canonical not-found message
 * if it's unset. Returns the env path when it exists on disk, otherwise
 * the first existing absolute fallback candidate (same-named binary or
 * the launcher under stable install locations), otherwise the env path
 * unchanged (letting the spawn report the missing binary).
 */
export function resolveChatCliBinFromEnv(
  probes: ResolveProbes = defaultProbes
): string {
  const env = process.env.KIRO_CHAT_CLI_BIN;
  if (!env) throw new Error(NOT_FOUND_MESSAGE);
  if (probes.exists(env)) return env;
  const localBin = join(probes.homeDir(), '.local', 'bin');
  const candidates = [
    join(localBin, basename(env)),
    join(localBin, LAUNCHER_NAME),
    join('/usr/local/bin', LAUNCHER_NAME),
    join('/opt/homebrew/bin', LAUNCHER_NAME),
  ];
  for (const candidate of candidates) {
    if (probes.exists(candidate)) return candidate;
  }
  return env;
}
