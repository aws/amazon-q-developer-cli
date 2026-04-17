import { describe, it, expect } from 'bun:test';
import {
  constrainColumnWidths,
  wrapCellText,
  padCell,
} from '../table-layout.js';

const len = (s: string) => s.length;

describe('constrainColumnWidths', () => {
  it('does nothing when columns fit within terminal width', () => {
    const widths = [10, 10, 10];
    constrainColumnWidths(widths, 120);
    expect(widths).toEqual([10, 10, 10]);
  });

  it('shrinks wide columns proportionally', () => {
    const widths = [5, 50, 50];
    // overhead = 3*3+1 = 10, margin = 4, available = 40-14 = 26
    constrainColumnWidths(widths, 40);
    // col 0 (5) is under fair share (26/3=8), stays fixed. remaining = 21
    // cols 1,2 split 21 proportionally (50/50 = equal)
    expect(widths[0]).toBe(5);
    expect(widths[1]).toBe(widths[2]);
    expect(widths[1]! + widths[2]! + widths[0]!).toBeLessThanOrEqual(26);
  });

  it('preserves narrow columns at natural width', () => {
    const widths = [3, 3, 100];
    constrainColumnWidths(widths, 40);
    expect(widths[0]).toBe(3);
    expect(widths[1]).toBe(3);
    expect(widths[2]).toBeLessThan(100);
  });

  it('clamps minimum width to avoid overflow on narrow terminals', () => {
    const widths = [50, 50, 50];
    // overhead = 10, margin = 4, available = 20-14 = 6
    // min = max(3, min(10, floor(6/3))) = max(3, 2) = 3
    constrainColumnWidths(widths, 20);
    for (const w of widths) {
      expect(w).toBeGreaterThanOrEqual(3);
    }
  });

  it('does nothing when maxContent is zero or negative', () => {
    const widths = [10, 10];
    const original = [...widths];
    constrainColumnWidths(widths, 5);
    expect(widths).toEqual(original);
  });

  it('handles single column', () => {
    const widths = [100];
    constrainColumnWidths(widths, 30);
    // overhead = 4, margin = 4, available = 22
    expect(widths[0]).toBeLessThanOrEqual(22);
  });
});

describe('wrapCellText', () => {
  it('returns single-element array when text fits', () => {
    expect(wrapCellText('hello', 10, len)).toEqual(['hello']);
  });

  it('wraps on word boundaries', () => {
    expect(wrapCellText('hello world foo', 11, len)).toEqual([
      'hello world',
      'foo',
    ]);
  });

  it('hard-breaks words exceeding column width', () => {
    const result = wrapCellText('abcdefghij', 4, len);
    expect(result).toEqual(['abcd', 'efgh', 'ij']);
  });

  it('handles empty string', () => {
    expect(wrapCellText('', 10, len)).toEqual(['']);
  });

  it('handles single character column width', () => {
    const result = wrapCellText('abc', 1, len);
    expect(result).toEqual(['a', 'b', 'c']);
  });

  it('wraps mixed short and long words', () => {
    const result = wrapCellText('hi superlongword ok', 6, len);
    expect(result[0]).toBe('hi');
    expect(result[1]).toBe('superl');
    expect(result[2]).toBe('ongwor');
    expect(result[3]).toBe('d ok');
  });

  it('uses custom measure function', () => {
    // Simulate a measure that counts each char as 2 columns (e.g. CJK)
    const doubleMeasure = (s: string) => s.length * 2;
    const result = wrapCellText('ab cd', 6, doubleMeasure);
    // "ab cd" = 10 cols, "ab" = 4 cols, "cd" = 4 cols
    expect(result).toEqual(['ab', 'cd']);
  });
});

describe('padCell', () => {
  it('pads left-aligned text', () => {
    expect(padCell('hi', 5, 'left', len)).toBe('hi   ');
  });

  it('pads right-aligned text', () => {
    expect(padCell('hi', 5, 'right', len)).toBe('   hi');
  });

  it('pads center-aligned text', () => {
    expect(padCell('hi', 6, 'center', len)).toBe('  hi  ');
  });

  it('returns text unchanged when wider than target', () => {
    expect(padCell('hello', 3, 'left', len)).toBe('hello');
  });

  it('handles exact width', () => {
    expect(padCell('abc', 3, 'left', len)).toBe('abc');
  });
});
