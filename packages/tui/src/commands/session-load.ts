/**
 * Shared `runSessionLoad` helper used by both the V2 chat handler
 * and the `/rewind` effect. Wraps `ctx.kiro.loadSession` with the
 * UI orchestration that every load-existing-session flow needs:
 * clearing transient UI state, optionally resetting message history,
 * showing a loading message, replaying buffered history events into
 * the message store (capped to recent turns), and updating model /
 * agent state on success.
 */

import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { logger } from '../utils/logger.js';
import {
  truncateToRecentTurns,
  MAX_DISPLAY_TURNS,
} from '../utils/truncate-history.js';
import type { AgentStreamEvent } from '../types/agent-events.js';
import type { CommandContext } from './types.js';

export interface RunSessionLoadOptions {
  /**
   * Drop the previous session's live messages before replaying the
   * loaded session's history. `/rewind` sets this so stale turns
   * from the old session don't appear in the forked session's
   * display; bare `/chat <id>` leaves prior context alone.
   */
  resetMessagesBeforeReplay: boolean;
  /**
   * Skip the agent welcome message when the loaded session's agent
   * differs from the active one. `/rewind` sets this since the user
   * is continuing, not starting fresh.
   */
  suppressAgentWelcome: boolean;
}

export function runSessionLoad(
  sessionId: string,
  ctx: CommandContext,
  options: RunSessionLoadOptions
): void {
  ctx.clearUIState();
  if (options.resetMessagesBeforeReplay) {
    ctx.resetMessages();
  }
  ctx.setLoadingMessage(`Loading session ${sessionId}...`);

  const buffered: AgentStreamEvent[] = [];

  ctx.kiro
    .loadSession(sessionId, (e) => buffered.push(e))
    .then((session) => {
      logger.debug('[chat] loadSession resolved', {
        sessionId,
        bufferedCount: buffered.length,
      });
      ctx.addSystemMessage(`Loaded session ${sessionId}`, true);
      if (buffered.length > 0) {
        const { events, omittedTurns } = truncateToRecentTurns(
          buffered,
          MAX_DISPLAY_TURNS
        );
        if (omittedTurns > 0) {
          ctx.addSystemMessage(
            `⋯ ${omittedTurns} earlier turn${omittedTurns === 1 ? '' : 's'} not shown`,
            true
          );
        }
        const handler = ctx.createStreamEventHandler();
        for (const e of events) handler(e);
        // TODO: extend `createStreamEventHandler` return type to expose
        // `flush` rather than escaping through `as any`. KAS's
        // `loadExistingSession` has the same cast and both should be
        // cleaned up together.
        (handler as any).flush?.();
      }
      ctx.setLoadingMessage(null);
      ctx.setSessionId(sessionId);
      if (session.currentModel) ctx.setCurrentModel(session.currentModel);
      if (session.currentAgent)
        ctx.setCurrentAgent(session.currentAgent, {
          suppressWelcome: options.suppressAgentWelcome,
        });
      ctx.showAlert('Session loaded', 'success', 3000);
    })
    .catch((err: unknown) => {
      logger.error('[chat] loadSession failed', {
        sessionId,
        err: JSON.stringify(err),
        stack: err instanceof Error ? err.stack : undefined,
      });
      ctx.setLoadingMessage(null);
      const message = extractRpcErrorMessage(err, 'Failed to load session');
      ctx.showAlert(message, 'error', 5000);
    });
}
