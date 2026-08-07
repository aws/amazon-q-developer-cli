/**
 * Tests for the review surface's row layout.
 *
 * The invariant that matters is width in terminal columns: a row wider than the
 * terminal is folded a second time by the terminal, and that fold carries no
 * indent, so text lands against the left edge and the document stops looking
 * like a document. `rowColumns` measures a row exactly as the surface draws it,
 * so these assertions hold against what the user sees rather than against a
 * second copy of the drawing rules.
 */
import { describe, it, expect } from 'bun:test';
import { layoutRows, rowColumns } from '../layout';
import type { ReviewAction } from '../review-actions';

const BULLET = '\u25c7 ';

const comment = (
  lineIndex: number,
  id: string,
  body: string
): ReviewAction => ({
  kind: 'comment',
  id,
  anchor: {
    range: { start: lineIndex, end: lineIndex },
    heading: null,
    snippet: '',
  },
  body,
});

const LONG_EN =
  '6. IF the User attempts to start the Web_Clock in Count_Down_Mode with a duration of 00:00:00, THEN THE Web_Clock SHALL not start counting and SHALL display an error message indicating that a duration greater than zero is required';
const LONG_CJK =
  '1. 当用户选择倒计时模式时，时钟应当显示持续时间输入框，并允许用户设置目标时间，同时在倒计时归零后停止递减并保持空闲状态。';
const MIXED = `2. WHEN 用户 selects Stopwatch_Mode 时，THE Clock_App SHALL 从零开始计时 🕐 并持续更新显示。`;

describe('layoutRows', () => {
  for (const [name, line] of [
    ['ascii', LONG_EN],
    ['CJK, whose characters take two columns each', LONG_CJK],
    ['mixed scripts and emoji', MIXED],
  ] as const) {
    it(`keeps every row inside the terminal for ${name}`, () => {
      for (const width of [40, 60, 80, 107, 120]) {
        const rows = layoutRows(
          ['## Requirements', line, 'short'],
          [comment(1, 'c1', line)],
          width,
          BULLET
        );
        for (const row of rows) {
          expect(rowColumns(row)).toBeLessThanOrEqual(width);
        }
      }
    });
  }

  it('loses no text to wrapping', () => {
    const rows = layoutRows([LONG_CJK], [], 50, BULLET);
    const rejoined = rows
      .map((row) => row.text)
      .join('')
      .replace(/\s+/g, '');
    expect(rejoined).toBe(LONG_CJK.replace(/\s+/g, ''));
  });

  it('lines a wrapped list item up with the text past its marker', () => {
    for (const [line, marker] of [
      [`3. ${LONG_EN}`, '3. '],
      [`- ${LONG_EN}`, '- '],
      [`  * ${LONG_EN}`, '  * '],
      [`10) ${LONG_EN}`, '10) '],
    ] as const) {
      const rows = layoutRows([line], [], 64, BULLET).filter(
        (row) => row.kind === 'line'
      );
      expect(rows.length).toBeGreaterThan(1);
      expect(rows[1]!.indent.length).toBe(
        rows[0]!.indent.length + marker.length
      );
    }
  });

  it('lines wrapped prose up with the first row, with no marker to hang from', () => {
    const prose = LONG_EN.replace(/^6\. /, '');
    const rows = layoutRows([prose], [], 64, BULLET);
    expect(rows.length).toBeGreaterThan(1);
    expect(rows[1]!.indent.length).toBe(rows[0]!.indent.length);
  });

  it("keeps a fenced block's indentation, marker-like lines included", () => {
    const rows = layoutRows(
      ['```yaml', `    - ${LONG_EN}`, '```'],
      [],
      64,
      BULLET
    );
    const wrapped = rows.filter((row) => row.lineIndex === 1);
    expect(wrapped.length).toBeGreaterThan(1);
    // Inside a fence the dash is content, not a marker to hang from.
    expect(wrapped[1]!.indent.length).toBe(
      wrapped[0]!.indent.length + '    '.length
    );
  });

  it('draws the bullet once, on the comment row that starts it', () => {
    const rows = layoutRows([LONG_EN], [comment(0, 'c1', LONG_EN)], 60, BULLET);
    const commentRows = rows.filter((row) => row.kind === 'comment');
    expect(commentRows.length).toBeGreaterThan(1);
    expect(commentRows[0]!.bullet).toBe(BULLET);
    expect(commentRows.slice(1).every((row) => row.bullet === '')).toBe(true);
    // Wrapped comment text still lines up under the first row's text.
    expect(commentRows[1]!.indent.length).toBe(
      commentRows[0]!.indent.length + BULLET.length
    );
  });

  it('accounts for a wider bullet, as the ASCII glyph set uses', () => {
    const rows = layoutRows(
      [LONG_EN],
      [comment(0, 'c1', LONG_EN)],
      60,
      'COMMENT: '
    );
    for (const row of rows) {
      expect(rowColumns(row)).toBeLessThanOrEqual(60);
    }
  });

  it('places a comment after the last row of the line it annotates', () => {
    const rows = layoutRows(
      ['first', LONG_EN, 'third'],
      [comment(1, 'c1', 'why?')],
      60,
      BULLET
    );
    const at = rows.findIndex((row) => row.kind === 'comment');
    expect(rows[at - 1]).toMatchObject({ kind: 'line', lineIndex: 1 });
    expect(rows[at + 1]).toMatchObject({ kind: 'line', lineIndex: 2 });
  });

  it('keeps a row per line where lines are empty', () => {
    const rows = layoutRows(['a', '', 'b'], [], 20, BULLET);
    expect(rows.map((row) => row.lineIndex)).toEqual([0, 1, 2]);
  });
});
