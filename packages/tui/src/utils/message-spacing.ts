import { MessageRole, type MessageType } from '../stores/app-store';

/**
 * Leading blank-line gap (in rows) for a body message, based on the message
 * before it. Single source of truth for the conversation's vertical rhythm so
 * every render path (active-turn tail, static flush, completed-turn card) stays
 * consistent.
 *
 * Rules:
 *  - Assistant (`Model`) output is separated from preceding tool output or a
 *    prior assistant chunk, but sits flush under the user prompt that triggered
 *    it (no gap directly after a user message).
 *  - A steered (mid-turn injected) user message is set off from the agent
 *    output it interrupts, so it's clear where the steer landed.
 *  - `prevRole === undefined` (first message in its list) never gets a gap.
 */
export function leadingGap(
  message: MessageType,
  prevRole: MessageRole | undefined
): number {
  if (prevRole === undefined) return 0;
  switch (message.role) {
    case MessageRole.User:
      return message.steered ? 1 : 0;
    case MessageRole.Model:
      return prevRole === MessageRole.User ? 0 : 1;
    default:
      return 0;
  }
}
