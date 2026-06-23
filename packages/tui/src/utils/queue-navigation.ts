/**
 * Queue-aware up/down arrow navigation for the lite prompt input.
 *
 * Mirrors Claude Code's behavior: pressing ↑ in an empty input pulls back the
 * most recently staged message into the buffer for editing. Subsequent ↑
 * presses page through older entries; once past the oldest, control
 * falls through to CommandHistory. ↓ walks back toward the newest entry
 * and finally clears the input.
 *
 * Two distinct staging concepts feed the same visible "what runs next" list:
 *
 *   - `queue` entries — the LOCAL `queuedMessages` buffer, drained by
 *     processQueue at turn boundaries. Fully editable in place; the slot
 *     keeps its index in `queuedMessages` so Kiro processes the edited
 *     message in its original order.
 *   - `steer` entries — a mid-turn message the BACKEND holds (echoed back as
 *     `pendingSteerContent`, possibly multi-line: successive steers
 *     concatenate with "\n\n"). Steer content is NOT in `queuedMessages` and
 *     must NOT be appended there (processQueue would then send it a second
 *     time, on top of the backend's own injection). Editing a steer entry is
 *     therefore a "clear-and-resteer" against the backend, never a local FIFO
 *     replace.
 *
 * The displayed list is `[...steerEntries, ...queueEntries]` — steer "cuts the
 * line" because the backend injects it first (see processQueue's steer-first
 * replay). This module operates over that unified, origin-tagged list so the
 * ↑/↓ machine and Enter-commit can route each entry to the correct transport.
 *
 * The actual text replacement is committed by the caller, which inspects the
 * returned entry `kind`:
 *   - queue → `replaceQueuedMessage(queueIndex, text)` / `removeQueuedMessage`
 *   - steer → `clearSteerMessage()` then `kiro.steerMessage(text)` (resteer)
 */

/** A single entry in the unified "what runs next" preview/nav list. */
export type UnifiedQueueEntry =
  | { kind: 'steer'; text: string }
  | { kind: 'queue'; text: string; queueIndex: number };

/**
 * Build the unified, origin-tagged list of pending entries shown in the lite
 * preview strip and walked by ↑/↓. Steer lines (split on the backend's "\n\n"
 * concatenation separator) come first because the backend injects the steer
 * before the local queue drains. Each queue entry carries its index into
 * `queuedMessages` so edits/deletes hit the right slot regardless of how many
 * steer lines sit above it.
 */
export function buildUnifiedQueueEntries(
  pendingSteerContent: string | null,
  queue: readonly string[]
): UnifiedQueueEntry[] {
  const entries: UnifiedQueueEntry[] = [];
  if (pendingSteerContent != null && pendingSteerContent.length > 0) {
    for (const line of pendingSteerContent.split('\n\n')) {
      entries.push({ kind: 'steer', text: line });
    }
  }
  queue.forEach((text, queueIndex) => {
    entries.push({ kind: 'queue', text, queueIndex });
  });
  return entries;
}

const STEER_SEP = '\n\n';

export function spliceSteerLine(
  buffer: string,
  targetLine: string,
  replacement: string
): string {
  const lines = buffer.split(STEER_SEP);
  const at = lines.indexOf(targetLine);
  if (at === -1) return replacement;
  lines[at] = replacement;
  return lines.join(STEER_SEP);
}

export function removeSteerLine(
  buffer: string,
  targetLine: string
): string | null {
  const lines = buffer.split(STEER_SEP);
  const at = lines.indexOf(targetLine);
  if (at === -1) return null;
  lines.splice(at, 1);
  return lines.length > 0 ? lines.join(STEER_SEP) : null;
}

export interface QueueRestoreState {
  /**
   * Position in the UNIFIED entry list (`buildUnifiedQueueEntries`) the input
   * buffer is currently displaying — NOT the raw `queuedMessages` index. Use
   * `kind`/`queueIndex` to route a commit to the right transport.
   */
  index: number;
  /** Origin of the entry the input is currently displaying. */
  kind: 'steer' | 'queue';
  /**
   * For `kind === 'queue'`: the index into `queuedMessages` this entry maps
   * to (so `replaceQueuedMessage`/`removeQueuedMessage` hit the right slot).
   * Undefined for steer entries.
   */
  queueIndex?: number;
  /** The text that was loaded into the input from the entry. */
  originalText: string;
}

/**
 * Instruction to persist an edit before navigating away from a dirty entry.
 * Tagged by origin so the caller routes it correctly:
 *   - queue → replaceQueuedMessage(queueIndex, text)
 *   - steer → clear-and-resteer the whole steer buffer to `text`
 */
export type QueueReplace =
  | { kind: 'queue'; queueIndex: number; text: string }
  | { kind: 'steer'; text: string; targetLine: string };

export type QueueNavResult =
  | {
      kind: 'queue';
      /** New restore state to install. Null means exit restore mode. */
      state: QueueRestoreState | null;
      /** Text to load into the input buffer. */
      loadText: string;
      /** If set, caller must persist this edit before loading (see QueueReplace). */
      replace?: QueueReplace;
    }
  | { kind: 'history' }
  | { kind: 'noop' };

/** Map a unified entry + display index into a fresh restore state. */
function toRestoreState(
  entry: UnifiedQueueEntry,
  index: number
): QueueRestoreState {
  return entry.kind === 'queue'
    ? {
        index,
        kind: 'queue',
        queueIndex: entry.queueIndex,
        originalText: entry.text,
      }
    : { index, kind: 'steer', originalText: entry.text };
}

