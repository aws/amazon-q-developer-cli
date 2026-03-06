import { describe, it, expect } from 'bun:test';
import { expandTabs } from '../string';

describe('expandTabs', () => {
  it('returns string unchanged when no tabs', () => {
    expect(expandTabs('hello world')).toBe('hello world');
  });

  it('expands tab at start of line (tabWidth=2)', () => {
    expect(expandTabs('\thi')).toBe('  hi');
  });

  it('expands tab respecting column position', () => {
    // 'a' is at col 0, tab at col 1 needs 1 space to reach col 2
    expect(expandTabs('a\tb')).toBe('a b');
  });

  it('expands multiple tabs', () => {
    // col 0: tab → 2 spaces, col 2: tab → 2 spaces
    expect(expandTabs('\t\thi')).toBe('    hi');
  });

  it('handles tab after even-width text', () => {
    // 'ab' is 2 chars, tab at col 2 → 2 spaces to reach col 4
    expect(expandTabs('ab\tc')).toBe('ab  c');
  });

  it('handles multiline strings', () => {
    expect(expandTabs('a\tb\n\tc')).toBe('a b\n  c');
  });

  it('respects custom tab width', () => {
    expect(expandTabs('\thi', 4)).toBe('    hi');
    expect(expandTabs('a\tb', 4)).toBe('a   b');
  });

  it('returns empty string unchanged', () => {
    expect(expandTabs('')).toBe('');
  });

  it('handles lines without tabs in multiline string', () => {
    expect(expandTabs('no tabs\n\thas tab')).toBe('no tabs\n  has tab');
  });
});
