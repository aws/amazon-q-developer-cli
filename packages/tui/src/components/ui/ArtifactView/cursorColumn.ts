import { CURSOR_MARKER } from './../../../renderer.js';

/**
 * Leading cursor column for a spec-summary row.
 *
 * `CURSOR_MARKER` is a zero-width APC sentinel: twinki records the row it sits
 * on (to anchor scrolling on the selected item) and then strips it, so it
 * contributes no visible width. We therefore always follow it with a single
 * visible space, and emit only that space when unselected — so selected and
 * unselected rows are exactly one column wide either way. Emitting the bare
 * marker for selected and a space for unselected (the previous behavior) made
 * the selected row one column narrower, shifting its text left as the cursor
 * moved.
 */
export function cursorColumn(selected: boolean): string {
  return (selected ? CURSOR_MARKER : '') + ' ';
}
