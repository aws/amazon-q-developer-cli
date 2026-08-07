/**
 * Visual-width-aware string utilities.
 *
 * Terminal columns ≠ `String.length`. Multi-codepoint emoji, CJK ideographs,
 * and combining characters all break the assumption that one JS char = one
 * column. These helpers use twinki's `visibleWidth` (backed by `string-width`)
 * and `Intl.Segmenter` to measure and truncate strings correctly.
 */
import { AnsiCodeTracker, visibleWidth } from 'twinki';

export { AnsiCodeTracker, visibleWidth };

const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

export function maxVisibleWidth(lines: Iterable<string>): number {
  let max = 0;
  for (const line of lines) max = Math.max(max, visibleWidth(line));
  return max;
}

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

/**
 * Pad a string on the left with spaces to exactly `targetCols` visible columns
 * (right-align). If the string is already wider, returns it unchanged.
 */
export function padToWidthRight(s: string, targetCols: number): string {
  const w = visibleWidth(s);
  if (w >= targetCols) return s;
  return ' '.repeat(targetCols - w) + s;
}

/**
 * Cheap per-character zero-width check (a fast path avoiding `visibleWidth`'s
 * Map lookup). INVARIANT: must cover the same ranges `visibleWidth` treats as
 * zero-width so wrap math agrees with measurement.
 */
export function isZeroWidthChar(ch: string): boolean {
  if (ch.length === 0) return true;
  const cp = ch.charCodeAt(0);
  if (cp < 0x300) return false;
  if (cp >= 0x300 && cp <= 0x36f) return true; // combining diacritical marks
  if (cp >= 0x200b && cp <= 0x200f) return true; // ZW space, joiner, non-joiner, marks
  if (cp === 0xfeff) return true; // ZW no-break space
  if (cp >= 0x1ab0 && cp <= 0x1aff) return true; // combining marks extended
  if (cp >= 0x1dc0 && cp <= 0x1dff) return true; // combining marks supplement
  if (cp >= 0x20d0 && cp <= 0x20ff) return true; // combining marks for symbols
  if (cp >= 0xfe00 && cp <= 0xfe0f) return true; // variation selectors
  if (cp >= 0xfe20 && cp <= 0xfe2f) return true; // combining half marks
  return false;
}

/**
 * Break `s` into chunks where the first chunk fits in `firstWidth` columns and
 * subsequent chunks fit in `restWidth`. Prefers breaking at whitespace; falls
 * back to hard-cut on long unbreakable runs (URLs, ids, file paths).
 *
 * INVARIANT: single forward pass (O(n)). The earlier shape measured the full
 * remaining tail each iteration (O(n²)) and froze on 100K unbreakable runs.
 */
export function wrapAtWords(
  s: string,
  firstWidth: number,
  restWidth: number
): string[] {
  if (!s) return [''];
  const out: string[] = [];
  // chunkStart..i is the chunk-in-progress; after each emit, advance past the
  // cut and skip leading whitespace. Iterate by code point so emoji stay whole.
  let chunkStart = 0;
  let i = 0;
  let cumWidth = 0;
  let lastSpace = -1;
  let width = firstWidth;
  const len = s.length;
  while (i < len) {
    const cp = s.codePointAt(i)!;
    const charLen = cp > 0xffff ? 2 : 1;
    const ch = s.slice(i, i + charLen);
    const w = isZeroWidthChar(ch) ? 0 : visibleWidth(ch);
    if (cumWidth + w > width) {
      // Choose the cut point: prefer the last whitespace inside this chunk
      // (so we break on a word boundary). If none exists, hard-cut at `i` —
      // and if that would emit an empty chunk (first char already over
      // budget, e.g. width=0), advance by one code point to make forward
      // progress without splitting a surrogate pair.
      let cut = lastSpace > chunkStart ? lastSpace : i;
      if (cut <= chunkStart) cut = chunkStart + charLen;
      out.push(s.slice(chunkStart, cut).trimEnd());
      // Skip leading whitespace so the next chunk doesn't start with a
      // dangling space — matches the original `replace(/^[\s]+/, '')`.
      let next = cut;
      while (next < len) {
        const c = s[next]!;
        if (c !== ' ' && c !== '\t' && c !== '\n' && c !== '\r') break;
        next++;
      }
      chunkStart = next;
      i = next;
      cumWidth = 0;
      lastSpace = -1;
      width = restWidth;
      continue;
    }
    cumWidth += w;
    if (ch === ' ' || ch === '\t') lastSpace = i;
    i += charLen;
  }
  if (chunkStart < len) {
    const tail = s.slice(chunkStart);
    if (tail.length > 0) out.push(tail);
  } else if (out.length === 0) {
    out.push('');
  }
  return out;
}
