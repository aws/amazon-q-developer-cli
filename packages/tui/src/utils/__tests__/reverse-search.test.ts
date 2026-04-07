import { describe, expect, it } from 'bun:test';
import {
  searchHistory,
  createReverseSearchState,
  enterSearch,
  appendQuery,
  backspaceQuery,
  cycleOlder,
  exitSearch,
  abortSearch,
  formatPrompt,
} from '../reverse-search';

describe('searchHistory', () => {
  const history = ['abc', 'def', 'abcdef'];

  it('finds substring match from newest', () => {
    const result = searchHistory(history, 'abc', history.length);
    expect(result).toEqual({
      historyIndex: 2,
      matchPosition: 0,
      line: 'abcdef',
    });
  });

  it('with startBefore skips recent entries', () => {
    const result = searchHistory(history, 'abc', 2);
    expect(result).toEqual({ historyIndex: 0, matchPosition: 0, line: 'abc' });
  });

  it('returns null on no match', () => {
    expect(searchHistory(history, 'xyz', history.length)).toBeNull();
  });

  it('positions cursor at match start', () => {
    const result = searchHistory(history, 'def', history.length);
    expect(result!.matchPosition).toBe(3); // "abcdef" -> "def" at index 3
  });

  it('returns null for empty query', () => {
    expect(searchHistory(history, '', history.length)).toBeNull();
  });
});

describe('state machine', () => {
  const history = ['echo alpha', 'echo beta', 'hello world'];

  it('enterSearch initializes state', () => {
    const state = createReverseSearchState();
    enterSearch(state, 'current', 5);
    expect(state.active).toBe(true);
    expect(state.query).toBe('');
    expect(state.match).toBeNull();
    expect(state.savedInput).toBe('current');
    expect(state.savedCursor).toBe(5);
  });

  it('appendQuery finds match and narrows', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);

    appendQuery(state, 'e', history);
    expect(state.match!.line).toBe('hello world'); // most recent with 'e'

    appendQuery(state, 'c', history);
    expect(state.query).toBe('ec');
    expect(state.match!.line).toBe('echo beta'); // most recent with 'ec'
  });

  it('backspaceQuery shortens query and re-searches from newest', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'e', history);
    appendQuery(state, 'c', history);
    appendQuery(state, 'h', history);
    expect(state.query).toBe('ech');

    backspaceQuery(state, history);
    expect(state.query).toBe('ec');
    expect(state.match!.line).toBe('echo beta'); // re-searched from newest
  });

  it('backspaceQuery on empty query is no-op', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    backspaceQuery(state, history);
    expect(state.query).toBe('');
    expect(state.match).toBeNull();
  });

  it('backspaceQuery to empty clears match', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'x', history);
    // Even if no match, backspace to empty should clear
    backspaceQuery(state, history);
    expect(state.query).toBe('');
    expect(state.match).toBeNull();
  });

  it('cycleOlder moves to earlier match', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'echo', history);
    expect(state.match!.line).toBe('echo beta'); // index 1

    cycleOlder(state, history);
    expect(state.match!.line).toBe('echo alpha'); // index 0
  });

  it('cycleOlder with no more matches keeps current', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'hello', history);
    expect(state.match!.line).toBe('hello world');

    cycleOlder(state, history); // no older match
    expect(state.match!.line).toBe('hello world'); // unchanged
  });

  it('exitSearch with matchPos returns match position', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'world', history);
    const result = exitSearch(state, 'matchPos');
    expect(result.text).toBe('hello world');
    expect(result.cursor).toBe(6); // "hello " = 6
    expect(state.active).toBe(false);
  });

  it('exitSearch with start returns cursor 0', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'echo', history);
    const result = exitSearch(state, 'start');
    expect(result.cursor).toBe(0);
  });

  it('exitSearch with end returns cursor at end', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'echo', history);
    const result = exitSearch(state, 'end');
    expect(result.cursor).toBe('echo beta'.length);
  });

  it('exitSearch with no match returns saved input', () => {
    const state = createReverseSearchState();
    enterSearch(state, 'original', 3);
    // No query appended, no match
    const result = exitSearch(state, 'matchPos');
    expect(result.text).toBe('original');
    expect(result.cursor).toBe(0);
  });

  it('abortSearch restores saved input and cursor', () => {
    const state = createReverseSearchState();
    enterSearch(state, 'my input', 5);
    appendQuery(state, 'echo', history);
    const result = abortSearch(state);
    expect(result.text).toBe('my input');
    expect(result.cursor).toBe(5);
    expect(state.active).toBe(false);
  });
});

describe('formatPrompt', () => {
  it('formats with match', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    appendQuery(state, 'hello', ['hello world']);
    expect(formatPrompt(state)).toBe("(reverse-i-search)`hello': hello world");
  });

  it('formats with empty query', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    expect(formatPrompt(state)).toBe("(reverse-i-search)`': ");
  });

  it('formats with query but no match', () => {
    const state = createReverseSearchState();
    enterSearch(state, '', 0);
    state.query = 'xyz';
    expect(formatPrompt(state)).toBe("(reverse-i-search)`xyz': ");
  });
});
