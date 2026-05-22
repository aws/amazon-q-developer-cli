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
