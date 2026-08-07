import { describe, it, expect } from 'bun:test';
import {
  constrainColumnWidths,
  wrapCellText,
  padCell,
  shouldStackTable,
  formatStackedTable,
} from '../table-layout.js';
import { visibleWidth } from '../text-width.js';

const len = (s: string) => s.length;
const ESC = String.fromCharCode(0x1b);
const BEL = String.fromCharCode(0x07);
const ansiSgrRe = new RegExp(`${ESC}\\[[0-9;]*m`, 'g');
const ansiSgrPrefixRe = new RegExp(`^${ESC}\\[[0-9;]*m`);
const stripAnsi = (s: string) => s.replace(ansiSgrRe, '');

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

  it('keeps ANSI escapes intact when hard-breaking styled cells', () => {
    const styled = `\x1b[36m${'p'.repeat(24)}\x1b[39m`;
    const chunks = wrapCellText(styled, 14, visibleWidth);
    expect(chunks.map((c) => visibleWidth(c))).toEqual([14, 10]);
    expect(chunks.map(stripAnsi).join('')).toBe('p'.repeat(24));
    for (const chunk of chunks) {
      let idx = chunk.indexOf('\x1b');
      while (idx !== -1) {
        expect(ansiSgrPrefixRe.test(chunk.slice(idx))).toBe(true);
        idx = chunk.indexOf('\x1b', idx + 1);
      }
    }
  });

  it('does not split OSC 8 hyperlinks in styled cells', () => {
    const link = `\x1b]8;;https://example.com/very/long/path\x07texttexttext\x1b]8;;\x07`;
    expect(visibleWidth(link)).toBe(12);
    const chunks = wrapCellText(link, 10, visibleWidth);
    const stripOsc = (s: string) =>
      s.replace(
        new RegExp(
          `${ESC}\\][0-9]*;[^${BEL}${ESC}]*(?:${BEL}|${ESC}\\\\)`,
          'g'
        ),
        ''
      );
    expect(chunks.map((c) => stripAnsi(stripOsc(c))).join('')).toBe(
      'texttexttext'
    );
    for (const chunk of chunks) {
      expect(/example\.com|https/.test(stripOsc(stripAnsi(chunk)))).toBe(false);
    }
  });
});

describe('table stacking', () => {
  it('detects when columns are too narrow for readable table layout', () => {
    expect(shouldStackTable([20, 20, 20], 28)).toBe(true);
    expect(shouldStackTable([5, 5], 80)).toBe(false);
  });

  it('formats stacked rows and skips all-empty rows without double blanks', () => {
    const lines = formatStackedTable(
      ['Command', 'Description'],
      [
        ['ls', 'list files'],
        ['', ''],
        ['pwd', 'print dir'],
      ],
      (s) => s,
      (s) => `**${s}**`
    );
    expect(lines).toEqual([
      '**Command**: ls',
      '**Description**: list files',
      '',
      '**Command**: pwd',
      '**Description**: print dir',
    ]);
  });

  it('keeps headers visible when a narrow table has no data rows', () => {
    expect(
      formatStackedTable(
        ['Command', 'Description'],
        [],
        (s) => `_${s}_`,
        (s) => `**${s}**`
      )
    ).toEqual(['**_Command_**', '**_Description_**']);
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
