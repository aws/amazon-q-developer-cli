import { describe, it, expect } from 'bun:test';
import { CURSOR_MARKER } from './../../../renderer.js';
import { cursorColumn } from './cursorColumn.js';

describe('cursorColumn', () => {
  it('unselected is a single visible space', () => {
    expect(cursorColumn(false)).toBe(' ');
  });

  it('selected differs only by the zero-width cursor marker (no width shift)', () => {
    const selected = cursorColumn(true);
    // The marker is the only addition; its removal must yield the exact
    // unselected column, proving identical visible width across selection.
    expect(selected.includes(CURSOR_MARKER)).toBe(true);
    expect(selected.replace(CURSOR_MARKER, '')).toBe(cursorColumn(false));
  });
});
