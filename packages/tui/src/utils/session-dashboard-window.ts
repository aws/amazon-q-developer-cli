/**
 * Scroll-window packing for the session dashboard list.
 *
 * The window over the nav items is grown item-by-item around the cursor
 * against a real line budget: every item pays its true rendered cost — one
 * line per row, two extra for the group band above a group's first
 * in-window row, plus any anchor-only extra (the cursor row's match
 * snippet). Growing alternately up and down keeps the anchor centered; a
 * side that cannot afford its next item closes. The result can never
 * overflow the budget, and backfills the opposite side at list edges so
 * the screen stays full.
 */

export interface ScrollWindowInput {
  /** Total nav items (rows + expanders). */
  itemCount: number;
  /** Group ordinal per nav index (a group's band renders with its first
   *  in-window row). */
  navGroupIdx: readonly number[];
  /** Anchor nav index — kept in view, roughly centered. */
  cursor: number;
  /** Rendered lines the window may spend. */
  lineBudget: number;
  /** Extra lines the anchor itself costs (e.g. its match snippet). */
  anchorExtraLines?: number;
}

export interface ScrollWindow {
  startIdx: number;
  endIdx: number;
}

export function packScrollWindow(input: ScrollWindowInput): ScrollWindow {
  const { itemCount, navGroupIdx, lineBudget } = input;
  if (itemCount === 0) return { startIdx: 0, endIdx: 0 };
  const anchor = Math.min(Math.max(input.cursor, 0), itemCount - 1);
  // The anchor always renders: its row (1) + its group's band (2).
  let start = anchor;
  let end = anchor + 1;
  let used = 3 + (input.anchorExtraLines ?? 0);
  let upOpen = start > 0;
  let downOpen = end < itemCount;
  while ((upOpen || downOpen) && used < lineBudget) {
    // Extend whichever side keeps the anchor closer to the middle.
    const goDown = downOpen && (!upOpen || end - anchor <= anchor - start);
    const idx = goDown ? end : start - 1;
    const neighbor = goDown ? end - 1 : start;
    const cost = 1 + (navGroupIdx[idx] !== navGroupIdx[neighbor] ? 2 : 0);
    if (used + cost > lineBudget) {
      if (goDown) downOpen = false;
      else upOpen = false;
      continue;
    }
    used += cost;
    if (goDown) {
      end++;
      downOpen = end < itemCount;
    } else {
      start--;
      upOpen = start > 0;
    }
  }
  return { startIdx: start, endIdx: end };
}

/** Rendered line count for a window — the packer's own cost model, exposed
 *  so tests can assert a packed window never exceeds its budget. */
export function windowLineCount(
  navGroupIdx: readonly number[],
  startIdx: number,
  endIdx: number,
  anchorExtraLines = 0
): number {
  let lines = anchorExtraLines;
  for (let i = startIdx; i < endIdx; i++) {
    lines += 1;
    if (i === startIdx || navGroupIdx[i] !== navGroupIdx[i - 1]) lines += 2;
  }
  return lines;
}
