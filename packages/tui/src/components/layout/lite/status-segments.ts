import { visibleWidth } from '../../../utils/text-width.js';

/**
 * Greedily pack pre-colored status-line segments into width-bounded lines.
 *
 * The lite status footer (agent · model · effort · ctx · branch · goal) is a
 * single logical row that must collapse onto additional lines as it grows past
 * the terminal width — the modern TUI gets this for free from `ContextBar`'s
 * flex wrap, but lite renders a flat string row, so we wrap it ourselves.
 *
 * Each segment is already styled (carries ANSI), so widths are measured with
 * `visibleWidth` (terminal columns, ANSI-stripped). Segments are joined with
 * `separator` within a line; a segment that would push the line past `maxCols`
 * starts a new line instead. A single segment wider than `maxCols` gets its own
 * line and is left for the terminal to soft-wrap — clipping it would drop real
 * information (a long branch name, the goal iteration).
 *
 * Empty/whitespace-only segments are skipped so callers can pass conditional
 * slots (`effort && …`, `goal && …`) without pre-filtering.
 *
 * Pure + width-only so the wrap behavior is unit-tested without a renderer.
 */
export function packStatusSegments(
  segments: string[],
  maxCols: number,
  separator: string
): string[] {
  const sepWidth = visibleWidth(separator);
  const lines: string[] = [];
  let current = '';
  let currentWidth = 0;
  for (const seg of segments) {
    if (!seg) continue;
    const segWidth = visibleWidth(seg);
    if (current === '') {
      current = seg;
      currentWidth = segWidth;
      continue;
    }
    if (currentWidth + sepWidth + segWidth > maxCols) {
      lines.push(current);
      current = seg;
      currentWidth = segWidth;
    } else {
      current += separator + seg;
      currentWidth += sepWidth + segWidth;
    }
  }
  if (current !== '') lines.push(current);
  return lines;
}
