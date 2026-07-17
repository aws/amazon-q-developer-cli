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
import { LITE_HISTORY_RENDER_CAP } from '../components/layout/lite/static-flush.js';
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
  // Lite and TUI share this flow, but lite has display-model needs the TUI
  // doesn't: its <Static> scrollback must be reset when the shown session is
  // replaced, it tags replayed rows by agent (so the loaded agent must be
  // applied BEFORE replay), it paints the full history once, and it drains
  // its input queue afterward. Each lite-only step is gated on `isLite`; the
  // `else`/ungated paths are byte-for-byte main's original behavior so the
  // modern TUI is unaffected.
  const isLite = ctx.getUiMode?.() === 'lite';
  ctx.clearUIState();
  if (options.resetMessagesBeforeReplay || isLite) {
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
      // Lite: apply the loaded session's agent BEFORE replay. The stream
      // handler stamps `agentName: currentAgent?.name` on every row it
      // creates; if applied after replay, historical tool rows would be
      // tagged with the pre-load agent and dropped by isInnerSubagentTool.
      // Welcome is suppressed — the user already saw it in the prior session.
      if (isLite) {
        if (session.currentModel) ctx.setCurrentModel(session.currentModel);
        if (session.currentAgent)
          ctx.setCurrentAgent(session.currentAgent, { suppressWelcome: true });
      }
      if (buffered.length > 0) {
        // Lite paints history into <Static> once (then skips it via
        // lite.staticSkipBefore — ~zero render cost), so it replays the full
        // session. The TUI re-renders every store message each frame, so it
        // caps to the most recent turns.
        const { events, omittedTurns } = isLite
          ? { events: buffered, omittedTurns: 0 }
          : truncateToRecentTurns(buffered, MAX_DISPLAY_TURNS);
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
      // Lite-only: clamp painted history to the most recent rows. The bookmark
      // is a static lower bound — live turns appended later render normally.
      if (isLite) {
        ctx.setLiteStaticSkipBefore?.(
          Math.max(0, ctx.getMessages().length - LITE_HISTORY_RENDER_CAP)
        );
      }
      ctx.setLoadingMessage(null);
      ctx.setSessionId(sessionId);
      // TUI applies model/agent after replay (lite already did, above).
      if (!isLite) {
        if (session.currentModel) ctx.setCurrentModel(session.currentModel);
        if (session.currentAgent)
          ctx.setCurrentAgent(session.currentAgent, {
            suppressWelcome: options.suppressAgentWelcome,
          });
      }
      ctx.showAlert('Session loaded', 'success', 3000);
      // Lite gates input on loadingMessage and queues anything typed during
      // load; drain it now that loading is done.
      if (isLite) void ctx.processQueue();
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
      if (isLite) void ctx.processQueue();
    });
}
