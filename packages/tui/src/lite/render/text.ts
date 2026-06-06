import chalk from 'chalk';
import { highlight } from 'cli-highlight';
import { visibleWidth } from '../../utils/text-width.js';
import { resolveHighlightLanguage } from '../../utils/highlight-languages.js';

export function resolveLanguageFromPathLite(path?: string): string | undefined {
  if (!path) return undefined;
  const base = path.split('/').pop() ?? path;
  const ext = base.includes('.') ? base.split('.').pop() : undefined;
  return resolveHighlightLanguage(ext);
}

export function highlightLineSafe(code: string, language?: string): string {
  if (!code || !language || language === 'plaintext') return code;
  try {
    // cli-highlight occasionally writes warnings to console.error for
    // tokenizer hiccups; silence them to avoid corrupting the TTY.
    const orig = console.error;
    console.error = () => {};
    try {
      return highlight(code, { language });
    } finally {
      console.error = orig;
    }
  } catch {
    return code;
  }
}

/**
 * Tail-clip a string at `maxChars` of visible width (ignoring ANSI escapes
 * and double-width chars), appending `…`. Used by the output bar's per-row
 * char cap. Naive char-count would mistruncate in the middle of an ANSI
 * sequence and corrupt downstream rendering; visibleWidth is the same
 * helper formatBarBlock uses to wrap.
 */
