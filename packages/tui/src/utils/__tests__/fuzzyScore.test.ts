import { describe, expect, test } from 'bun:test';
import { fuzzyScore } from '../fuzzyScore.js';

describe('fuzzyScore', () => {
  test('exact match scores highest', () => {
    expect(fuzzyScore('abc', 'abc')).toBeGreaterThan(fuzzyScore('ac', 'abc'));
  });

  test('returns 0 when query is not a subsequence', () => {
    expect(fuzzyScore('xyz', 'abc')).toBe(0);
  });

  test('returns 0 for empty query', () => {
    expect(fuzzyScore('', 'abc')).toBe(0);
  });

  test('consecutive matches score higher than scattered', () => {
    // 'ab' consecutive in 'abc' vs scattered in 'axb'
    expect(fuzzyScore('ab', 'abc')).toBeGreaterThan(fuzzyScore('ab', 'axb'));
  });

  test('word-boundary match gets bonus', () => {
    // 'b' at word boundary (after hyphen) vs mid-word
    expect(fuzzyScore('b', 'a-b')).toBeGreaterThan(fuzzyScore('b', 'ab'));
  });

  test('match at index 0 gets word-boundary bonus', () => {
    // 'a' at start of string gets +2 bonus
    expect(fuzzyScore('a', 'abc')).toBe(3); // 1 base + 2 boundary
  });

  test('single character subsequence', () => {
    expect(fuzzyScore('c', 'abc')).toBe(1); // just base, no boundary
  });

  test('query longer than target returns 0', () => {
    expect(fuzzyScore('abcd', 'abc')).toBe(0);
  });
});
