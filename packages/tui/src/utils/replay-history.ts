import {
  AgentEventType,
  type AgentStreamEvent,
} from '../types/agent-events.js';

/**
 * Truncate a buffered history-event sequence to the last N user turns.
 *
 * A turn starts at each `UserMessage` event and includes every subsequent
 * event until the next `UserMessage`. Returns the trailing slice plus the
 * count of turns that were dropped from the head.
 */
export function truncateToRecentTurns(
  events: AgentStreamEvent[],
  maxTurns: number
): { events: AgentStreamEvent[]; omittedTurns: number } {
  const turnStarts: number[] = [];
  for (let i = 0; i < events.length; i++) {
    if (events[i]!.type === AgentEventType.UserMessage) {
      turnStarts.push(i);
    }
  }
  if (turnStarts.length <= maxTurns) {
    return { events, omittedTurns: 0 };
  }
  const keepFrom = turnStarts[turnStarts.length - maxTurns]!;
  return {
    events: events.slice(keepFrom),
    omittedTurns: turnStarts.length - maxTurns,
  };
}

/** Minimal context needed to render replayed history events. */
export interface HistoryReplayContext {
  addSystemMessage: (content: string, success: boolean) => void;
  createStreamEventHandler: (options?: { fromHistory?: boolean }) => {
    (event: AgentStreamEvent): void;
    flush?: () => void;
  };
}

/**
 * Replay buffered session-history events into the conversation view, truncated
 * to the last `maxTurns` turns. Prepends a "⋯ N earlier turns not shown" system
 * message when turns were dropped. Currently used by the /tangent switch path;
 * /chat and /rewind still replay inline and are intended to converge on this
 * helper in a later migration.
 */
export function replayBufferedHistory(
  ctx: HistoryReplayContext,
  buffered: AgentStreamEvent[],
  maxTurns = 10
): void {
  if (buffered.length === 0) return;
  const { events, omittedTurns } = truncateToRecentTurns(buffered, maxTurns);
  if (omittedTurns > 0) {
    ctx.addSystemMessage(
      `⋯ ${omittedTurns} earlier turn${omittedTurns === 1 ? '' : 's'} not shown`,
      true
    );
  }
  // fromHistory: replayed tool rows have no persisted duration, so the store
  // must skip stamping fresh Date.now() start/finish times on them (matches
  // /chat, /rewind, and session-load). Without it, a tangent switch renders
  // replayed tool calls with bogus current-time timing chips.
  const handler = ctx.createStreamEventHandler({ fromHistory: true });
  for (const e of events) handler(e);
  handler.flush?.();
}
