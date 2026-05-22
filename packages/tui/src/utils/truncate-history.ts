import {
  AgentEventType,
  type AgentStreamEvent,
} from '../types/agent-events.js';

/** Maximum number of user turns to display when replaying history. */
export const MAX_DISPLAY_TURNS = 10;

/**
 * Keep only the last `maxTurns` user turns from a buffered event stream.
 * A "turn" starts at each UserMessage event and includes all subsequent
 * events until the next UserMessage.  Returns the truncated slice and
 * how many turns were dropped.
 */
export function truncateToRecentTurns(
  events: AgentStreamEvent[],
  maxTurns: number = MAX_DISPLAY_TURNS
): { events: AgentStreamEvent[]; omittedTurns: number } {
  // Find indices where each user turn starts
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
