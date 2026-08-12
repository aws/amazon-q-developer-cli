import { describe, expect, it } from 'bun:test';
import { clauseRowText } from '../clauseRow.js';

const LONG =
  'WHEN the countdown timer reaches zero (this.seconds becomes 0) THEN the system continues decrementing this.seconds to negative values on each tick';

describe('clauseRowText', () => {
  it('marks a clause it had to cut', () => {
    // The renderer clips without a mark, so a row ending mid-word reads as a
    // fault. The ellipsis is what says "this clause has more to it".
    const row = clauseRowText('1.1', LONG, 83);

    expect(row.endsWith('…')).toBe(true);
    expect(row).not.toContain('on each tick');
    expect(LONG.startsWith(row.slice(0, -1))).toBe(true);
  });

  it('leaves a clause that already fits untouched', () => {
    const short = 'WHEN it fits THEN nothing is cut';

    expect(clauseRowText('2.1', short, 83)).toBe(short);
    expect(clauseRowText('2.1', short, 83)).not.toContain('…');
  });

  it('keeps the row inside the columns the panel leaves it', () => {
    // Panel padding plus the row indent take 6; the number and its trailing
    // space take the rest. Measured at the width the panel actually renders at.
    for (const width of [40, 60, 83, 120]) {
      const row = `1.1 ${clauseRowText('1.1', LONG, width)}`;
      expect(row.length, `width ${width}`).toBeLessThanOrEqual(width - 6);
    }
  });

  it('yields nothing rather than overflowing a width with no room', () => {
    // A pane too narrow to hold even one character must not push the row wider
    // than the panel and wrap it onto a second line.
    expect(clauseRowText('1.1', LONG, 10)).toBe('');
    expect(clauseRowText('1.1', LONG, 0)).toBe('');
  });
});
