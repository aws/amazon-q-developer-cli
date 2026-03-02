import { MessageRole } from '../stores/app-store.js';

export interface FlushableMessage {
  id: string;
  role: MessageRole;
  isFinished?: boolean; // ToolUse only
  content?: string; // Model only
}

/**
 * Pure state machine: given the ordered list of messages in a turn
 * (userMessage first, then aiMessages) and whether the turn is still
 * processing, returns the number of leading messages that are safe to
 * flush to the static buffer.
 *
 * Rules (FIFO — a message only flushes when all before it are flushed):
 *   User    → always done
 *   ToolUse → done when isFinished=true
 *   Model   → done when followed by any message (i.e. not the last), OR when !isProcessing
 *
 * The tail (messages NOT flushed) is everything from flushCount onward.
 * Callers keep at least TAIL_SIZE messages in the tail regardless — this
 * ensures the last active tool and streaming model text stay visible in the
 * dynamic area while the turn is live. The tail is only flushed to <Static>
 * when the turn completes (moves to completedTurns in ConversationView).
 *
 * The tailSize cap applies unconditionally (even when !isProcessing) because
 * the intent is: keep the tail visible until the NEXT turn starts, not until
 * processing stops. ConversationView's completedTurns path handles the final flush.
 */
export function computeFlushCount(
  messages: FlushableMessage[],
  isProcessing: boolean,
  tailSize: number
): number {
  // Walk forward and find the furthest contiguous run of done messages
  let doneCount = 0;
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]!;
    const isLast = i === messages.length - 1;

    if (msg.role === MessageRole.User) {
      doneCount = i + 1;
    } else if (msg.role === MessageRole.ToolUse) {
      if (msg.isFinished) {
        doneCount = i + 1;
      } else {
        break; // unfinished tool blocks everything after it
      }
    } else if (msg.role === MessageRole.Model) {
      // Done if not the last message (something follows it, so it won't grow)
      // or if processing has stopped
      if (!isLast || !isProcessing) {
        doneCount = i + 1;
      } else {
        break; // still streaming — don't flush
      }
    } else {
      // Unknown role — treat as done
      doneCount = i + 1;
    }
  }

  // Never flush more than (total - tailSize) messages
  return Math.max(0, Math.min(doneCount, messages.length - tailSize));
}
