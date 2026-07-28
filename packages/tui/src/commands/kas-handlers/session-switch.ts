/**
 * Shared KAS/V3 session-switch orchestration. Currently used only by /tangent;
 * /chat and /rewind still carry their own inline copies of this flow and are
 * intended to migrate onto this helper (its option surface is kept generic for
 * that reason). KAS replays a session's conversation as `session/update`
 * notifications during `session/load`; this helper buffers those events and,
 * on success, clears transient UI, optionally resets the message store,
 * replays the conversation (capped to recent turns), and updates
 * session/model/agent/tangent state.
 *
 * The UI is cleared only AFTER the load succeeds, so a failed load leaves the
 * current conversation intact instead of blanking the screen.
 */

import { extractRpcErrorMessage } from '../../utils/error-handling';
import { logger } from '../../utils/logger';
import { replayBufferedHistory } from '../../utils/replay-history';
import type { AgentStreamEvent } from '../../types/agent-events';
import type { CommandContext } from '../types';

type LoadedSession = Awaited<ReturnType<CommandContext['kiro']['loadSession']>>;

export interface KasSessionSwitchOptions {
  /** Loading indicator text shown while the session loads. */
  loadingLabel: string;
  /**
   * Reset the message store before replaying history. Default true. Set false
   * to append the loaded session's history to the current view (e.g. /chat).
   */
  resetMessages?: boolean;
  /** System message rendered before the replayed history (e.g. "Loaded session X"). */
  preReplayMessage?: string;
  /**
   * Resolve the tangent chip name from the loaded session. Receives the load
   * response; may ignore it and close over list-derived data (e.g. /tangent
   * goBack, which derives the name from the session list for reliability).
   * Omit to leave the chip unchanged.
   */
  resolveTangentName?: (session: LoadedSession) => string | null;
  /** Success alert text. Omit to show none (e.g. /rewind). */
  successAlert?: string;
  /** Error alert fallback text. */
  errorLabel?: string;
  /** Suppress the agent welcome message on switch. Default true. */
  suppressAgentWelcome?: boolean;
  /** Log tag for diagnostics (e.g. 'tangent', 'chat', 'rewind'). */
  logTag?: string;
}

/**
 * Load a KAS session and render it into the conversation view.
 * Returns true on success, false on failure.
 */
export async function switchToKasSession(
  ctx: CommandContext,
  sessionId: string,
  opts: KasSessionSwitchOptions
): Promise<boolean> {
  ctx.setLoadingMessage(opts.loadingLabel);
  const buffered: AgentStreamEvent[] = [];
  try {
    const session = await ctx.kiro.loadSession(sessionId, (e) =>
      buffered.push(e)
    );
    ctx.setLoadingMessage(null);
    ctx.clearUIState();
    if (opts.resetMessages !== false) {
      ctx.resetMessages();
      // The TUI renders committed history via an append-only <Static>
      // scrollback that resetMessages() does NOT wipe in TUI mode (its
      // clear-token bump is gated to lite). Without this, a session takeover
      // leaves the previous session's painted rows on screen and the replayed
      // history stacks on top of them — the same line repeats on every switch.
      // Bump the clear token so ConversationView wipes its <Static> singletons
      // + the terminal, so the switch truly replaces instead of appending.
      ctx.bumpLiteScrollbackClear();
    }
    ctx.setSessionId(sessionId);
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent) {
      ctx.setCurrentAgent(session.currentAgent, {
        suppressWelcome: opts.suppressAgentWelcome !== false,
      });
    }
    if (opts.preReplayMessage)
      ctx.addSystemMessage(opts.preReplayMessage, true);
    replayBufferedHistory(ctx, buffered);
    if (opts.resolveTangentName) {
      ctx.setTangentName(opts.resolveTangentName(session));
    }
    if (opts.successAlert) ctx.showAlert(opts.successAlert, 'success', 2000);
    return true;
  } catch (err) {
    logger.error(`[${opts.logTag ?? 'session-switch'}] loadSession failed`, {
      sessionId,
      err,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(
        err,
        opts.errorLabel ?? 'Failed to switch session'
      ),
      'error',
      5000
    );
    return false;
  }
}
