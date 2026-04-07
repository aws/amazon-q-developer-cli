/**
 * Reverse incremental search (Ctrl+R) — standalone state machine.
 * No React dependencies; pure functions + mutable state object.
 */

export interface ReverseSearchMatch {
  /** Index into the history array */
  historyIndex: number;
  /** Character offset where the query was found in the history entry */
  matchPosition: number;
  /** The full history entry text */
  line: string;
}

export interface ReverseSearchState {
  active: boolean;
  query: string;
  match: ReverseSearchMatch | null;
  /** Input text that was in the buffer before search started (for Escape restore) */
  savedInput: string;
  /** Saved cursor position before search started */
  savedCursor: number;
  /** Last query from a previous search session (for double Ctrl+R reuse) */
  lastQuery: string;
}

export function createReverseSearchState(): ReverseSearchState {
  return { active: false, query: '', match: null, savedInput: '', savedCursor: 0, lastQuery: '' };
}

/**
 * Search history (newest-first) for a substring match.
 * @param history - Array of history entries (oldest first, like CommandHistory.getAll())
 * @param query - Substring to search for
 * @param startBefore - Start searching before this index (exclusive). Pass history.length to search from newest.
 * @returns Match info or null
 */
export function searchHistory(
  history: string[],
  query: string,
  startBefore: number,
): ReverseSearchMatch | null {
  if (!query) return null;
  for (let i = Math.min(startBefore, history.length) - 1; i >= 0; i--) {
    const line = history[i]!;
    const pos = line.indexOf(query);
    if (pos !== -1) {
      return { historyIndex: i, matchPosition: pos, line };
    }
  }
  return null;
}

/** Enter reverse search mode, saving current input for potential Escape restore. */
export function enterSearch(state: ReverseSearchState, currentInput: string, currentCursor: number): void {
  state.active = true;
  state.query = '';
  state.match = null;
  state.savedInput = currentInput;
  state.savedCursor = currentCursor;
}

/** Append a character to the query and re-search. */
export function appendQuery(state: ReverseSearchState, char: string, history: string[]): void {
  state.query += char;
  const startBefore = state.match ? state.match.historyIndex + 1 : history.length;
  const result = searchHistory(history, state.query, startBefore);
  if (result) {
    state.match = result;
  } else {
    // Try searching from the end (query changed, might match something newer)
    const fullResult = searchHistory(history, state.query, history.length);
    if (fullResult) state.match = fullResult;
    // If still no match, keep existing match (no-match behavior)
  }
}

/** Delete last character from query and re-search from newest. */
export function backspaceQuery(state: ReverseSearchState, history: string[]): void {
  if (state.query.length === 0) return;
  state.query = state.query.slice(0, -1);
  if (state.query.length === 0) {
    state.match = null;
    return;
  }
  // Re-search from newest since query shortened; keep previous match if still no match
  const result = searchHistory(history, state.query, history.length);
  if (result) state.match = result;
}

/** Cycle to the next older match (Ctrl+R pressed again). */
export function cycleOlder(state: ReverseSearchState, history: string[]): void {
  // If query is empty, reuse the last search string (double Ctrl+R behavior)
  if (!state.query && state.lastQuery) {
    state.query = state.lastQuery;
    state.match = searchHistory(history, state.query, history.length);
    return;
  }
  if (!state.query) return;
  const startBefore = state.match ? state.match.historyIndex : history.length;
  const result = searchHistory(history, state.query, startBefore);
  if (result) state.match = result;
}

/** Exit search, returning the accepted line and cursor position. */
export function exitSearch(
  state: ReverseSearchState,
  cursorMode: 'matchPos' | 'start' | 'end',
): { text: string; cursor: number } {
  const text = state.match?.line ?? state.savedInput;
  let cursor: number;
  switch (cursorMode) {
    case 'start':
      cursor = 0;
      break;
    case 'end':
      cursor = text.length;
      break;
    case 'matchPos':
      cursor = state.match?.matchPosition ?? 0;
      break;
  }
  if (state.query) state.lastQuery = state.query;
  state.active = false;
  state.query = '';
  state.match = null;
  return { text, cursor };
}

/** Abort search (Escape), restoring original input. */
export function abortSearch(state: ReverseSearchState): { text: string; cursor: number } {
  const result = { text: state.savedInput, cursor: state.savedCursor };
  state.active = false;
  state.query = '';
  state.match = null;
  return result;
}

/** Build the display string for the prompt: (reverse-i-search)`query': matched_line */
export function formatPrompt(state: ReverseSearchState): string {
  const line = state.match?.line ?? '';
  return `(reverse-i-search)\`${state.query}': ${line}`;
}
