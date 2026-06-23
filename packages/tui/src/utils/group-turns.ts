import {
  MessageRole,
  type ConversationTurn,
  type MessageType,
} from '../stores/app-store';

/**
 * Group a flat conversation message list into {@link ConversationTurn}s.
 *
 * A turn is anchored by a user *prompt* and owns the body it produced —
 * assistant content, tool calls, AND mid-turn steered user messages.
 *
 * ## Steering
 *
 * Steering injects user messages *mid-turn*: the agent is already working on a
 * prompt, and the steered text is folded into that same turn's continuation.
 * Multiple steers are concatenated into a single agent response, so an injected
 * bubble legitimately has no response of its own — the shared response attaches
 * to the end of the turn.
 *
 * To reflect that, a user message flagged `steered` does NOT open a new turn;
 * it is appended to the current turn's body in order (so it renders inline at
 * its injection point). Only a non-steered user prompt starts a new turn.
 *
 * This is what keeps injected steers from rendering as standalone, response-less
 * turns that the view would otherwise mislabel as "Cancelled".
 *
 * `isActive` is set on the trailing turn (the one still accruing output); the
 * caller decides how to treat it. Standalone `Model` messages (e.g. the welcome
 * message) and any body that appears before the first prompt become their own
 * inactive, anchor-only turns — matching the prior inline behavior.
 */
export function groupMessagesIntoTurns(
  messages: MessageType[]
): ConversationTurn[] {
  const turns: ConversationTurn[] = [];
  let currentTurn: ConversationTurn | null = null;

  const isSteeredUser = (msg: MessageType): boolean =>
    msg.role === MessageRole.User &&
    (msg as { steered?: boolean }).steered === true;

  for (const msg of messages) {
    if (msg.role === MessageRole.User && !isSteeredUser(msg)) {
      // A fresh prompt closes the previous turn and opens a new one.
      if (currentTurn) {
        currentTurn.isActive = false;
        turns.push(currentTurn);
      }
      currentTurn = { userMessage: msg, aiMessages: [], isActive: true };
    } else if (
      msg.role === MessageRole.Model &&
      (msg as { standalone?: boolean }).standalone
    ) {
      // Standalone model message (welcome banner, etc.) — its own turn.
      if (currentTurn) {
        currentTurn.isActive = false;
        turns.push(currentTurn);
        currentTurn = null;
      }
      turns.push({ userMessage: msg, aiMessages: [], isActive: false });
    } else if (currentTurn) {
      // Body of the active turn: assistant content, tool calls, and steered
      // (mid-turn injected) user messages all land here, preserving order.
      currentTurn.aiMessages.push(msg);
    } else {
      // Body with no preceding prompt (e.g. a steer or model chunk before any
      // user message). Defensive: render as an anchor-only inactive turn.
      turns.push({ userMessage: msg, aiMessages: [], isActive: false });
    }
  }

  if (currentTurn) turns.push(currentTurn);

  return turns;
}