export function clipVisibleWidth(s: string, maxChars: number): string {
  if (visibleWidth(s) <= maxChars) return s;
  // Walk the string, accumulating chars until we hit the cap. Strip ANSI
  // along the way using the same regex visibleWidth uses internally — we
  // re-apply a single chalk.reset at the end so the next row starts clean
  // even if we cut mid-styled-segment.
  // eslint-disable-next-line no-control-regex
  const ansiRe = /\x1b\[[0-9;]*m/g;
  let out = '';
  let visible = 0;
  let i = 0;
  while (i < s.length && visible < maxChars - 1) {
    ansiRe.lastIndex = i;
    const match = ansiRe.exec(s);
    if (match && match.index === i) {
      out += match[0];
      i = match.index + match[0].length;
      continue;
    }
    out += s[i];
    visible += 1;
    i += 1;
  }
  return out + '…\x1b[0m';
}

export function stripAnsiQuick(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b\[[0-9;]*m/g, '');
}

/**
 * Visible-width-aware soft wrap for already-styled content. Splits on
 * spaces when possible; falls back to a hard cut on long unbreakable runs
 * (URLs, hashes). ANSI escapes are zero-width and survive wrapping —
 * chalk's bold/italic/color sequences pass through untouched.
 *
 * Width 0 disables wrapping (one-line passthrough). Used by the markdown
 * renderer's tests so output is deterministic across terminal sizes.
 */
export function wrapStyled(
  s: string,
  firstWidth: number,
  restWidth: number
): string[] {
  if (!s) return [''];
  if (firstWidth <= 0 && restWidth <= 0) return s.split('\n');
  // Source string may carry hard newlines from `<br>` etc. — wrap each
  // logical line independently so we don't bridge them into one paragraph.
  const out: string[] = [];
  const lines = s.split('\n');
  for (let li = 0; li < lines.length; li++) {
    const isFirstLogical = li === 0;
    const w0 = isFirstLogical ? firstWidth || restWidth : restWidth;
    const wR = restWidth || w0;
    out.push(...wrapAnsiLine(lines[li] ?? '', w0, wR));
  }
  return out;
}

/**
 * Soft-wrap a single (newline-free) ANSI-styled line. Preserves ANSI
 * escapes (zero-width) and falls back to a hard cut on words that exceed
 * the column. Returns at least one row even when input is empty so block
 * separators stay correctly sized.
 *
 * Exported for reuse by `lite/diff.ts` — the diff renderer wraps already-
 * highlighted source so wrapped continuation rows inherit the SGR state
 * that was active mid-token at the wrap boundary (otherwise the bg tint
 * and syntax-highlight color would reset to default at every continuation
 * row's hanging indent).
 */
export function wrapAnsiLine(
  line: string,
  firstWidth: number,
  restWidth: number
): string[] {
  if (!line) return [''];
  const w0 = firstWidth > 0 ? firstWidth : line.length;
  const wR = restWidth > 0 ? restWidth : line.length;
  const out: string[] = [];
  // eslint-disable-next-line no-control-regex
  const ansiRe = /\x1b\[[0-9;]*m/g;
  // Scan once, building character cells with their visible width and the
  // ANSI bytes attached to the previous boundary. lastSpace records the
  // most recent inter-word break so we can soft-wrap on spaces.
  // Iterate by code points so astral chars (emoji at U+1F000+) stay in a
  // single cell — splitting their UTF-16 surrogate pair across rows
  // produces lone surrogates that render as garbled replacement chars.
  // ZWJ clusters (e.g. 👨‍👩‍👧‍👦) still split between codepoints; keeping
  // a full grapheme cluster together would require Intl.Segmenter.
  type Cell = { ansi: string; ch: string; width: number };
  const cells: Cell[] = [];
  let pendingAnsi = '';
  let i = 0;
  while (i < line.length) {
    ansiRe.lastIndex = i;
    const m = ansiRe.exec(line);
    if (m && m.index === i) {
      pendingAnsi += m[0];
      i = m.index + m[0].length;
      continue;
    }
    const cp = line.codePointAt(i)!;
    const charLen = cp > 0xffff ? 2 : 1;
    const ch = line.slice(i, i + charLen);
    cells.push({ ansi: pendingAnsi, ch, width: visibleWidth(ch) });
    pendingAnsi = '';
    i += charLen;
  }

  let curStart = 0;
  let curWidth = 0;
  let lastSpace = -1;
  let activeWidth = w0;
  const flush = (end: number) => {
    let s = '';
    for (let k = curStart; k < end; k++) {
      const c = cells[k]!;
      s += c.ansi + c.ch;
    }
    out.push(s.replace(/\s+$/, ''));
  };
  for (let k = 0; k < cells.length; k++) {
    const c = cells[k]!;
    if (curWidth + c.width > activeWidth) {
      const breakAt = lastSpace > curStart ? lastSpace : k;
      flush(breakAt);
      curStart = breakAt;
      // Skip leading whitespace on the next row, BUT preserve any ANSI
      // escapes attached to the skipped cells. The cell at lastSpace
      // commonly carries a closer for the styled span that ended right
      // before it — `\x1b[22m` after `**bold**`, `\x1b[23m` after
      // `*italic*`, `\x1b[24m\x1b[39m\x1b[2m` between a
      // `chalk.underline.cyan` link label and its dim `(url)` trailer,
      // `\x1b[39m` after a code span `\x1b[36m`, etc. Without preserving
      // these on the surviving cell, the row finishes without closing
      // the style and terminal state stays bold/italic/underline/color
      // forever — bleeding into the rest of the message AND every
      // subsequent message until something else resets terminal state.
      // (Markdown links wrap-and-bleed almost 100% of the time because
      // the closer-bearing space is the only natural break point in
      // `[label](url)`.) Moving the closer to the next surviving cell is
      // safe because ANSI escapes are zero-width and the skipped char
      // (whitespace) doesn't render anyway at a row boundary.
      let carriedAnsi = '';
      while (curStart < cells.length && cells[curStart]!.ch.trim() === '') {
        carriedAnsi += cells[curStart]!.ansi;
        curStart += 1;
      }
      if (carriedAnsi) {
        if (curStart < cells.length) {
          cells[curStart] = {
            ...cells[curStart]!,
            ansi: carriedAnsi + cells[curStart]!.ansi,
          };
        } else {
          // Every remaining cell was whitespace — let the trailing-tail
          // handler below append the closer onto the last row, same as
          // the prior trailing-ANSI fix does for `pendingAnsi`.
          pendingAnsi = carriedAnsi + pendingAnsi;
        }
      }
      curWidth = 0;
      lastSpace = -1;
      activeWidth = wR;
      // Recount width from the new start up through the current cell.
      for (let kk = curStart; kk <= k; kk++) {
        curWidth += cells[kk]!.width;
        if (cells[kk]!.ch === ' ' || cells[kk]!.ch === '\t') lastSpace = kk;
      }
      // If a single cell is wider than activeWidth (long unbreakable run),
      // flush it on its own line and continue.
      if (curWidth > activeWidth) {
        flush(k + 1);
        curStart = k + 1;
        curWidth = 0;
        lastSpace = -1;
        activeWidth = wR;
      }
      continue;
    }
    curWidth += c.width;
    if (c.ch === ' ' || c.ch === '\t') lastSpace = k;
  }
  if (curStart < cells.length) flush(cells.length);
  if (out.length === 0) out.push('');
  // Trailing ANSI sequences live in `pendingAnsi` after the cell-build loop —
  // they're escapes that came AFTER the last visible character (typically a
  // color/style reset). The cell loop only attaches `pendingAnsi` to the
  // NEXT cell's `ansi` field, so a trailing escape with no cell behind it
  // gets silently dropped. That's how cli-highlight's `\x1b[39m` (foreground
  // reset, emitted at the end of every highlighted token) was disappearing
  // — the closing fence's `chalk.dim('```')` only resets `dim` (`\x1b[22m`),
  // so the unclosed red bled into the prose below the block. Append the
  // tail to the final row so the reset survives.
  if (pendingAnsi) {
    out[out.length - 1] = (out[out.length - 1] ?? '') + pendingAnsi;
  }
  // SGR carryover across wrap rows. wrapAnsiLine emits each cell's `ansi`
  // bytes inline, so a styled span that opens at row 0's first cell and
  // doesn't close until row N's last cell has the open sequence on row 0
  // ONLY — every continuation row renders in default style. That's
  // invisible when the terminal does the wrap (the same logical line
  // preserves SGR state across visual rows), but with manual wrap each
  // row is its own logical line, so the styling vanishes the moment a
  // span crosses a wrap boundary.
  //
  // Walk the produced rows in order, tracking accumulated open SGR
  // sequences. Any non-reset SGR seq appends to the active state; a full
  // reset (`\x1b[0m`) clears it. At the start of each continuation row,
  // prepend whatever's active so the row resumes the same style. The
  // model is correct for highlighters/chalk that close with `\x1b[0m`
  // (cli-highlight always does); for partial closers (`\x1b[22m`,
  // `\x1b[39m`) the state may carry over a code that's already been
  // partially closed — that's harmless because emitting a closed code
  // again is a no-op for the terminal.
  //
  // Scan the ORIGINAL row content (captured before we prepend
  // `activeSgr` to it) — NOT the post-prepend mutated string. Scanning
  // the mutated string re-finds every escape we just prepended and
  // appends it to `activeSgr` again on the same iteration, so each
  // continuation row doubles `activeSgr`'s length. With short outputs
  // (a few wrap rows) this is harmless; with long outputs that wrap to
  // many rows — e.g. a tool stdout line clipped to MAX_INPUT_LINE_CHARS
  // upstream then wrapping to 2500 rows of 80 cols — `activeSgr` blows
  // up to gigabytes within ~30 rows and the renderer OOMs (RangeError:
  // Out of memory). Capturing the original row first scales `activeSgr`
  // with the count of distinct source-content SGR escapes, which is
  // bounded by the actual styling in the input.
  if (out.length > 1) {
    let activeSgr = '';
    // eslint-disable-next-line no-control-regex
    const sgrRe = /\x1b\[[0-9;]*m/g;
    for (let i = 0; i < out.length; i++) {
      const original = out[i] ?? '';
      if (i > 0 && activeSgr) {
        out[i] = activeSgr + original;
      }
      sgrRe.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = sgrRe.exec(original)) !== null) {
        if (m[0] === '\x1b[0m') {
          activeSgr = '';
        } else {
          activeSgr += m[0];
        }
      }
    }
  }
  return out;
}

/** Tail-clip a plain string at `max` characters, appending `…`. Identical
 *  semantics to truncateInline but used for block-mode value clipping where
 *  ANSI handling isn't needed (raw values come straight off the parsed JSON
 *  before any styling is applied). */
export function clipChars(s: string, max: number | null): string {
  if (max == null || max <= 0) return s;
  return s.length > max ? s.slice(0, max - 1) + '…' : s;
}

/**
 * Soft-wrap `value` after a leading `keyPrefix` so the first line is
 * `<dim>keyPrefix</dim>value...` and continuation lines are padded to
 * `continuationCols` visible columns. Wraps at word boundaries when possible.
 */
export function wrapKeyedLine(
  keyPrefix: string,
  value: string,
  continuationCols: number,
  termCols: number
): string[] {
  const dimPrefix = chalk.dim(keyPrefix);
  const prefixCols = visibleWidth(keyPrefix);
  const firstAvail = Math.max(8, termCols - prefixCols);
  const restAvail = Math.max(8, termCols - continuationCols);
  const chunks = wrapAtWords(value, firstAvail, restAvail);
  const indent = ' '.repeat(continuationCols);
  return chunks.map((chunk, i) =>
    i === 0 ? `${dimPrefix}${chunk}` : `${indent}${chunk}`
  );
}

export function wrapPlainLine(
  value: string,
  indentCols: number,
  termCols: number
): string[] {
  const avail = Math.max(8, termCols - indentCols);
  const indent = ' '.repeat(indentCols);
  const chunks = wrapAtWords(value, avail, avail);
  return chunks.map((c) => `${indent}${c}`);
}

/**
 * Break `s` into chunks where the first chunk fits in `firstWidth` columns and
 * subsequent chunks fit in `restWidth`. Prefers breaking at whitespace; falls
 * back to hard-cut on long unbreakable runs (URLs, ids, file paths).
 *
 * Single forward pass: a sliding cursor accumulates per-char width and tracks
 * the most recent whitespace position in the current chunk. When the running
 * width exceeds the budget, we emit at last whitespace if one exists, else
 * hard-cut at the overflow character. The previous shape called
 * `visibleWidth(remaining)` on the full tail every iteration (O(n²); a 100K
 * unbreakable run froze the renderer for 30+ seconds).
 */
export function wrapAtWords(
  s: string,
  firstWidth: number,
  restWidth: number
): string[] {
  if (!s) return [''];
  const out: string[] = [];
  // chunkStart..i is the current chunk-in-progress. After every emit we
  // advance chunkStart past the cut and skip leading whitespace, then continue
  // walking from the new position. Worst-case re-walk per emit is bounded by
  // the column budget, so total work stays O(n). Iterate by code points so
  // astral chars (emoji) stay in a single cell on wrap boundaries.
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

/**
 * Cheap per-character zero-width check. `visibleWidth` already handles these
 * via the BMP combining-mark range, but routing single-char calls through
 * `visibleWidth` (which checks ASCII fast path, ANSI scan, then segments)
 * costs a Map lookup each time. Inline check shaves ~30% off the wrap pass on
 * pure-ASCII input. Covers the same ranges twinki's visibleWidth does for
 * BMP combining marks + ZW joiners, so wrap math agrees with measurement.
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
