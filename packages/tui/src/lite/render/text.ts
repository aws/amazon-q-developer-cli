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
    // Silence cli-highlight's tokenizer warnings — they write to console.error
    // and would corrupt the TTY.
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
 * and double-width chars), appending `…`. ANSI-aware: a naive char-count would
 * cut mid-escape-sequence and corrupt downstream rendering.
 */
export function clipVisibleWidth(s: string, maxChars: number): string {
  if (visibleWidth(s) <= maxChars) return s;
  // Re-apply a single chalk.reset at the end so the next row starts clean
  // even if we cut mid-styled-segment.
  // eslint-disable-next-line no-control-regex
  const ansiRe = /\x1b\[[0-9;]*m/g;
  let out = '';
  let visible = 0;
  let i = 0;
  // Iterate by code points (not UTF-16 units) and account for double-width
  // chars so we never split a surrogate pair (emoji) or overshoot the cap.
  while (i < s.length && visible < maxChars - 1) {
    ansiRe.lastIndex = i;
    const match = ansiRe.exec(s);
    if (match && match.index === i) {
      out += match[0];
      i = match.index + match[0].length;
      continue;
    }
    const cp = s.codePointAt(i)!;
    const charLen = cp > 0xffff ? 2 : 1;
    const ch = s.slice(i, i + charLen);
    const w = visibleWidth(ch);
    if (visible + w > maxChars - 1) break;
    out += ch;
    visible += w;
    i += charLen;
  }
  return out + '…\x1b[0m';
}

export function stripAnsiQuick(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b\[[0-9;]*m/g, '');
}

/**
 * Visible-width-aware soft wrap for already-styled content. Width 0 disables
 * wrapping (one-line passthrough).
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
 * Soft-wrap a single (newline-free) ANSI-styled line. Preserves ANSI escapes
 * (zero-width) and hard-cuts overlong words. Returns at least one row even when
 * input is empty. Exported for `lite/diff.ts`, which relies on the SGR
 * carryover below so highlighted continuation rows keep their bg/color.
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
  // Build character cells (visible width + ANSI bytes from the prior
  // boundary), iterating by code point so an emoji's UTF-16 surrogate pair
  // can't split across rows into garbled lone surrogates. (ZWJ clusters still
  // split between codepoints; full grapheme clustering would need Intl.Segmenter.)
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
      // Skip leading whitespace on the next row, but carry forward any ANSI
      // escapes attached to the skipped cells: the break cell often holds the
      // closer for the span that just ended (e.g. `\x1b[22m` after bold). Drop
      // it and the unclosed style bleeds into every following row/message.
      // Safe to move because ANSI escapes are zero-width.
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
  // Escapes after the last visible char (typically a trailing reset) sit in
  // `pendingAnsi` with no following cell to attach to, so they'd be dropped —
  // e.g. cli-highlight's closing `\x1b[39m`, without which color bleeds past
  // the block. Append the tail to the final row so the reset survives.
  if (pendingAnsi) {
    out[out.length - 1] = (out[out.length - 1] ?? '') + pendingAnsi;
  }
  // SGR carryover across wrap rows: manual wrap makes each row its own logical
  // line, so a span opening on row 0 but closing on row N would render rows
  // 1..N in default style. Track open SGR state (full `\x1b[0m` clears it) and
  // prepend it to each continuation row.
  // INVARIANT: scan the ORIGINAL (pre-prepend) row, not the mutated one — else
  // we re-count the escapes we just prepended and `activeSgr` doubles per row,
  // OOMing on inputs that wrap to thousands of rows.
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

/** Tail-clip a plain (un-styled) string at `max` characters, appending `…`. */
export function clipChars(s: string, max: number | null): string {
  if (max == null || max <= 0) return s;
  return s.length > max ? s.slice(0, max - 1) + '…' : s;
}

/**
 * Soft-wrap `value` after a leading dim `keyPrefix`; continuation lines are
 * padded to `continuationCols` columns.
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
