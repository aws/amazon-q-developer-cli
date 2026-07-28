/**
 * Wrapper for `kiro-cli chat _ refresh-feed`.
 *
 * Asks the Rust side to do a bounded blocking fetch of the remote
 * changelog feed and snapshot it to the feed file this process reads
 * (KIRO_FEED_FILE). Resolves `true` when fresh content was written and
 * the caller should re-read the feed; `false` when the fetch failed,
 * the channel does not fetch remotely, or the spawn itself failed —
 * the existing snapshot remains valid in all of those cases.
 */

import type { AsyncSpawner } from './chat-internal-cli.js';
import { runChatInternalAsync } from './chat-internal-cli.js';

/** Spawn budget: the Rust-side fetch is itself bounded at 3s. */
const REFRESH_TIMEOUT_MS = 10_000;

/**
 * Pass `signal` to cancel a refresh whose result is no longer wanted (e.g.
 * the changelog panel was closed): aborting kills the spawned child rather
 * than letting it run to completion.
 */
export async function refreshChangelogFeed(
  spawner?: AsyncSpawner,
  signal?: AbortSignal
): Promise<boolean> {
  const r = await runChatInternalAsync(
    ['chat', '_', 'refresh-feed'],
    spawner,
    REFRESH_TIMEOUT_MS,
    signal
  );
  return r.ok && r.output.kind === 'refreshFeed' && r.output.data.updated;
}
