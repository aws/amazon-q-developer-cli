/**
 * Locating the `chat_cli` binary for production code paths.
 *
 * Production paths (auth callback, chat-internal subcommand runners,
 * session listing) use [`requireChatCliBinFromEnv`]: the launcher always
 * sets `KIRO_CHAT_CLI_BIN`, so falling back to a repo-relative path would
 * mask deployment misconfiguration.
 *
 * Test/dev binary resolution (workspace build with fallbacks) lives in
 * `src/test-utils/chat-cli-bin.ts` and must not be imported by production
 * code.
 */

export const NOT_FOUND_MESSAGE = 'Failed to find the kiro-cli binary';

/**
 * Production contract: read `KIRO_CHAT_CLI_BIN` and throw with the
 * canonical not-found message if it's unset. No fallback.
 */
export function requireChatCliBinFromEnv(): string {
  const env = process.env.KIRO_CHAT_CLI_BIN;
  if (env && env.length > 0) return env;
  throw new Error(NOT_FOUND_MESSAGE);
}
