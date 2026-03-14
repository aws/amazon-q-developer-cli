import { MessageRole } from '../stores/app-store.js';

export interface FlushableMessage {
  id: string;
  role: MessageRole;
  isFinished?: boolean; // ToolUse only
  content?: string; // Model only
}

/**
 * Hard ceiling on the dynamic tail to prevent viewport overflow.
 * When the tail would exceed this, we selectively flush finished
 * messages from the tail — but NEVER flush active (unfinished) tools
 * or a currently-streaming model message.
 */
export const MAX_TAIL_SIZE = 6;

/**
 * Pure state machine that decides which messages to flush to static.
 *
 * Returns a Set of message IDs that should be flushed. The caller
 * renders messages NOT in this set as the dynamic tail.
 *
 * Normal rules (FIFO):
 *   - Walk forward; a message is "done" if:
 *       User    → always
 *       ToolUse → isFinished=true
 *       Model   → not the last message, OR !isProcessing
 *   - Stop at the first non-done message.
 *   - Keep at least `tailSize` messages in the tail.
 *
 * MAX_TAIL_SIZE guardrail:
 *   If the tail still exceeds MAX_TAIL_SIZE, we flush additional
 *   **finished** messages from the tail (breaking FIFO order).
 *   Active tools and streaming models are NEVER flushed.
 */
export function computeFlushSet(
  messages: FlushableMessage[],
  isProcessing: boolean,
  tailSize: number
): Set<string> {
  const flushSet = new Set<string>();
  const isLast = (i: number) => i === messages.length - 1;

  // --- Phase 1: normal FIFO flush ---
  let doneCount = 0;
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]!;
    if (msg.role === MessageRole.User) {
      doneCount = i + 1;
    } else if (msg.role === MessageRole.ToolUse) {
      if (msg.isFinished) {
        doneCount = i + 1;
      } else {
        break;
      }
    } else if (msg.role === MessageRole.Model) {
      if (!isLast(i) || !isProcessing) {
        doneCount = i + 1;
      } else {
        break;
      }
    } else {
      doneCount = i + 1;
    }
  }

  const fifoFlush = Math.max(
    0,
    Math.min(doneCount, messages.length - tailSize)
  );
  for (let i = 0; i < fifoFlush; i++) {
    flushSet.add(messages[i]!.id);
  }

  // --- Phase 2: MAX_TAIL_SIZE guardrail ---
  // Count how many messages are NOT flushed
  let tailLen = messages.length - flushSet.size;
  if (tailLen <= MAX_TAIL_SIZE) return flushSet;

  // Walk the tail (everything after fifoFlush) and flush finished
  // messages until we're back under the cap. Never flush active tools
  // or a streaming model at the end.
  for (let i = fifoFlush; i < messages.length && tailLen > MAX_TAIL_SIZE; i++) {
    const msg = messages[i]!;
    if (flushSet.has(msg.id)) continue;

    const isActive =
      (msg.role === MessageRole.ToolUse && !msg.isFinished) ||
      (msg.role === MessageRole.Model && isLast(i) && isProcessing);

    if (!isActive) {
      flushSet.add(msg.id);
      tailLen--;
    }
  }

  return flushSet;
}
