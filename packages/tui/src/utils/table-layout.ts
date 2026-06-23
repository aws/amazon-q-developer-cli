/** Padding per column: ` ` + content + ` ` + border char, plus leading border. */
const BORDER_OVERHEAD_PER_COL = 3;
const BORDER_OVERHEAD_FIXED = 1;
/** Safety margin so the table doesn't touch the terminal edge. */
const TABLE_MARGIN = 4;
const MIN_COL_WIDTH = 3;
const READABLE_MIN_COL_WIDTH = 8;

export type Alignment = 'left' | 'right' | 'center';

// eslint-disable-next-line no-control-regex
const ANSI_ESC_RE =
  /^(?:\x1b\][0-9]*;[^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;]*m)/;

export function shouldStackTable(
  colWidths: number[],
  termWidth: number
): boolean {
  if (termWidth <= 0 || colWidths.length === 0) return false;
  const overhead =
    BORDER_OVERHEAD_PER_COL * colWidths.length + BORDER_OVERHEAD_FIXED;
  const wanted =
    colWidths.reduce((a, w) => a + Math.min(w, READABLE_MIN_COL_WIDTH), 0) +
    overhead +
    TABLE_MARGIN;
  return wanted > termWidth;
}

export function formatStackedTable(
  headers: string[],
  rows: string[][],
  renderInline: (s: string) => string,
  styleLabel: (s: string) => string
): string[] {
  const out: string[] = [];
  for (const row of rows) {
    const rowLines: string[] = [];
    for (let ci = 0; ci < headers.length; ci++) {
      const cell = row[ci];
      if (cell == null || cell === '') continue;
      rowLines.push(`${styleLabel(headers[ci] ?? '')}: ${renderInline(cell)}`);
    }
    if (rowLines.length === 0) continue;
    if (out.length > 0) out.push('');
    out.push(...rowLines);
  }
  return out;
}

/**
 * Shrink column widths so the table fits within `termWidth`.
 * Mutates `colWidths` in place. No-ops when columns already fit
 * or when the terminal is too narrow to fit even the border chrome.
 *
 * Algorithm:
 * 1. Compute available content width by subtracting border overhead
 *    (3 chars per column + 1) and a safety margin from `termWidth`.
 * 2. Calculate a fair share per column (available / column count).
 * 3. "Fix" every column that is already at or below the fair share -
 *    these keep their natural width. Subtract them from the budget.
 * 4. Distribute the remaining budget to unfixed (wide) columns
 *    proportionally to their original widths, with a floor of
 *    `min(10, budget / unfixedCount)` (at least MIN_COL_WIDTH).
 *
 * Note: the floor means the total may still exceed `termWidth` on very
 * narrow terminals with many columns. This is intentional - columns
 * below 3 chars are unusable.
 *
 * @param colWidths - Natural column widths to constrain. Modified in place.
 * @param termWidth - Terminal width in visible columns.
 */
export function constrainColumnWidths(
  colWidths: number[],
  termWidth: number
): void {
  const overhead =
    BORDER_OVERHEAD_PER_COL * colWidths.length + BORDER_OVERHEAD_FIXED;
  const maxContent = termWidth - overhead - TABLE_MARGIN;
  const total = colWidths.reduce((a, b) => a + b, 0);
  if (total <= maxContent || maxContent <= 0) return;

  const fairShare = Math.floor(maxContent / colWidths.length);
  let remaining = maxContent;
  const fixed: boolean[] = colWidths.map(() => false);

  // Step 3: fix narrow columns at their natural width
  for (let i = 0; i < colWidths.length; i++) {
    if (colWidths[i]! <= fairShare) {
      fixed[i] = true;
      remaining -= colWidths[i]!;
    }
  }

  const unfixedCount = fixed.filter((f) => !f).length;
  const unfixedTotal = colWidths.reduce((a, w, i) => a + (fixed[i] ? 0 : w), 0);
  if (unfixedTotal <= 0 || unfixedCount === 0) return;

  // Step 4: distribute remaining budget proportionally to wide columns
  const minWidth = Math.max(
    MIN_COL_WIDTH,
    Math.min(10, Math.floor(remaining / unfixedCount))
  );

  for (let i = 0; i < colWidths.length; i++) {
    if (!fixed[i]) {
      colWidths[i] = Math.max(
        minWidth,
        Math.floor((colWidths[i]! / unfixedTotal) * remaining)
      );
    }
  }
}

/**
 * Word-wrap `text` to fit within `maxWidth` visible columns.
 * Splits on spaces first; words that still exceed `maxWidth` are
 * hard-broken character by character.
 *
 * Limitation: wrapping operates on raw text (before inline markdown
 * rendering). If a markdown span like `**bold text**` wraps across
 * lines, each fragment is rendered independently and the delimiters
 * won't match - the user sees raw syntax instead of styled text.
 * This is an acceptable tradeoff: rendering first would require
 * splitting ANSI escape sequences, which is worse.
 *
 * @param text - Raw cell text to wrap.
 * @param maxWidth - Maximum visible width per line.
 * @param measureWidth - Returns the visible terminal width of a string
 *   (e.g. after rendering inline markdown and stripping ANSI).
 * @returns Array of wrapped lines (at least one element, possibly `['']`).
 */
export function wrapCellText(
  text: string,
  maxWidth: number,
  measureWidth: (s: string) => number
): string[] {
  if (measureWidth(text) <= maxWidth) return [text];

  const words = text.split(' ');
  const lines: string[] = [];
  let line = '';

  for (const word of words) {
    if (measureWidth(word) > maxWidth) {
      if (line) {
        lines.push(line);
      }
      let chunk = '';
      let chunkWidth = 0;
      let j = 0;
      while (j < word.length) {
        if (word.charCodeAt(j) === 0x1b) {
          const esc = ANSI_ESC_RE.exec(word.slice(j));
          if (esc && esc.index === 0) {
            chunk += esc[0];
            j += esc[0].length;
            continue;
          }
        }
        const cp = word.codePointAt(j)!;
        const charLen = cp > 0xffff ? 2 : 1;
        const ch = word.slice(j, j + charLen);
        const cw = measureWidth(ch);
        if (chunk !== '' && chunkWidth + cw > maxWidth) {
          lines.push(chunk);
          chunk = '';
          chunkWidth = 0;
        }
        chunk += ch;
        chunkWidth += cw;
        j += charLen;
      }
      line = chunk;
      continue;
    }

    const candidate = line ? `${line} ${word}` : word;
    if (measureWidth(candidate) <= maxWidth) {
      line = candidate;
    } else {
      if (line) lines.push(line);
      line = word;
    }
  }

  if (line) lines.push(line);
  return lines.length > 0 ? lines : [''];
}

/**
 * Pad `text` with spaces to `width` visible columns, respecting alignment.
 *
 * @param text - Already-styled text to pad.
 * @param width - Target width in visible columns.
 * @param align - Column alignment.
 * @param measureWidth - Returns the visible terminal width of `text`.
 * @returns `text` padded with spaces to exactly `width` columns,
 *   or unchanged if already wider.
 */
export function padCell(
  text: string,
  width: number,
  align: Alignment,
  measureWidth: (s: string) => number
): string {
  const pad = width - measureWidth(text);
  if (pad <= 0) return text;
  if (align === 'right') return ' '.repeat(pad) + text;
  if (align === 'center') {
    const left = Math.floor(pad / 2);
    return ' '.repeat(left) + text + ' '.repeat(pad - left);
  }
  return text + ' '.repeat(pad);
}
