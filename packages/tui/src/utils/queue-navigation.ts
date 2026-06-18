/**
 * Queue-aware up/down arrow navigation for the lite prompt input.
 *
 * Mirrors Claude Code's behavior: pressing ↑ in an empty input pulls back the
 * most recently queued message into the buffer for editing. Subsequent ↑
 * presses page through older queue entries; once past the oldest, control
 * falls through to CommandHistory. ↓ walks back toward the newest queue entry
 * and finally clears the input.
 *
 * The queued message stays at its original index in `queuedMessages` so the
 * order in which Kiro processes the queue is preserved across edits — even
 * if the user only meant to inspect the message and pressed ↑↓ to scroll.
 *
 * The actual text replacement (when `dirty`) is committed via
 * `replaceQueuedMessage(index, text)` in the app store; the caller is
 * responsible for invoking that on the returned `replace` instruction.
 */

export interface QueueRestoreState {
  /** Which queue slot the input buffer is currently displaying. */
  index: number;
  /** The text that was loaded into the input from the queue. */
  originalText: string;
}

export type QueueNavResult =
  | {
      kind: 'queue';
      /** New restore state to install. Null means exit restore mode. */
      state: QueueRestoreState | null;
      /** Text to load into the input buffer. */
      loadText: string;
      /** If set, caller must call replaceQueuedMessage(index, text) before loading. */
      replace?: { index: number; text: string };
    }
  | { kind: 'history' }
  | { kind: 'noop' };

/**
 * Compute what should happen when the user presses ↑ in the prompt input.
 *
 * @param state         current queue-restore state, or null if not in restore mode
 * @param currentInput  visible text in the input box (used both as a guard
 *                      against grabbing the queue mid-typing and as the
 *                      value to commit back if the slot is dirty)
 * @param queue         current queuedMessages
 */
export function navigateQueueUp(
  state: QueueRestoreState | null,
  currentInput: string,
  queue: readonly string[]
): QueueNavResult {
  // Not in restore mode yet — entering it requires an empty input AND a
  // non-empty queue. If the user has typed anything, we let CommandHistory
  // handle ↑ as usual; tampering with the queue mid-compose is too surprising.
  if (state == null) {
    if (queue.length === 0) return { kind: 'history' };
    if (currentInput.length > 0) return { kind: 'history' };
    const idx = queue.length - 1;
    const text = queue[idx]!;
    return {
      kind: 'queue',
      state: { index: idx, originalText: text },
      loadText: text,
    };
  }

  // Already in restore mode — commit edits to the current slot before we
  // step away from it. The slot retains its position in the queue. Dirty
  // is derived from whether the current input differs from what we loaded,
  // so we don't have to track edits as a separate boolean.
  const dirty = currentInput !== state.originalText;
  const replace = dirty
    ? { index: state.index, text: currentInput }
    : undefined;

  const newIndex = state.index - 1;
  if (newIndex < 0) {
    // Walked past the oldest queued message — exit restore mode and let
    // CommandHistory take over from here. The input keeps whatever we just
    // committed back, so the caller should clear or hand to history.
    return {
      kind: 'queue',
      state: null,
      loadText: '',
      replace,
    };
  }
  const text = queue[newIndex]!;
  return {
    kind: 'queue',
    state: { index: newIndex, originalText: text },
    loadText: text,
    replace,
  };
}

/**
 * Compute what should happen when the user presses ↓ in the prompt input.
 *
 * Symmetric with `navigateQueueUp` but exits restore mode when stepping past
 * the newest entry (input becomes empty). When not in restore mode, falls
 * through to CommandHistory unchanged.
 */
export function navigateQueueDown(
  state: QueueRestoreState | null,
  currentInput: string,
  queue: readonly string[]
): QueueNavResult {
  if (state == null) return { kind: 'history' };

  const dirty = currentInput !== state.originalText;
  const replace = dirty
    ? { index: state.index, text: currentInput }
    : undefined;

  const newIndex = state.index + 1;
  if (newIndex >= queue.length) {
    // Stepped past the newest queued message — exit restore mode and clear
    // the input. The user can keep pressing ↓ which will be a no-op.
    return {
      kind: 'queue',
      state: null,
      loadText: '',
      replace,
    };
  }
  const text = queue[newIndex]!;
  return {
    kind: 'queue',
    state: { index: newIndex, originalText: text },
    loadText: text,
    replace,
  };
}

/**
 * On Enter while in queue-restore mode, commit the current input to the
 * slot it was loaded from. The message stays in the queue at its original
 * index so Kiro processes it in order. If `state.index` is no longer valid
 * (the queue drained while the user was editing), the caller should fall
 * back to a normal `onSubmit` so the user's edits aren't lost.
 */
export function commitQueueRestore(
  state: QueueRestoreState,
  currentInput: string,
  queue: readonly string[]
):
  | { kind: 'replace'; index: number; text: string }
  | { kind: 'fallback'; text: string } {
  const stillThere =
    state.index < queue.length && queue[state.index] === state.originalText;
  if (!stillThere) {
    return { kind: 'fallback', text: currentInput };
  }
  return { kind: 'replace', index: state.index, text: currentInput };
}