/** Build the dirty-commit instruction for the entry we're stepping away from. */
function buildReplace(
  state: QueueRestoreState,
  currentInput: string
): QueueReplace | undefined {
  if (currentInput === state.originalText) return undefined;
  return state.kind === 'queue'
    ? { kind: 'queue', queueIndex: state.queueIndex!, text: currentInput }
    : { kind: 'steer', text: currentInput, targetLine: state.originalText };
}

/**
 * Compute what should happen when the user presses ↑ in the prompt input.
 *
 * @param state    current queue-restore state, or null if not in restore mode
 * @param currentInput visible text in the input box (guard against grabbing
 *                     the list mid-typing, and the value to commit if dirty)
 * @param entries  current unified entry list (`buildUnifiedQueueEntries`)
 */
export function navigateQueueUp(
  state: QueueRestoreState | null,
  currentInput: string,
  entries: readonly UnifiedQueueEntry[]
): QueueNavResult {
  // Not in restore mode yet — entering it requires an empty input AND a
  // non-empty list. If the user has typed anything, we let CommandHistory
  // handle ↑ as usual; tampering with the staged list mid-compose is too
  // surprising.
  if (state == null) {
    if (entries.length === 0) return { kind: 'history' };
    if (currentInput.length > 0) return { kind: 'history' };
    const idx = entries.length - 1;
    const entry = entries[idx]!;
    return {
      kind: 'queue',
      state: toRestoreState(entry, idx),
      loadText: entry.text,
    };
  }

  // Already in restore mode — commit edits to the current entry before we
  // step away from it. Dirty is derived from whether the current input
  // differs from what we loaded, so we don't track edits as a separate bool.
  const replace = buildReplace(state, currentInput);

  const newIndex = state.index - 1;
  if (newIndex < 0) {
    // Walked past the oldest entry — exit restore mode and let CommandHistory
    // take over. The input keeps whatever we just committed; caller clears or
    // hands to history.
    return { kind: 'queue', state: null, loadText: '', replace };
  }
  const entry = entries[newIndex]!;
  return {
    kind: 'queue',
    state: toRestoreState(entry, newIndex),
    loadText: entry.text,
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
  entries: readonly UnifiedQueueEntry[]
): QueueNavResult {
  if (state == null) return { kind: 'history' };

  const replace = buildReplace(state, currentInput);

  const newIndex = state.index + 1;
  if (newIndex >= entries.length) {
    // Stepped past the newest entry — exit restore mode and clear the input.
    // The user can keep pressing ↓ which will be a no-op.
    return { kind: 'queue', state: null, loadText: '', replace };
  }
  const entry = entries[newIndex]!;
  return {
    kind: 'queue',
    state: toRestoreState(entry, newIndex),
    loadText: entry.text,
    replace,
  };
}

/**
 * Result of committing an in-restore edit on Enter.
 *   - replace-queue → replaceQueuedMessage(queueIndex, text)
 *   - replace-steer → clear-and-resteer the steer buffer to `text`
 *   - delete-queue  → removeQueuedMessage(queueIndex) (empty edit)
 *   - delete-steer  → clearSteerMessage() (empty edit)
 *   - fallback      → slot drained/shifted; send as a fresh message
 */
export type QueueCommitResult =
  | { kind: 'replace-queue'; queueIndex: number; text: string }
  | { kind: 'replace-steer'; text: string; targetLine: string }
  | { kind: 'delete-queue'; queueIndex: number }
  | { kind: 'delete-steer'; targetLine: string }
  | { kind: 'fallback'; text: string };

/**
 * On Enter while in queue-restore mode, commit the current input to the entry
 * it was loaded from.
 *
 * For QUEUE entries: the message stays in the queue at its original index so
 * Kiro processes it in order. If the slot is no longer valid (the queue
 * drained while editing), fall back to a normal send so edits aren't lost.
 *
 * For STEER entries: the edit becomes a clear-and-resteer (the caller clears
 * the backend steer and resteers `text`). We always honor the steer edit even
 * if the live steer buffer changed underneath us — resteering the edited text
 * is the user's clear intent; the alternative (silent loss) is worse. An empty
 * edit deletes (clear) the steer.
 *
 * @param state        the active restore state
 * @param currentInput the edited text
 * @param entries      the current unified entry list (to validate queue slots)
 */
export function commitQueueRestore(
  state: QueueRestoreState,
  currentInput: string,
  entries: readonly UnifiedQueueEntry[]
): QueueCommitResult {
  const trimmed = currentInput.trim();

  if (state.kind === 'steer') {
    if (!trimmed)
      return { kind: 'delete-steer', targetLine: state.originalText };
    return {
      kind: 'replace-steer',
      text: currentInput,
      targetLine: state.originalText,
    };
  }

  // Queue entry: validate the slot still holds the originalText at the same
  // display position before committing in place; otherwise fall back.
  const entry = entries[state.index];
  const stillThere =
    entry != null &&
    entry.kind === 'queue' &&
    entry.queueIndex === state.queueIndex &&
    entry.text === state.originalText;
  if (!stillThere) {
    return { kind: 'fallback', text: currentInput };
  }
  if (!trimmed) {
    return { kind: 'delete-queue', queueIndex: state.queueIndex! };
  }
  return {
    kind: 'replace-queue',
    queueIndex: state.queueIndex!,
    text: currentInput,
  };
}
