import { describe, it, expect } from 'bun:test';
import { packScrollWindow, windowLineCount } from '../session-dashboard-window';

/** navGroupIdx for groups of the given sizes: [2,3] → [0,0,1,1,1]. */
function groups(...sizes: number[]): number[] {
  const idx: number[] = [];
  sizes.forEach((n, g) => {
    for (let i = 0; i < n; i++) idx.push(g);
  });
  return idx;
}

function pack(
  navGroupIdx: number[],
  cursor: number,
  lineBudget: number,
  anchorExtraLines = 0
) {
  return packScrollWindow({
    itemCount: navGroupIdx.length,
    navGroupIdx,
    cursor,
    lineBudget,
    anchorExtraLines,
  });
}

describe('packScrollWindow', () => {
  it('returns an empty window for an empty list', () => {
    expect(pack([], 0, 20)).toEqual({ startIdx: 0, endIdx: 0 });
  });

  it('always renders the anchor, even when the budget is tiny', () => {
    const g = groups(10);
    const w = pack(g, 5, 1);
    expect(w.startIdx).toBe(5);
    expect(w.endIdx).toBe(6);
  });

  it('never exceeds the budget, whatever the group density', () => {
    // Alternating 1-row groups (worst band density), one huge group, mixed.
    const shapes = [
      groups(...Array.from({ length: 40 }, () => 1)),
      groups(80),
      groups(1, 7, 2, 30, 1, 1, 5),
    ];
    for (const g of shapes) {
      for (const budget of [4, 7, 12, 25, 60]) {
        for (let cursor = 0; cursor < g.length; cursor += 7) {
          const w = pack(g, cursor, budget);
          const lines = windowLineCount(g, w.startIdx, w.endIdx);
          expect(lines).toBeLessThanOrEqual(Math.max(budget, 3));
        }
      }
    }
  });

  it('fills the budget when there are items to show', () => {
    const g = groups(80);
    const w = pack(g, 40, 20);
    // One group: band 2 + one line per row → 18 rows.
    expect(w.endIdx - w.startIdx).toBe(18);
    expect(windowLineCount(g, w.startIdx, w.endIdx)).toBe(20);
  });

  it('keeps the anchor centered mid-list', () => {
    const g = groups(100);
    const w = pack(g, 50, 21);
    expect(50 - w.startIdx).toBeGreaterThanOrEqual(8);
    expect(w.endIdx - 1 - 50).toBeGreaterThanOrEqual(8);
  });

  it('backfills upward at the list end so the screen stays full', () => {
    const g = groups(50);
    const w = pack(g, 49, 20);
    expect(w.endIdx).toBe(50);
    // 18 rows fit (band 2 + 18) — all above the anchor.
    expect(w.endIdx - w.startIdx).toBe(18);
  });

  it('backfills downward at the list start', () => {
    const g = groups(50);
    const w = pack(g, 0, 20);
    expect(w.startIdx).toBe(0);
    expect(w.endIdx - w.startIdx).toBe(18);
  });

  it('charges a band when growth crosses into another group', () => {
    // Two groups of 10; window from the middle of the first.
    const g = groups(10, 10);
    // Budget for anchor(3) + 9 same-group rows = 12 exactly: the window
    // must not cross the group boundary (crossing would cost 3).
    const w = pack(g, 5, 12);
    const rows = w.endIdx - w.startIdx;
    expect(rows).toBe(10);
    expect(w.startIdx).toBe(0);
    expect(w.endIdx).toBe(10);
  });

  it('shows fewer rows when many tiny groups pay band costs', () => {
    const many = pack(groups(...Array.from({ length: 30 }, () => 1)), 15, 30);
    const single = pack(groups(30), 15, 30);
    const manyRows = many.endIdx - many.startIdx;
    const singleRows = single.endIdx - single.startIdx;
    expect(manyRows).toBeLessThan(singleRows);
    // Every item costs 3 (row + band) → 30/3 = 10 rows.
    expect(manyRows).toBe(10);
  });

  it('regression: many tiny groups must not collapse the window to 3 rows', () => {
    // 40 groups of 2 (the all+empty shape): a 27-line budget must still
    // produce a healthy window, not 3 rows.
    const g = groups(...Array.from({ length: 40 }, () => 2));
    const w = pack(g, 20, 27);
    expect(w.endIdx - w.startIdx).toBeGreaterThanOrEqual(13);
  });

  it('charges the anchor extra lines (search snippet)', () => {
    const g = groups(30);
    const plain = pack(g, 15, 12);
    const snip = pack(g, 15, 12, 1);
    expect(plain.endIdx - plain.startIdx - (snip.endIdx - snip.startIdx)).toBe(
      1
    );
  });

  it('clamps an out-of-range cursor', () => {
    const g = groups(10);
    const w = pack(g, 99, 10);
    expect(w.endIdx).toBe(10);
    expect(w.startIdx).toBeGreaterThanOrEqual(0);
  });
});
