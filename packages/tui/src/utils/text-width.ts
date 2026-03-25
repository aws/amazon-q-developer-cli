/**
 * Visual-width-aware string utilities.
 *
 * Terminal columns ≠ `String.length`. Multi-codepoint emoji, CJK ideographs,
 * and combining characters all break the assumption that one JS char = one
 * column. These helpers use twinki's `visibleWidth` (backed by `string-width`)
 * and `Intl.Segmenter` to measure and truncate strings correctly.
 */
import { visibleWidth } from 'twinki';

export { visibleWidth };

const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/**
 * Truncate a string to fit within `maxCols` visible terminal columns.
 * Uses grapheme segmentation so multi-codepoint emoji are never split.
 *
 * @param s       The string to truncate
 * @param maxCols Maximum visible width in terminal columns
 * @param ellipsis Character(s) appended when truncation occurs (default '…')
 * @returns The (possibly truncated) string
 */
export function truncateToWidth(
  s: string,
  maxCols: number,
  ellipsis = '…'
): string {
  const w = visibleWidth(s);
  if (w <= maxCols) return s;
  const ellipsisW = visibleWidth(ellipsis);
  const target = maxCols - ellipsisW;
  if (target <= 0) return ellipsis.slice(0, maxCols);
  let used = 0;
  let result = '';
  for (const { segment } of segmenter.segment(s)) {
    const gw = visibleWidth(segment);
    if (used + gw > target) break;
    result += segment;
    used += gw;
  }
  return result + ellipsis;
}

/**
 * Pad a string with spaces to exactly `targetCols` visible columns.
 * If the string is already wider, returns it unchanged.
 */
export function padToWidth(s: string, targetCols: number): string {
  const w = visibleWidth(s);
  if (w >= targetCols) return s;
  return s + ' '.repeat(targetCols - w);
}
