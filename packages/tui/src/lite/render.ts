/**
 * Lite-mode render core: markdown / diff / syntax-highlight rendering for the
 * lite TUI.
 */
import { chalk } from '../utils/color.js';
import { highlight } from 'cli-highlight';
import { diffLines } from 'diff';
import { AnsiCodeTracker, visibleWidth } from '../utils/text-width.js';
import { resolveHighlightLanguage } from '../utils/highlight-languages.js';
import { getAgentDisplayName } from '../utils/agentColors.js';
import {
  parseMarkdown,
  parseInlineMarkdown,
  stripInlineMarkdown,
  type MarkdownSegment,
} from '../utils/markdown.js';
import {
  constrainColumnWidths,
  wrapCellText,
  padCell,
  shouldStackTable,
  formatStackedTable,
  type Alignment,
} from '../utils/table-layout.js';
import { UNICODE_GLYPHS, type Glyphs } from '../utils/glyphs.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';
import {
  isParentSubagentTool,
  isTrivialTool,
  resolveScrollbackToolRenderer,
  resolveToolDisplayName,
  resolveToolId,
  toolDiffPolicy,
  type ScrollbackToolRenderer,
  type ToolCallOrigin,
} from '../types/tool-capabilities.js';
import { formatLineRange } from '../types/tool-status.js';
import {
  getVerboseDisplay,
  shouldShowToolOutput,
  categorize,
  isMcpMessage,
  type VerboseDisplayConfig,
} from './verbose.js';
import { needsLeadingBlankByRole } from './blank-rules.js';
import {
  normalizeSubagentPrompt,
  orderSubagentStageItems,
} from '../utils/subagent-display.js';
import { unescapeJsonNewlines } from '../utils/tool-result.js';
import type { ToolDenial } from '../utils/tool-denial.js';

// Number-column width shared by read/write + diff renderers so they line up.
const LINE_NUM_WIDTH = 4;

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
 * input is empty. The SGR carryover below is what keeps highlighted diff
 * continuation rows holding their bg/color across a wrap boundary.
 */
export function wrapAnsiLine(
  line: string,
  firstWidth: number,
  restWidth: number
): string[] {
  if (!line) return [''];
  const w0 = firstWidth > 0 ? firstWidth : line.length;
  const wR = restWidth > 0 ? restWidth : line.length;
  // Tool output and prose are usually plain text. Avoid allocating one Cell
  // object per code point when no ANSI state needs to cross a wrap boundary.
  if (!line.includes('\x1b')) {
    return wrapAtWords(line, w0, wR).map((row) => row.trimEnd());
  }
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
    if (line.charCodeAt(i) === 0x1b) {
      ansiRe.lastIndex = i;
      const m = ansiRe.exec(line);
      if (m && m.index === i) {
        pendingAnsi += m[0];
        i = m.index + m[0].length;
        continue;
      }
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
 * Soft-wrap `value` with a literal `firstPrefix` on row 0 and `restIndentCols`
 * spaces on continuation rows. Row 0's word-budget is termCols minus the
 * prefix's visible width; continuation rows' budget is termCols minus the
 * indent — so a wide first prefix doesn't push text past the edge.
 */
function wrapWithIndent(
  value: string,
  firstPrefix: string,
  firstPrefixCols: number,
  restIndentCols: number,
  termCols: number
): string[] {
  const firstAvail = Math.max(8, termCols - firstPrefixCols);
  const restAvail = Math.max(8, termCols - restIndentCols);
  const indent = ' '.repeat(restIndentCols);
  return wrapAtWords(value, firstAvail, restAvail).map((chunk, i) =>
    i === 0 ? `${firstPrefix}${chunk}` : `${indent}${chunk}`
  );
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
  return wrapWithIndent(
    value,
    chalk.dim(keyPrefix),
    visibleWidth(keyPrefix),
    continuationCols,
    termCols
  );
}

export function wrapPlainLine(
  value: string,
  indentCols: number,
  termCols: number
): string[] {
  return wrapWithIndent(
    value,
    ' '.repeat(indentCols),
    indentCols,
    indentCols,
    termCols
  );
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
export function resolveGlyphs(g?: Glyphs): Glyphs {
  return g ?? UNICODE_GLYPHS;
}

// Must be frame-shaped (exception name leading a line, followed by `:`), not a
// bare prose mention — else an agent message explaining `AccessDeniedException`
// would be re-styled wholesale as a system error.
const ERROR_FRAME_RE =
  /(^|\n)\s*(?:ValidationException|ThrottlingException|ServiceException|AccessDeniedException|ResourceNotFoundException|InternalServerException)\s*:/;

export function isErrorContent(text: string): boolean {
  return ERROR_FRAME_RE.test(text);
}

// Default chalk wrappers for callers that don't thread a theme (tests, pure
// contexts); mirror the kiroDark base theme. When a theme IS available, callers
// pass RenderContext.theme and the renderer reads from there instead.
export const brand = chalk.hex('#C19AFF');
export const responseChip = chalk.hex('#FF8FB1');
export const DEFAULT_USER_TAG = chalk.bold.cyan;
// Tool-output body tint — soft sage-green reads as "successful result" without
// competing with neutral prose; errors stay loud red.
export const softSuccessOutput = chalk.hex('#a3c0a3');

// Per-render theme accessors, passed via RenderContext.theme so the renderer
// stays pure. Each field is a chalk-like `(s) => string`; every one falls back
// to a legacy hardcoded color when the theme is unavailable.
export interface RenderTheme {
  brand: (s: string) => string;
  primary: (s: string) => string;
  responseChip: (s: string) => string;
  userTag: (s: string) => string;
  userBody: (s: string) => string;
  inlineCode: (s: string) => string;
  link: (s: string) => string;
  secondary: (s: string) => string;
  /**
   * Diff row bg tints. `null` means the theme explicitly opts out of bg
   * tints (kiroSafe's `background: 'default'`) — diff rows then render
   * git-style: no tint, whole line painted with diffAddedFg/diffRemovedFg.
   * This keeps rows readable when the terminal background is unknown
   * (SSH / failed detection), where a hardcoded dark tint under the
   * terminal's default fg is unreadable on light backgrounds.
   */
  diffAddedBg: ((s: string) => string) | null;
  diffRemovedBg: ((s: string) => string) | null;
  diffAddedBar: (s: string) => string;
  diffRemovedBar: (s: string) => string;
  /** Whole-line fg colors used only when the bg slots are null. */
  diffAddedFg?: (s: string) => string;
  diffRemovedFg?: (s: string) => string;
}

// The diff bgHex values must match diff.ts's ADDED_BG_OPEN / REMOVED_BG_OPEN
// constants exactly (applyBg re-asserts those SGRs across cli-highlight resets).
const DEFAULT_RENDER_THEME: RenderTheme = {
  brand,
  primary: chalk.white,
  responseChip,
  userTag: DEFAULT_USER_TAG,
  userBody: chalk.cyan,
  inlineCode: chalk.cyan,
  link: chalk.cyan,
  secondary: chalk.dim,
  diffAddedBg: chalk.bgHex('#1F2D22'),
  diffRemovedBg: chalk.bgHex('#2D1F22'),
  diffAddedBar: chalk.hex('#80ffb5'),
  diffRemovedBar: chalk.hex('#ff8080'),
};

export function resolveTheme(t?: RenderTheme): RenderTheme {
  return t ?? DEFAULT_RENDER_THEME;
}

// Each token falls back to a hardcoded color when the resolver throws — keeps
// lite render functional even if a custom theme misses a slot.
export function buildRenderTheme(
  getColor: (path: string) => any,
  getUserPromptColor?: () => any,
  getUserPromptBgHex?: () => string | undefined
): RenderTheme {
  const safeChalk = (path: string, fallback: (s: string) => string) => {
    try {
      const fn = getColor(path);
      // chalk chains are callable with (s) => string, but theme accessors
      // can return a chain that needs further calling. Test by invoking on
      // the empty string — if it doesn't return a string, fall back.
      const probe = fn('');
      if (typeof probe !== 'string') return fallback;
      return (s: string) => fn(s);
    } catch {
      return fallback;
    }
  };
  // getColor builds an FG-mode wrapper, so for diff bg tints we read its
  // resolved hex and rebuild as bg-mode chalk. The ansi256(N) sentinel (256-
  // color terminals) routes through bgAnsi256 to preserve the color-table
  // index — bgHex would double-convert through hex and lose precision.
  // 'inherit' (kiroSafe's `background: 'default'`) means the theme explicitly
  // wants NO bg tint — return null so diff rows render fg-only; only genuine
  // resolution failures (missing slot, throwing resolver) keep the legacy
  // dark fallback.
  const safeBgChalk = (
    path: string,
    fallback: (s: string) => string
  ): ((s: string) => string) | null => {
    try {
      const fn = getColor(path);
      const hex = fn?.hex;
      if (hex === 'inherit') return null;
      if (typeof hex !== 'string') return fallback;
      const ansi256Match = /^ansi256\((\d+)\)$/.exec(hex);
      if (ansi256Match) {
        const idx = parseInt(ansi256Match[1]!, 10);
        const bg = chalk.bgAnsi256(idx);
        return (s: string) => bg(s);
      }
      const bg = chalk.bgHex(hex);
      return (s: string) => bg(s);
    } catch {
      return fallback;
    }
  };
  let userTagColorFn: (s: string) => string = chalk.cyan;
  if (getUserPromptColor) {
    try {
      const fn = getUserPromptColor();
      const probe = fn('');
      if (typeof probe === 'string') userTagColorFn = (s: string) => fn(s);
    } catch {
      // keep chalk.cyan fallback
    }
  }
  // userBody composes the prompt text color with the prompt bg hex (if any)
  // so a Purple preset paints white-on-violet across the whole message body
  // in scrollback — matching what standard mode does via `<Box backgroundColor>`.
  let userBodyFn: (s: string) => string = userTagColorFn;
  if (getUserPromptBgHex) {
    try {
      const bgHex = getUserPromptBgHex();
      if (bgHex && bgHex !== 'inherit') {
        const bg = chalk.bgHex(bgHex);
        userBodyFn = (s: string) => bg(userTagColorFn(s));
      }
    } catch {
      // keep fg-only fallback
    }
  }
  return {
    brand: safeChalk('brand', brand),
    primary: safeChalk('primary', chalk.white),
    // No dedicated "response chip" slot — accent is the closest, still shifts per theme.
    responseChip: safeChalk('accent', responseChip),
    userTag: (s: string) => chalk.bold(userTagColorFn(s)),
    userBody: userBodyFn,
    inlineCode: safeChalk('highlight', chalk.cyan),
    link: safeChalk('link', chalk.cyan),
    secondary: safeChalk('secondary', chalk.dim),
    // Bg slots use safeBgChalk (cli-highlight resets need real bg SGRs to
    // re-assert); bar slots are fg glyph colors. Null bg = fg-only diff rows,
    // painted with the fg slots below (kiroSafe defines these as named
    // green/red, which the terminal palette keeps readable on any bg).
    diffAddedBg: safeBgChalk('diff.added.background', chalk.bgHex('#1F2D22')),
    diffRemovedBg: safeBgChalk(
      'diff.removed.background',
      chalk.bgHex('#2D1F22')
    ),
    diffAddedBar: safeChalk('diff.added.bar', chalk.hex('#80ffb5')),
    diffRemovedBar: safeChalk('diff.removed.bar', chalk.hex('#ff8080')),
    diffAddedFg: safeChalk('diff.added.foreground', chalk.green),
    diffRemovedFg: safeChalk('diff.removed.foreground', chalk.red),
  };
}
/**
 * COPY-PASTE INVARIANT (load-bearing across this file): prose, code blocks,
 * list items, blockquotes, and shell output each emit ONE logical line per
 * source line (no width-aware wrapping). The Static <Text wrap="overflow"> in
 * LiteLayout lets the terminal soft-wrap visually, so the clipboard keeps the
 * original logical line. Baking \n at every visual row boundary used to corrupt
 * URLs, shell commands, and run-on prose when they crossed the terminal edge.
 * Only structural blocks whose prefix must repeat per row (tables, list
 * markers, blockquote bars) wrap at termCols on purpose.
 */
export function renderUserMessage(text: string, theme?: RenderTheme): string {
  // `You:` tag + body painted with the user's prompt preset colors (so a
  // Purple preset paints white-on-violet, like standard mode's <Box bg>).
  // Continuation lines get no leading indent (see COPY-PASTE INVARIANT).
  const lines = text.split('\n');
  const first = lines[0] ?? '';
  const rest = lines.slice(1);
  const userTag = theme?.userTag ?? DEFAULT_USER_TAG;
  const userBody = theme?.userBody ?? ((s: string) => s);
  const head = userTag('You:') + ' ' + (first ? userBody(first) : '');
  if (rest.length === 0) return head;
  return `${head}\n${rest.map((l) => userBody(l)).join('\n')}`;
}

/**
 * Render an agent (model) message as styled markdown. Only finalized messages
 * are parsed here (streaming chunks render verbatim in the live region) since
 * partial-block text reflows as fences/lists/tables arrive. `termCols` drives
 * wrapping for structural blocks (tables etc.); see COPY-PASTE INVARIANT.
 */
export function renderAgentMessage(
  content: string,
  agentName?: string,
  theme?: RenderTheme,
  termCols?: number,
  getAgentTagColor?: (name: string) => (s: string) => string,
  glyphs?: Glyphs
): string {
  if (!content.trim()) return '';
  // Default to "Kiro" when no active agent is known. Built-in mode ids use
  // canonical product labels; custom agent names pass through.
  const rawAgentName = agentName?.trim();
  const tag = rawAgentName ? getAgentDisplayName(rawAgentName) : 'Kiro';
  // Per-agent role-tag color so scrollback matches the footer (getAgentColor);
  // falls back to theme.brand for the default agent and pure contexts.
  const tagColorFn =
    getAgentTagColor && agentName
      ? getAgentTagColor(agentName)
      : (theme?.brand ?? brand);
  // Theme fns are plain string→string and don't compose with chalk.x.bold, so
  // bold the colored tag manually. chalk.bold uses \x1b[1m…\x1b[22m (not a full
  // reset) so the foreground color survives.
  const tagBold = chalk.bold(tagColorFn(`${tag}:`));
  const cols = termCols && termCols > 0 ? termCols : 0;
  const restWidth = cols ? Math.max(20, cols) : 0;

  const body = renderMarkdownToLines(content, restWidth, glyphs, theme);
  if (body.length === 0) return tagBold;
  const [first, ...rest] = body;
  // When the body opens with a structural block whose first row is chrome
  // (table border, code fence, blockquote bar, rule), gluing `Kiro: ` onto
  // it shifts only row 0 right and breaks alignment — so put the tag on its
  // own line. Inline markdown keeps the `Kiro: <reply>` form.
  if (firstBlockNeedsOwnLine(content)) {
    return tagBold + '\n' + body.join('\n');
  }
  if (!first || rest.length === 0) return tagBold + ' ' + (first ?? '');
  return [tagBold + ' ' + first, ...rest].join('\n');
}

/**
 * True when the first markdown segment is a structural block whose first row
 * is chrome (table border / code fence / blockquote bar / rule) and so must
 * paint at column 0. Headers and list items deliberately stay inline (no
 * chrome on row 0; a leading `- ` reads fine after the role tag).
 */
function firstBlockNeedsOwnLine(text: string): boolean {
  const segments = parseMarkdown(text);
  if (segments.length === 0) return false;
  const first = segments[0]!;
  return !!(
    first.codeBlock ||
    first.table ||
    first.blockquote ||
    first.horizontalRule
  );
}

/**
 * Render the agent's reasoning ("thinking") block: brand-colored top/bottom
 * rules bracketing a dim-italic body. Callers gate on
 * showReasoningContent; returns '' for empty input.
 */
export function renderThinkingBlock(
  thinking: string,
  theme?: RenderTheme,
  termCols?: number,
  glyphs?: Glyphs
): string {
  if (!thinking || !thinking.trim()) return '';
  const cols = termCols && termCols > 0 ? termCols : 0;
  const indent = '';
  const brandFn = theme?.brand ?? brand;
  // Full width, less 1 col so an off-by-one stdout.columns doesn't wrap to col 0.
  const ruleWidth = cols > 0 ? Math.max(20, cols - 1) : 32;
  const g = resolveGlyphs(glyphs);
  // ASCII mode: lineHorizontal degrades to '-'; the label stays in slot.
  const h = g.lineHorizontal;
  const topRule = brandFn(
    h.repeat(3) + ' thinking ' + h.repeat(Math.max(3, ruleWidth - 13))
  );
  const bottomRule = brandFn(h.repeat(Math.max(3, ruleWidth)));
  // One logical body row per source line (see COPY-PASTE INVARIANT); the
  // terminal soft-wraps long lines and carries the dim-italic SGR across rows.
  const sourceLines = thinking.split('\n');
  // Trim trailing blank rows so the bottom rule sits flush against the body.
  while (
    sourceLines.length > 0 &&
    !sourceLines[sourceLines.length - 1]?.trim()
  ) {
    sourceLines.pop();
  }
  if (sourceLines.length === 0) return '';
  const body = sourceLines
    .map((r) => indent + chalk.dim.italic(r || ' '))
    .join('\n');
  return [topRule, body, bottomRule].join('\n');
}

/**
 * Render `!` shell-escape command output: a brand `! ` gutter per source line,
 * echoing the `!` the user typed to enter the mode. One gutter per source line
 * (see COPY-PASTE INVARIANT); not re-wrapped because shell output is already
 * column-0 positioned and may carry cursor-positioning escapes. Trailing blanks
 * trimmed; returns '' for empty input.
 */
export function renderShellOutputBlock(
  content: string,
  theme?: RenderTheme,
  termCols?: number
): string {
  if (!content) return '';
  const brandFn = theme?.brand ?? brand;
  const gutter = brandFn('! ');
  // Drop only trailing blanks — leading blanks can be meaningful.
  const lines = content.split('\n');
  while (lines.length > 0 && !lines[lines.length - 1]?.trim()) {
    lines.pop();
  }
  if (lines.length === 0) return '';
  void termCols; // accepted for API symmetry, unused (see fn doc)
  return lines.map((line) => gutter + line).join('\n');
}

// ─── Markdown → ANSI Lines ───────────────────────────────────────────────────

/**
 * Turn markdown into ANSI-styled rows ready to `\n`-join into <Static>.
 * Blocks are blank-line separated except adjacent same-indent list items
 * (mirrors TUI MarkdownRenderer's marginTop). `restWidth` of 0 skips
 * width-aware wrapping (tests use 0; see COPY-PASTE INVARIANT).
 */
export function renderMarkdownToLines(
  text: string,
  restWidth: number,
  glyphs?: Glyphs,
  theme?: RenderTheme
): string[] {
  const segments = parseMarkdown(text);
  if (segments.length === 0) return [];

  const out: string[] = [];
  let isFirstBlock = true;
  // Group consecutive plain-text inline segments into a single paragraph so
  // wrapping operates on the joined text rather than per-segment fragments.
  let textGroup: MarkdownSegment[] = [];

  const flushTextGroup = () => {
    if (textGroup.length === 0) return;
    const styled = textGroup
      .map((seg) => renderInlineSegment(seg, theme))
      .join('');
    if (!styled) {
      textGroup = [];
      return;
    }
    // One logical line per source paragraph; wrapStyled(s, 0, 0) is the
    // explicit no-wrap path (see COPY-PASTE INVARIANT). restWidth is still
    // consumed by the structural blocks below.
    void restWidth;
    appendBlock(out, isFirstBlock, () => wrapStyled(styled, 0, 0));
    isFirstBlock = false;
    textGroup = [];
  };

  let prev: MarkdownSegment | null = null;
  for (const seg of segments) {
    if (
      seg.codeBlock ||
      seg.header ||
      seg.boldHeading ||
      seg.blockquote ||
      seg.horizontalRule ||
      seg.table ||
      seg.listItem
    ) {
      flushTextGroup();
      const lines = renderBlockSegment(seg, restWidth, glyphs, theme);
      const skipBlank = prev
        ? prev.listItem && seg.listItem
          ? prev.listItem.indent === seg.listItem.indent
          : !!prev.blockquote && !!seg.blockquote
        : false;
      if (!isFirstBlock && !skipBlank) out.push('');
      out.push(...lines);
      isFirstBlock = false;
      prev = seg;
      continue;
    }
    // Inline-ish segment (paragraph text) — accumulate.
    textGroup.push(seg);
    prev = seg;
  }
  flushTextGroup();
  return out;
}

/** Append a block's lines, with a leading blank when not the first block. */
function appendBlock(
  out: string[],
  isFirstBlock: boolean,
  produce: () => string[]
): void {
  if (!isFirstBlock) out.push('');
  out.push(...produce());
}

function renderBlockSegment(
  seg: MarkdownSegment,
  width: number,
  glyphs?: Glyphs,
  theme?: RenderTheme
): string[] {
  const g = resolveGlyphs(glyphs);
  if (seg.codeBlock) return renderCodeBlock(seg.codeBlock, width);
  // INLINE RE-LEX CONTRACT: parseMarkdown keeps block bodies (headers,
  // blockquotes, list items, table cells) raw in seg.text, so the renderer
  // re-lexes them via renderInlineMarkdown (parseInlineMarkdown) to surface
  // **bold**/`code`/links. renderInlineSegment alone only honors flags
  // already on the segment and would emit the raw markers.
  if (seg.header || seg.boldHeading) {
    const inline = renderInlineMarkdown(seg.text, theme);
    return wrapStyled(chalk.bold(inline), width, width);
  }
  if (seg.listItem) return renderListItem(seg, width, theme);
  if (seg.blockquote) {
    const inline = renderInlineMarkdown(seg.text, theme);
    const prefix = chalk.dim(`${g.lineVertical} `);
    const styled = chalk.italic(inline);
    // One logical line with a leading │ (see COPY-PASTE INVARIANT).
    if (!inline) return [prefix];
    return [prefix + styled];
  }
  if (seg.horizontalRule) {
    const w = Math.min(40, width || 40);
    return [chalk.dim(g.lineHorizontal.repeat(Math.max(3, w)))];
  }
  if (seg.table) return renderMarkdownTable(seg.table, width, glyphs, theme);
  return [];
}

function renderListItem(
  seg: MarkdownSegment,
  _width: number,
  theme?: RenderTheme
): string[] {
  const list = seg.listItem!;
  const indent = '  '.repeat(list.indent);
  const bullet = list.ordered ? `${list.number ?? 1}.` : '-';
  const head = `${indent}${bullet} `;
  // Re-lex body via inline path (see INLINE RE-LEX CONTRACT).
  const inline = renderInlineMarkdown(seg.text, theme);
  if (!inline) return [head.trimEnd()];
  // One logical line with a leading bullet (see COPY-PASTE INVARIANT).
  return [head + inline];
}

function renderCodeBlock(
  code: { code: string; language?: string; isComplete: boolean },
  _width: number
): string[] {
  const lines: string[] = [];
  const lang = code.language ? ` ${code.language}` : '';
  lines.push(chalk.dim(`\`\`\`${lang}`));
  // Code lines emitted as-is, one per source line (see COPY-PASTE INVARIANT).
  const body = (code.code ?? '').replace(/\n+$/, '').split('\n');
  const language = resolveHighlightLanguage(code.language);
  for (const line of body) {
    lines.push(highlightLineSafe(line, language));
  }
  if (code.isComplete) lines.push(chalk.dim('```'));
  return lines;
}

function renderMarkdownTable(
  table: {
    headers: string[];
    rows: string[][];
    alignments: ('left' | 'center' | 'right')[];
  },
  termWidth: number,
  glyphs?: Glyphs,
  theme?: RenderTheme
): string[] {
  const headers = table.headers;
  if (headers.length === 0) return [];

  const measureRendered = (s: string) =>
    visibleWidth(renderInlineMarkdown(s, theme));

  const colWidths = headers.map((h, ci) => {
    const headerW = measureRendered(h);
    const dataW = table.rows.map((r) => measureRendered(r[ci] || ''));
    return Math.max(headerW, ...dataW, 3);
  });

  if (shouldStackTable(colWidths, termWidth)) {
    return formatStackedTable(
      headers,
      table.rows,
      (s) => renderInlineMarkdown(s, theme),
      chalk.bold
    );
  }

  if (termWidth > 0) constrainColumnWidths(colWidths, termWidth);

  // Active glyph set (Unicode box-drawing / ASCII fallbacks). teeLeft=┤,
  // teeRight=├ (modern TUI's semantics), mapped through directly.
  const g = resolveGlyphs(glyphs);
  const cornerTL = g.cornerTopLeft;
  const cornerTR = g.cornerTopRight;
  const cornerBL = g.cornerBottomLeft;
  const cornerBR = g.cornerBottomRight;
  const lineH = g.lineHorizontal;
  const lineV = g.lineVertical;
  const teeT = g.teeTop;
  const teeB = g.teeBottom;
  const teeL = g.teeLeft;
  const teeR = g.teeRight;
  const cross = g.tableCross;

  const border = (left: string, mid: string, right: string, fill: string) =>
    left + colWidths.map((w) => fill.repeat(w + 2)).join(mid) + right;

  const renderRow = (rawCells: string[], bold?: boolean): string[] => {
    const styledCells = rawCells.map((c) => {
      let s = renderInlineMarkdown(c, theme);
      if (bold && s) s = chalk.bold(s);
      return s;
    });
    const wrapped = styledCells.map((c, ci) =>
      wrapCellText(c, colWidths[ci]!, visibleWidth)
    );
    const maxLines = Math.max(1, ...wrapped.map((w) => w.length));
    const lines: string[] = [];
    for (let li = 0; li < maxLines; li++) {
      const cells = colWidths.map((_, ci) => {
        const styled = wrapped[ci]?.[li] ?? '';
        return padCell(
          styled,
          colWidths[ci]!,
          (table.alignments[ci] || 'left') as Alignment,
          visibleWidth
        );
      });
      const joined = cells.join(` ${chalk.dim(lineV)} `);
      lines.push(`${chalk.dim(lineV)} ${joined} ${chalk.dim(lineV)}`);
    }
    return lines;
  };

  const out: string[] = [];
  out.push(chalk.dim(border(cornerTL, teeT, cornerTR, lineH)));
  out.push(...renderRow(headers, true));
  if (table.rows.length > 0) {
    out.push(chalk.dim(border(teeR, cross, teeL, lineH)));
    for (let ri = 0; ri < table.rows.length; ri++) {
      out.push(...renderRow(table.rows[ri] ?? []));
      if (ri < table.rows.length - 1) {
        out.push(chalk.dim(border(teeR, cross, teeL, lineH)));
      }
    }
  }
  out.push(chalk.dim(border(cornerBL, teeB, cornerBR, lineH)));
  return out;
}

/**
 * Render a markdown segment as inline ANSI, recursing into children. Color
 * slots come from the theme (inlineCode for codespans — the `seg.quote` flag
 * is historical naming, not a blockquote tie-in).
 */
function renderInlineSegment(
  seg: MarkdownSegment,
  theme?: RenderTheme
): string {
  const t = resolveTheme(theme);
  if (seg.children && seg.children.length > 0) {
    const inner = seg.children
      .map((child) => renderInlineSegment(child, t))
      .join('');
    if (seg.bold) return chalk.bold(inner);
    if (seg.italic) return chalk.italic(inner);
    if (seg.strikethrough) return chalk.strikethrough(inner);
    if (seg.link) {
      // Underline applied separately from theme color so links stay distinct
      // on themes whose link slot matches prose (lite uses underline+color,
      // not OSC8, so it works in every terminal).
      const labeled = chalk.underline(t.link(inner));
      // Drop the `(url)` trailer when the label already equals the URL.
      const stripped = stripAnsiQuick(inner);
      if (stripped === seg.link.url) return labeled;
      return labeled + t.secondary(` (${seg.link.url})`);
    }
    return inner;
  }
  if (seg.quote) return t.inlineCode(seg.text);
  if (seg.bold) return chalk.bold(seg.text);
  if (seg.italic) return chalk.italic(seg.text);
  if (seg.strikethrough) return chalk.strikethrough(seg.text);
  return seg.text;
}

/**
 * Re-lex a raw block/cell body through the inline path (see INLINE RE-LEX
 * CONTRACT). Uses marked's lexInline so leading `#`/`-` aren't promoted to
 * headings/lists inside a cell.
 */
function renderInlineMarkdown(s: string, theme?: RenderTheme): string {
  if (!s) return '';
  const segs = parseInlineMarkdown(s);
  const t = resolveTheme(theme);
  return segs.map((seg) => renderInlineSegment(seg, t)).join('');
}
export interface ToolCallRenderInfo {
  name: string;
  /** Reasoning ("why"), brand (purple). With an inline arg it renders on its
   *  own line(s) below the name so what (white args) and why (purple) don't
   *  share a slot; without one, the first line sits inline (legacy). */
  description?: string;
  /** Inline arg chip in default color: `tool [args]`. Independent of
   *  description so both can show under inline-args + reasoning. */
  inlineArg?: string;
  mcpServer?: string;
  agentPrefix?: string;
  elapsed?: number;
  status: 'running' | 'done' | 'error' | 'cancelled';
  isTrivial?: boolean;
  rejected?: boolean;
  /** STATUS-SLOT CONTRACT (running status only): awaitingApproval wins and
   *  paints a yellow ' ...' — the only path to ' ...' — because the agent
   *  isn't progressing while approval is pending, so a spinner would lie
   *  (matches the prompt's hotkey color). Otherwise runningSpinner shows the
   *  spinner glyph for motion regardless of triviality (a running trivial tool
   *  is pre-approved and genuinely in motion); the static path keeps ' ...'. */
  runningSpinner?: string;
  /** See STATUS-SLOT CONTRACT — takes precedence over runningSpinner. */
  awaitingApproval?: boolean;
}

/** Canonical built-in label (Shell/Read/…) so KAS titles ("Run Command",
 *  "List Directory") read like v2; raw name for MCP/unknown tools. */
export function toolDisplayName(
  name: string,
  kind?: string,
  origin?: ToolCallOrigin
): string {
  return resolveToolDisplayName(name, kind, origin);
}

/**
 * Lite-mode render of a {@link ToolDenial} — parity with the full TUI's
 * ToolDenialDetails card. A red "Blocked by <source>" line plus dim-labeled
 * Rule and (when known) Tool rows, indented to sit under the tool call. Returns
 * a `\n`-joined block WITHOUT a leading newline; the caller adds the separator.
 */
export function renderToolDenial(
  denial: ToolDenial,
  theme?: RenderTheme
): string {
  const secondary = theme?.secondary ?? chalk.dim;
  const lines = [
    `  ${chalk.red('Blocked')} by ${denial.source}`,
    `  ${secondary('Rule')} ${denial.rule}`,
  ];
  if (denial.tool) lines.push(`  ${secondary('Tool')} ${denial.tool}`);
  return lines.join('\n');
}

export function renderToolCall(
  info: ToolCallRenderInfo,
  theme?: RenderTheme
): string {
  const isTrivial = info.isTrivial ?? isTrivialTool(info.name);

  const brandFn = theme?.brand ?? brand;
  const agent = info.agentPrefix ? chalk.blue(info.agentPrefix) : '';
  const source = info.mcpServer ? chalk.dim(`${info.mcpServer}/`) : '';
  const name = isTrivial ? chalk.dim.bold(info.name) : chalk.bold(info.name);
  // Uncolored chip (default fg) reads as literal "what was passed"; brand
  // color is reserved for reasoning so what-vs-why is distinguishable.
  const argChip = info.inlineArg ? ` ${info.inlineArg}` : '';

  let statusStr: string;
  switch (info.status) {
    case 'running':
      // See STATUS-SLOT CONTRACT on ToolCallRenderInfo.
      if (info.awaitingApproval) {
        statusStr = chalk.yellow(' ...');
      } else if (info.runningSpinner) {
        statusStr = ` ${info.runningSpinner}`;
      } else {
        statusStr = chalk.dim(' ...');
      }
      break;
    case 'done':
      statusStr =
        info.elapsed != null
          ? chalk.dim(` ${formatElapsed(info.elapsed)}`)
          : '';
      break;
    case 'error':
      statusStr = info.rejected ? chalk.red(' DENIED') : chalk.red(' FAILED');
      break;
    case 'cancelled':
      // renderToolCall isn't threaded glyphs; the cancelled mark is a rare
      // branch, so read the active set live rather than widen the signature.
      statusStr = chalk.yellow(` ${getActiveGlyphs().cross} cancelled`);
      break;
  }

  // Reasoning layout (see ToolCallRenderInfo.description): with an inline arg
  // it goes on its own line(s) below; without one, the first line is inline.
  const descLines = info.description ? info.description.split('\n') : [];
  const inlineDesc =
    info.inlineArg || descLines.length === 0
      ? ''
      : ` ${brandFn(descLines[0] ?? '')}`;
  const indentedLines = info.inlineArg ? descLines : descLines.slice(1);
  const firstLine = `${agent}${source}${name}${argChip}${inlineDesc}${statusStr}`;
  if (indentedLines.length === 0) return firstLine;
  const indent = '    ';
  return [
    firstLine,
    ...indentedLines.map((l) => `${indent}${brandFn(l)}`),
  ].join('\n');
}

/**
 * Render a write/edit tool call with a unified diff. Used by the chat-log
 * finalizer and the approval prompt. `suppressDiff` returns the bare header.
 *
 * WIRE FORMAT (referenced throughout this file): fs_write args are snake_case
 * (Rust serde, crates/.../tools/fs_write.rs): command `str_replace`/`create`/
 * `insert`/`append`, fields `old_str`/`new_str`/`file_text`/`insert_line`.
 * camelCase variants are accepted as a fallback for non-Rust callers.
 */
export function renderWriteToolCall(
  info: ToolCallRenderInfo,
  content: string,
  opts: {
    suppressDiff?: boolean;
    termCols?: number;
    startLine?: number;
    theme?: RenderTheme;
  } = {}
): string {
  if (opts.suppressDiff) return renderToolCall(info, opts.theme);

  let path: string | undefined;
  let oldText = '';
  let newText = '';
  let startLine = opts.startLine ?? 1;

  try {
    const args = JSON.parse(content);
    path = args.path;
    const oldStr = args.old_str ?? args.oldStr;
    const newStr = args.new_str ?? args.newStr;
    const fileText = args.file_text ?? args.content;
    const insertLine = args.insert_line ?? args.insertLine;
    if (
      args.command === 'str_replace' ||
      args.command === 'strReplace' ||
      (oldStr && newStr != null)
    ) {
      oldText = String(oldStr ?? '');
      newText = String(newStr ?? '');
    } else if (args.command === 'insert' || insertLine != null) {
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
      if (typeof insertLine === 'number') startLine = insertLine + 1;
    } else if (args.command === 'append') {
      // Pure-add block (no baseline read for trailing context), like create.
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
    } else if (
      args.command === 'create' ||
      (!oldStr && (fileText != null || newStr != null))
    ) {
      oldText = '';
      newText = String(fileText ?? newStr ?? '');
    }
  } catch {
    return renderToolCall(info, opts.theme);
  }

  const out: string[] = [renderToolCall(info, opts.theme)];
  // Suppress the diff's path header only when the inline chip already carries
  // the path (inline mode); in block/off mode the chip is empty, so the diff
  // must print the path itself or the filename appears nowhere.
  const diff = renderUnifiedDiff(oldText, newText, {
    path,
    suppressPathHeader: !!info.inlineArg,
    startLine,
    termCols: opts.termCols,
    theme: opts.theme,
  });
  // Diffs render in full (the payload being reviewed; no safe tail to drop).
  if (diff.length > 0) {
    out.push(...diff);
  } else if (!info.inlineArg && path) {
    // Empty diff (delete / empty create / no-op edit) has no path row of its
    // own — show the filename here so a chip-less mode still names the file.
    out.push(chalk.dim(`  ${path}`));
  }
  return out.join('\n');
}

/**
 * Render a read-style tool call (fs_read et al.): header + path line + numbered,
 * highlighted body (no gutter/bg — this is inspection, not a change). Capped to
 * maxLines visual rows via the same outputMaxLines knob the bar formatter uses.
 */
export function renderReadToolCall(
  info: ToolCallRenderInfo,
  content: string,
  result?: { status: string; error?: string; output?: unknown },
  opts: {
    termCols?: number;
    maxLines?: number | null;
    maxCharsPerLine?: number | null;
    theme?: RenderTheme;
    glyphs?: Glyphs;
  } = {}
): string {
  const cols = opts.termCols ?? 80;
  const g = resolveGlyphs(opts.glyphs);
  let path: string | undefined;
  let startLine = 1;
  try {
    const args = JSON.parse(content);
    // `operations` is the multi-read shape; fall back to top-level path.
    const op = Array.isArray(args.operations) ? args.operations[0] : null;
    path = op?.path ?? args.path ?? args.file_path ?? args.filePath;
    if (op && typeof op.offset === 'number') startLine = op.offset + 1;
    else if (typeof args.offset === 'number') startLine = args.offset + 1;
  } catch {
    // fall through; the path header will simply be skipped.
  }

  const out: string[] = [renderToolCall(info, opts.theme)];

  // Errors take the loud red bar path (red glyph AND body) so they stay visible.
  if (result?.status === 'error' && result.error) {
    const indent = '    ';
    const barPrefix = `${indent}${g.lineVertical} `;
    const avail = Math.max(20, cols - visibleWidth(barPrefix));
    const lines = formatBarBlock(
      result.error,
      avail,
      barPrefix,
      chalk.red,
      chalk.red
    );
    if (lines.length > 0) out.push(...lines);
    return out.join('\n');
  }

  // No body to surface — fall back to the bare tool-call header so the row
  // still says what was attempted.
  if (result?.output == null) return out.join('\n');
  const text =
    typeof result.output === 'string'
      ? result.output
      : unwrapToolOutputAsText(result.output);
  if (!text.trim()) return out.join('\n');

  if (path) out.push(chalk.dim(`  ${path}`));

  const language = resolveLanguageFromPathLite(path);

  const linePrefixCols = 2 + LINE_NUM_WIDTH + 1;
  const codeCols = Math.max(20, cols - linePrefixCols);
  const sourceLines = text.replace(/\n+$/, '').split('\n');

  // Wrap each highlighted line; continuation rows blank the number column.
  const visualRows: string[] = [];
  for (let i = 0; i < sourceLines.length; i++) {
    const line = sourceLines[i] ?? '';
    const numStr = String(startLine + i).padStart(LINE_NUM_WIDTH);
    const dimNum = chalk.dim(`  ${numStr} `);
    const blankNum = chalk.dim('  ' + ' '.repeat(LINE_NUM_WIDTH) + ' ');
    const bounded = boundToolOutputLine(line, 'end');
    if (bounded.droppedChars > 0) {
      visualRows.push(
        `${blankNum}${chalk.dim(
          `... (line clipped; +${bounded.droppedChars} chars before)`
        )}`
      );
    }
    const styled = highlightLineSafe(bounded.text, language);
    const chunks = wrapAnsiLine(styled, codeCols, codeCols);
    if (chunks.length === 0) {
      visualRows.push(dimNum);
      continue;
    }
    visualRows.push(`${dimNum}${chunks[0]}`);
    for (let j = 1; j < chunks.length; j++) {
      visualRows.push(`${blankNum}${chunks[j]}`);
    }
  }

  // Per-row char clip honoring maxCharsPerLine (visible-width aware).
  const clipped =
    opts.maxCharsPerLine && opts.maxCharsPerLine > 0
      ? visualRows.map((r) => clipVisibleWidth(r, opts.maxCharsPerLine!))
      : visualRows;

  // Tail-window at maxLines (keep the LAST N, marker above) — matches the
  // tail the user saw scroll past in the live region.
  const capped = applyTailLineCap(clipped, opts.maxLines ?? null, (n) =>
    chalk.dim(`  ${' '.repeat(LINE_NUM_WIDTH)} ... (+${n} more lines above)`)
  );

  // Footer line count so truncated reads still name the file's true length.
  out.push(...capped);
  const lineCount = sourceLines.length;
  out.push(chalk.dim(`  ${lineCount} ${lineCount === 1 ? 'line' : 'lines'}`));
  return out.join('\n');
}

export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

type LiteToolRenderMode = 'write' | 'read' | 'subagent' | 'task' | 'generic';

function resolveLiteToolRenderMode(
  renderer: ScrollbackToolRenderer
): LiteToolRenderMode {
  switch (renderer) {
    case 'write':
      return 'write';
    case 'read':
      return 'read';
    case 'session':
      return 'subagent';
    case 'task':
      return 'task';
    case 'shell':
    case 'web_search':
    case 'web_fetch':
    case 'grep':
    case 'glob':
    case 'ls':
    case 'code':
    case 'introspect':
    case 'image_read':
    case 'goal':
    case 'knowledge':
    case 'workflow':
    case 'generic':
      return 'generic';
    default: {
      const exhaustive: never = renderer;
      return exhaustive;
    }
  }
}

function resolveLiteToolRenderDispatch(
  name: string,
  kind?: string,
  origin?: ToolCallOrigin
): {
  mode: LiteToolRenderMode;
  isRead: boolean;
  isWriteWithDiff: boolean;
} {
  const mode = resolveLiteToolRenderMode(
    resolveScrollbackToolRenderer(name, kind, origin)
  );
  return {
    mode,
    isRead: mode === 'read',
    isWriteWithDiff:
      mode === 'write' && toolDiffPolicy(name, kind, origin) === 'unified',
  };
}

/**
 * Render the tool's output as a `│ `-bar block at column 4 (so a call reads
 * name → args → response). Gated by the /verbose filter list; errors always
 * surface (red bar) regardless of the filter.
 */
export function renderVerboseOutput(
  toolName: string,
  result?: { status: string; error?: string; output?: unknown },
  maxLines?: number | null,
  filtersOverride?: readonly string[],
  maxCharsPerLine?: number | null,
  termCols?: number,
  glyphs?: Glyphs,
  isMcp = false
): string {
  if (!result) return '';
  // Errors always surface; the filter only gates successful output.
  const isError = result.status === 'error';
  if (!isError && !shouldShowToolOutput(toolName, filtersOverride, isMcp))
    return '';
  // termCols is threaded from RenderContext so all renderers agree on width
  // (and resizes don't re-flow already-flushed rows differently).
  const cols = Math.max(40, termCols ?? 120);
  const indent = '    ';
  const g = resolveGlyphs(glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  // Dim "output:" header so the args block and the bar don't merge visually.
  // Empty-output paths early-return before this, so no orphan label.
  const outputHeader = chalk.dim('  output:');

  // Per-row visible-width clip at maxCharsPerLine (both error + normal paths).
  const clipRow = (s: string): string => {
    if (maxCharsPerLine == null || maxCharsPerLine <= 0) return s;
    return clipVisibleWidth(s, maxCharsPerLine);
  };

  // Shared finalize: tail-window the bar rows (marker tinted to match the
  // body), prepend the dim "output:" header, then per-row width-clip.
  const finalize = (
    lines: string[],
    markerColor: (s: string) => string
  ): string => {
    if (lines.length === 0) return '';
    const capped = applyTailLineCap(lines, maxLines ?? null, (n) =>
      markerColor(`${barPrefix}... (truncated; +${n} more lines above)`)
    );
    return '\n' + outputHeader + '\n' + capped.map(clipRow).join('\n');
  };

  if (result.status === 'error') {
    // Prefer the explicit error field; fall back to the output envelope so
    // failures that put their reason in the body still surface.
    let errText = result.error ?? '';
    if (!errText && result.output != null) {
      errText =
        typeof result.output === 'string'
          ? result.output
          : unwrapToolOutputAsText(result.output);
    }
    if (errText.trim().length === 0) return '';
    return finalize(
      formatBarBlock(errText, avail, barPrefix, chalk.red, chalk.red),
      chalk.red
    );
  }
  if (result.output == null) return '';
  const unwrapped: UnwrappedToolOutput =
    typeof result.output === 'string'
      ? { kind: 'text', value: result.output }
      : unwrapToolOutput(result.output);

  // Structured envelope → key:value tree (same green success tint as text
  // outputs so the success/error signal is consistent across both branches).
  if (unwrapped.kind === 'json') {
    return finalize(
      formatJsonAsBarLines(
        unwrapped.value,
        barPrefix,
        cols,
        maxCharsPerLine ?? null,
        softSuccessOutput
      ),
      chalk.dim
    );
  }

  if (!unwrapped.value.trim()) return '';
  // Dim glyph (chrome) + sage-green body (content); red+red on errors above.
  return finalize(
    formatBarBlock(
      unwrapped.value,
      avail,
      barPrefix,
      chalk.dim,
      softSuccessOutput
    ),
    chalk.dim
  );
}

/**
 * Render an in-flight tool's output as a `│`-bar block, tail-windowed to
 * outputMaxLines (marker above) so the live preview matches the eventual
 * static rendering. Respects shouldShowToolOutput; returns [] when there's
 * nothing to show. (Only fires when the store's liveOutputs map is fed by
 * per-chunk ToolCallUpdate text, which some backends don't emit for shell —
 * the static finalizer uses the same cap, so scrollback stays consistent.)
 */
interface LiveOutputChunk {
  rows: string[];
  hasContent: boolean;
}

const liveOutputChunkCache = new WeakMap<
  readonly string[],
  Map<string, LiveOutputChunk>
>();

function formatLiveOutputChunk(
  sourceLines: readonly string[],
  avail: number,
  barPrefix: string,
  maxChars: number | null
): LiveOutputChunk {
  const cacheKey = `${avail}\0${barPrefix}\0${maxChars ?? ''}\0${chalk.level}`;
  let byFormat = liveOutputChunkCache.get(sourceLines);
  const cached = byFormat?.get(cacheKey);
  if (cached) return cached;

  const formatted = formatBarBlock(
    sourceLines.join('\n'),
    avail,
    barPrefix,
    chalk.dim,
    softSuccessOutput
  );
  const rows =
    maxChars != null && maxChars > 0
      ? formatted.map((line) => clipVisibleWidth(line, maxChars))
      : formatted;
  const result = {
    rows,
    hasContent: sourceLines.some((line) => line.trim().length > 0),
  };
  if (!byFormat) {
    byFormat = new Map();
    liveOutputChunkCache.set(sourceLines, byFormat);
  }
  byFormat.set(cacheKey, result);
  return result;
}

export function takeTailRows(
  chunks: readonly { rows: readonly string[] }[],
  count: number
): string[] {
  const reversed: string[] = [];
  for (let ci = chunks.length - 1; ci >= 0 && reversed.length < count; ci--) {
    const rows = chunks[ci]!.rows;
    for (let ri = rows.length - 1; ri >= 0 && reversed.length < count; ri--) {
      reversed.push(rows[ri]!);
    }
  }
  return reversed.reverse();
}

export function renderLiveStreamingOutputBar(
  toolName: string,
  sourceChunks: readonly (readonly string[])[],
  opts: {
    outputMaxLines: number | null;
    outputMaxChars: number | null;
    termCols: number;
    filtersOverride?: readonly string[];
    glyphs?: Glyphs;
    isMcp?: boolean;
  }
): string[] {
  if (!shouldShowToolOutput(toolName, opts.filtersOverride, opts.isMcp)) {
    return [];
  }
  if (sourceChunks.length === 0) return [];

  const cols = Math.max(40, opts.termCols);
  const indent = '    ';
  const g = resolveGlyphs(opts.glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  const chunks = sourceChunks.map((chunk) =>
    formatLiveOutputChunk(chunk, avail, barPrefix, opts.outputMaxChars ?? null)
  );
  if (!chunks.some((chunk) => chunk.hasContent)) return [];
  const totalRows = chunks.reduce((sum, chunk) => sum + chunk.rows.length, 0);
  if (totalRows === 0) return [];

  // Tail-window in visual rows; "streaming" distinguishes it from the
  // finalized "truncated" marker.
  const cap = opts.outputMaxLines ?? null;
  const isCapped = cap != null && cap > 0 && totalRows > cap;
  const shown = isCapped
    ? takeTailRows(chunks, cap)
    : chunks.flatMap((chunk) => chunk.rows);
  const tailCapped = isCapped
    ? [
        chalk.dim(
          `${barPrefix}... (streaming; +${totalRows - shown.length} more lines above)`
        ),
        ...shown,
      ]
    : shown;
  return [chalk.dim('  output:'), ...tailCapped];
}

/**
 * Pretty-print parsed JSON as `│ `-prefixed key:value rows, reusing
 * {@link formatArgLines} so styling matches the args block. No depth cap (the
 * user controls footprint via outputMaxLines). Optional `bodyColor` tints each
 * row (chalk's dim-on/off keeps dim keys intact under the tint).
 */
function formatJsonAsBarLines(
  parsed: unknown,
  barPrefix: string,
  termCols: number,
  maxChars: number | null,
  bodyColor?: (s: string) => string
): string[] {
  // Render at indent=0 (bar prefix is the visual anchor).
  const innerCols = Math.max(20, termCols - visibleWidth(barPrefix));
  const rawLines: string[] = [];

  if (parsed == null || typeof parsed !== 'object') {
    rawLines.push(formatScalar(parsed));
  } else if (Array.isArray(parsed)) {
    // One row per element (no key — the bar sits where a key would).
    for (const item of parsed) {
      if (item != null && typeof item === 'object' && !Array.isArray(item)) {
        rawLines.push(chalk.dim('-'));
        for (const [k, v] of Object.entries(item)) {
          rawLines.push(
            ...formatArgLines(
              k,
              v,
              1,
              Number.POSITIVE_INFINITY,
              innerCols,
              maxChars
            )
          );
        }
      } else {
        const scalar =
          typeof item === 'string'
            ? clipChars(formatScalar(item), maxChars)
            : formatScalar(item);
        rawLines.push(`${chalk.dim('-')} ${scalar}`);
      }
    }
  } else {
    const obj = parsed as Record<string, unknown>;
    for (const [k, v] of Object.entries(obj)) {
      rawLines.push(
        ...formatArgLines(
          k,
          v,
          0,
          Number.POSITIVE_INFINITY,
          innerCols,
          maxChars
        )
      );
    }
  }

  return rawLines.map((l) => {
    const body = bodyColor ? bodyColor(l) : l;
    return chalk.dim(barPrefix) + body;
  });
}

/**
 * Head-cap: keep the FIRST `cap` rows, append a marker below. Used for input
 * args (leading keys like `command`/`path` are the most informative). For
 * output use {@link applyTailLineCap}. `cap` null/<=0 disables; caller must
 * pre-wrap `lines` to visual rows.
 */
export function applyLineCap(
  lines: string[],
  cap: number | null,
  marker: (dropped: number) => string
): string[] {
  if (cap == null || cap <= 0 || lines.length <= cap) return lines;
  const dropped = lines.length - cap;
  return [...lines.slice(0, cap), marker(dropped)];
}

/**
 * Tail-cap: keep the LAST `cap` rows, prepend the marker above. Used for output
 * bars so static rendering matches the tail the user saw stream past. Same
 * semantics as {@link applyLineCap}.
 */
function applyTailLineCap(
  lines: string[],
  cap: number | null,
  marker: (dropped: number) => string
): string[] {
  if (cap == null || cap <= 0 || lines.length <= cap) return lines;
  const dropped = lines.length - cap;
  return [marker(dropped), ...lines.slice(-cap)];
}

/** `text` = a human-readable string the bar can stream as-is; `json` = an
 *  unknown shape the caller pretty-prints as a key:value tree. */
type UnwrappedToolOutput =
  | { kind: 'text'; value: string }
  | { kind: 'json'; value: unknown };

/**
 * Pull the meaningful payload out of a tool result envelope. Known text shapes:
 * shell {items:[{Json:{stdout,stderr,exit_status}}]}, items[].Text,
 * items[].Json.{text,content}, {content:[{text}]}. Unknown → {kind:'json'}
 * with the most informative parsed object.
 */
function unwrapToolOutput(output: unknown): UnwrappedToolOutput {
  if (output == null || typeof output !== 'object') {
    return { kind: 'text', value: safeJson(output, 1_000_000) };
  }
  const obj = output as Record<string, unknown>;

  // ACP {items: [...]} envelope — the canonical wire shape.
  if (Array.isArray(obj.items) && obj.items.length > 0) {
    const itemText = (raw: unknown): string | null => {
      if (!raw || typeof raw !== 'object') return null;
      const item = raw as Record<string, unknown>;
      if (typeof item.Text === 'string') return item.Text;
      if (item.Json && typeof item.Json === 'object') {
        const inner = item.Json as Record<string, unknown>;
        const shell = formatShellEnvelope(inner);
        if (shell != null) return shell;
        if (typeof inner.text === 'string') return inner.text;
        if (typeof inner.content === 'string') return inner.content;
      }
      return null;
    };

    // Multi-item: concatenate every item that yields text (reading only
    // items[0] silently dropped the rest).
    if (obj.items.length > 1) {
      const parts: string[] = [];
      for (const it of obj.items) {
        const t = itemText(it);
        if (t != null) parts.push(t);
      }
      if (parts.length > 0) return { kind: 'text', value: parts.join('\n') };
    } else {
      const first = obj.items[0] as Record<string, unknown> | undefined;
      const t = itemText(first);
      if (t != null) return { kind: 'text', value: t };
      // Single Json item, unknown shape — hand the inner up for the tree.
      if (
        first &&
        typeof first === 'object' &&
        first.Json &&
        typeof first.Json === 'object'
      ) {
        return { kind: 'json', value: first.Json as Record<string, unknown> };
      }
    }
  }

  // {content: [{text}]} — ACP content-block style.
  if (Array.isArray(obj.content) && obj.content.length > 0) {
    const parts: string[] = [];
    for (const item of obj.content) {
      if (
        item &&
        typeof item === 'object' &&
        'text' in item &&
        typeof (item as Record<string, unknown>).text === 'string'
      ) {
        parts.push((item as Record<string, unknown>).text as string);
      }
    }
    if (parts.length > 0) return { kind: 'text', value: parts.join('\n') };
  }

  // Plain shell-shaped object (no items wrapper).
  const shell = formatShellEnvelope(obj);
  if (shell != null) return { kind: 'text', value: shell };

  // Genuine unknown — surface the parsed object so the caller pretty-prints.
  return { kind: 'json', value: output };
}

/** Text-only form of unwrapToolOutput; JSON envelopes become a compact
 *  safeJson string (used by the read-tool path). */
function unwrapToolOutputAsText(output: unknown): string {
  const r = unwrapToolOutput(output);
  if (r.kind === 'text') return r.value;
  return unescapeJsonNewlines(safeJson(r.value, 1_000_000));
}

/** Format a shell-result-shaped object (stdout, then "(exit N)", then
 *  "[stderr] ..."); null when it isn't shell-shaped. */
function formatShellEnvelope(obj: Record<string, unknown>): string | null {
  const hasStdout = typeof obj.stdout === 'string';
  const hasStderr = typeof obj.stderr === 'string';
  const hasExit = 'exit_status' in obj;
  if (!hasStdout && !hasStderr && !hasExit) return null;

  const parts: string[] = [];
  const stdout = hasStdout ? (obj.stdout as string).replace(/\n+$/, '') : '';
  if (stdout.length > 0) parts.push(stdout);

  if (hasExit) {
    let code: number | null = null;
    const exit = obj.exit_status;
    if (typeof exit === 'number') code = exit;
    else if (typeof exit === 'string') {
      const m = exit.match(/(-?\d+)/);
      if (m && m[1]) code = parseInt(m[1], 10);
    }
    if (code != null && code !== 0) parts.push(`(exit ${code})`);
  }

  if (hasStderr) {
    const stderr = (obj.stderr as string).replace(/\n+$/, '');
    if (stderr.length > 0) parts.push(`[stderr] ${stderr}`);
  }

  return parts.join('\n');
}

/**
 * Pre-wrap cap on a single tool-output source line. The visual-row cap runs
 * after wrapping, so it cannot protect the wrapper from a multi-MB minified
 * JSON/base64 line. 50K still preserves hundreds of terminal rows while
 * bounding the expensive ANSI/Unicode path.
 */
export const MAX_TOOL_OUTPUT_LINE_CHARS = 50_000;

export interface BoundedToolOutputLine {
  text: string;
  droppedChars: number;
}

/**
 * Keep one bounded edge of a tool-output line without cutting a surrogate pair
 * or SGR escape. Tail retention also reapplies SGR state active at the cut.
 */
export function boundToolOutputLine(
  source: string,
  keep: 'start' | 'end'
): BoundedToolOutputLine {
  if (source.length <= MAX_TOOL_OUTPUT_LINE_CHARS) {
    return { text: source, droppedChars: 0 };
  }

  if (keep === 'start') {
    let end = MAX_TOOL_OUTPUT_LINE_CHARS;
    const before = source.charCodeAt(end - 1);
    const after = source.charCodeAt(end);
    if (
      before >= 0xd800 &&
      before <= 0xdbff &&
      after >= 0xdc00 &&
      after <= 0xdfff
    ) {
      end -= 1;
    }
    const openEscape = source.lastIndexOf('\x1b', end - 1);
    const closedEscape = source.lastIndexOf('m', end - 1);
    if (openEscape > closedEscape) end = openEscape;
    const retained = source.slice(0, end);
    return {
      text: retained.includes('\x1b') ? `${retained}\x1b[0m` : retained,
      droppedChars: source.length - end,
    };
  }

  let start = source.length - MAX_TOOL_OUTPUT_LINE_CHARS;
  const at = source.charCodeAt(start);
  const before = source.charCodeAt(start - 1);
  if (at >= 0xdc00 && at <= 0xdfff && before >= 0xd800 && before <= 0xdbff) {
    start += 1;
  }
  const openEscape = source.lastIndexOf('\x1b', start);
  if (openEscape >= 0) {
    const escapeEnd = source.indexOf('m', openEscape);
    if (escapeEnd >= start) start = escapeEnd + 1;
  }

  // Recreate the net SGR state at the retained tail's boundary. The tracker
  // keeps this prefix constant-size even if the dropped input changes color
  // thousands of times.
  const sgrState = new AnsiCodeTracker();
  // eslint-disable-next-line no-control-regex
  const sgrRe = /\x1b\[[0-9;]*m/g;
  let match: RegExpExecArray | null;
  while ((match = sgrRe.exec(source)) !== null && match.index < start) {
    sgrState.process(match[0]);
  }
  return {
    text: sgrState.getActiveCodes() + source.slice(start),
    droppedChars: start,
  };
}

/**
 * Hard-wrap each line of `text` to `availCols`, prefixing every visual row
 * with `barPrefix` (styled by `glyphColor`; body by `bodyColor` or plain).
 * Hard wrap (not terminal soft-wrap) so wrapped rows keep the `│` margin
 * instead of crashing to col 0 — accepts the copy-paste tradeoff since tool
 * output is read more than copied. Uses {@link wrapAnsiLine} (SGR carryover).
 */
function formatBarBlock(
  text: string,
  availCols: number,
  barPrefix: string,
  glyphColor: (s: string) => string,
  bodyColor?: (s: string) => string
): string[] {
  const out: string[] = [];
  const sourceLines = text.split('\n');
  // Floor the wrap budget so a narrow terminal doesn't burn a row per char.
  const w = Math.max(8, availCols);
  for (const rawLine of sourceLines) {
    if (rawLine.length === 0) {
      out.push(glyphColor(barPrefix.trimEnd()));
      continue;
    }
    // Pre-clip very long lines, keeping the TAIL to
    // match downstream applyTailLineCap; marker before the clipped content.
    const bounded = boundToolOutputLine(rawLine, 'end');
    if (bounded.droppedChars > 0) {
      out.push(
        `${glyphColor(barPrefix)}${chalk.dim(
          `... (line clipped; +${bounded.droppedChars} chars before)`
        )}`
      );
    }
    const body = bodyColor ? bodyColor(bounded.text) : bounded.text;
    const chunks = wrapAnsiLine(body, w, w);
    for (const chunk of chunks) {
      if (chunk.length === 0) {
        out.push(glyphColor(barPrefix.trimEnd()));
      } else {
        out.push(`${glyphColor(barPrefix)}${chunk}`);
      }
    }
  }
  return out;
}

/**
 * Shorten an absolute path for chips to the same file, shorter spelling:
 * relative → unchanged; under cwd → cwd-relative; under home → `~`-form;
 * else unchanged. Reads cwd/HOME defensively (may be absent in test workers;
 * both are per-process stable so no re-render race).
 */
function shortenPathForChip(path: string): string {
  if (!path) return path;
  // Already relative — don't accidentally match an unanchored prefix vs cwd.
  if (!path.startsWith('/') && !path.startsWith('~')) return path;
  let cwd: string | undefined;
  try {
    cwd =
      typeof process !== 'undefined' && typeof process.cwd === 'function'
        ? process.cwd()
        : undefined;
  } catch {
    cwd = undefined;
  }
  const home =
    typeof process !== 'undefined' && process.env
      ? process.env.HOME || process.env.USERPROFILE
      : undefined;
  if (cwd && path.startsWith(cwd + '/')) return path.slice(cwd.length + 1);
  if (cwd && path === cwd) return '.';
  if (home && path.startsWith(home + '/')) return '~' + path.slice(home.length);
  if (home && path === home) return '~';
  return path;
}

/**
 * Drop a leading `cd <path> &&` / `pushd <path> &&` segment and any leading
 * `VAR=value` env prefixes so the chip shows the command's actual work, not the
 * navigation boilerplate that otherwise wins the head-only clip. Falls back to
 * the original when nothing significant remains (e.g. a bare `cd /x`).
 */
export function stripShellPreamble(command: string): string {
  let rest = command.trim();
  // Leading env-var assignments: FOO=bar BAZ=qux <cmd>
  const envRe = /^[A-Za-z_][A-Za-z0-9_]*=(?:"[^"]*"|'[^']*'|\S*)\s+/;
  while (envRe.test(rest)) rest = rest.replace(envRe, '');
  // A single leading `cd`/`pushd <path> &&` segment.
  const cdMatch = /^(?:cd|pushd)\s+\S[^&]*?&&\s*(\S.*)$/.exec(rest);
  if (cdMatch?.[1]) rest = cdMatch[1].trim();
  return rest.length > 0 ? rest : command;
}

/**
 * Build the inline arg chip: the most informative single-line summary of the
 * tool's args. Search tools combine "what" + " in " + "where"; write tools
 * surface a verb + path (so create/edit/insert/delete differ at a glance).
 * Paths go through {@link shortenPathForChip}; empty/`.` paths are dropped.
 */
export function extractInlineArg(
  toolName: string,
  content: string,
  maxChars: number | null = 80,
  kind?: string,
  origin?: ToolCallOrigin
): string | undefined {
  if (!content) return undefined;
  let args: Record<string, unknown> | undefined;
  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === 'object')
      args = parsed as Record<string, unknown>;
  } catch {
    return undefined;
  }
  if (!args) return undefined;

  // Shell tools: the command is the chip. The renderer family gate prevents a
  // write tool's `command` discriminator from winning this branch.
  if (
    resolveScrollbackToolRenderer(toolName, kind, origin) === 'shell' &&
    typeof args.command === 'string' &&
    args.command.length > 0
  ) {
    const firstLine = args.command.split('\n')[0] ?? '';
    return `[${clipChars(stripShellPreamble(firstLine), maxChars)}]`;
  }

  // Write tools: verb + relative path (path alone hides the operation).
  if (resolveScrollbackToolRenderer(toolName, kind, origin) === 'write') {
    const path = typeof args.path === 'string' ? args.path : null;
    if (path) {
      let verb = 'write';
      const oldStr = args.old_str ?? args.oldStr;
      const fileText = args.file_text ?? args.content ?? args.text;
      const insertLine = args.insert_line ?? args.insertLine;
      // Prefer the explicit `command`; infer from shape for older callers.
      if (args.command === 'create') verb = 'create';
      else if (
        args.command === 'str_replace' ||
        args.command === 'strReplace' ||
        oldStr != null
      )
        verb = 'edit';
      else if (args.command === 'insert' || insertLine != null) verb = 'insert';
      else if (args.command === 'append') verb = 'append';
      else if (args.command === 'delete') verb = 'delete';
      else if (fileText != null && oldStr == null) verb = 'create';
      return `[${clipChars(`${verb} ${shortenPathForChip(path)}`, maxChars)}]`;
    }
  }

  // Code intelligence: `operation` is the discriminator (search_symbols vs
  // goto_definition vs get_diagnostics…); the generic branches below drop it
  // and show only the symbol/path. Prefix it + the target, like Code.tsx.
  if (resolveToolId(toolName, kind, origin) === 'code') {
    const operation =
      typeof args.operation === 'string' ? args.operation : null;
    const target =
      (typeof args.symbol_name === 'string' && args.symbol_name) ||
      (typeof args.pattern === 'string' && `"${args.pattern}"`) ||
      (typeof args.file_path === 'string' &&
        shortenPathForChip(args.file_path)) ||
      null;
    const label = [operation, target].filter(Boolean).join(' ');
    if (label) return `[${clipChars(label, maxChars)}]`;
  }

  // Pattern/query tools (grep, glob, search): combine "what" + " in " + path.
  const queryField =
    (typeof args.pattern === 'string' && args.pattern) ||
    (typeof args.query === 'string' && args.query) ||
    (typeof args.search_query === 'string' && args.search_query) ||
    (typeof args.symbol_name === 'string' && args.symbol_name) ||
    null;
  if (queryField) {
    const pathField = typeof args.path === 'string' ? args.path : null;
    if (pathField && pathField !== '.' && pathField !== '') {
      const relPath = shortenPathForChip(pathField);
      // cwd resolves to "." — adds no info (footer already shows cwd).
      if (relPath !== '.') {
        return `[${clipChars(`${queryField} in ${relPath}`, maxChars)}]`;
      }
    }
    return `[${clipChars(queryField, maxChars)}]`;
  }

  if (typeof args.url === 'string' && args.url.length > 0) {
    return `[${clipChars(args.url, maxChars)}]`;
  }

  // Read tools may pass an `operations: [{ path }]` array (multi-read API).
  // Append the line range (offset/limit) so reads of the same file at different
  // ranges read distinctly — matching the full-TUI Read component.
  const op = Array.isArray(args.operations)
    ? (args.operations[0] as Record<string, unknown> | undefined)
    : undefined;
  if (op && typeof op.path === 'string')
    return `[${clipChars(shortenPathForChip(op.path) + formatLineRange(op as { offset?: number; limit?: number }), maxChars)}]`;

  // Path-only tools (read, etc.). KAS read_file sends offset/limit alongside
  // the flat `path`, so append the same line-range suffix.
  for (const key of ['path', 'file_path', 'filePath']) {
    const v = args[key];
    if (typeof v === 'string' && v.length > 0)
      return `[${clipChars(shortenPathForChip(v) + formatLineRange(args as { offset?: number; limit?: number }), maxChars)}]`;
  }

  // `name` / `key` are short identifiers — pass through unshortened.
  for (const key of ['name', 'key']) {
    const v = args[key];
    if (typeof v === 'string' && v.length > 0)
      return `[${clipChars(v, maxChars)}]`;
  }

  // Last resort: the model's stated purpose, else the raw `command` — covers
  // command-bearing tools whose title isn't in SHELL_TOOL_NAMES (e.g. an
  // agent-advertised "Shell" rather than the canonical lowercase alias), which
  // would otherwise render no chip at all. Bracketed so it reads as a chip.
  const purpose = args.__tool_use_purpose;
  if (typeof purpose === 'string' && purpose.length > 0)
    return `[${clipChars(purpose, maxChars)}]`;
  if (typeof args.command === 'string' && args.command.length > 0)
    return `[${clipChars(stripShellPreamble(args.command.split('\n')[0] ?? ''), maxChars)}]`;
  return undefined;
}

/**
 * Extract only the real LLM reasoning (`__tool_use_purpose`); undefined when
 * absent — used in inline-args mode so args aren't shown twice (once as a
 * chip, once masquerading as purple reasoning).
 */
export function extractToolReasoning(
  content: string,
  typedPurpose?: string
): string | undefined {
  // Prefer the typed `purpose` sibling (captured at the ACP boundary from
  // untouched rawInput) so edit-kind tools, whose rebuilt content loses
  // __tool_use_purpose, still surface reasoning; else parse content.
  if (typeof typedPurpose === 'string' && typedPurpose.trim().length > 0) {
    return typedPurpose;
  }
  if (!content) return undefined;
  try {
    const parsed = JSON.parse(content) as Record<string, unknown>;
    const purpose = parsed.__tool_use_purpose;
    if (typeof purpose === 'string' && purpose.trim().length > 0) {
      return purpose;
    }
  } catch {
    // Non-JSON content — treat as no reasoning.
  }
  return undefined;
}

/**
 * key: value pairs for scrollback (skips internal fields); null when empty.
 * Returns raw lines so callers can wrap each in their own container.
 * `perValueLineCap` is the per-value multi-line clamp; the block-mode renderer
 * passes null when argsMaxLines is "unlimited" so that toggle means no
 * truncation anywhere (P438130055). Others keep the 5-line default.
 */
export function formatToolArgLines(
  toolName: string,
  content: string,
  termCols?: number,
  maxChars: number | null = null,
  perValueLineCap: number | null = 5
): string[] | null {
  if (!content) return null;
  try {
    const args = JSON.parse(content);
    if (!args || typeof args !== 'object') return null;

    const cols =
      termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120;

    // Read tool: flatten the operations array into readable fields.
    if (
      args.operations &&
      Array.isArray(args.operations) &&
      args.operations.length > 0
    ) {
      const lines: string[] = [];
      for (const op of args.operations) {
        if (op.path)
          lines.push(
            ...wrapKeyedLine(
              '  path: ',
              clipChars(String(op.path), maxChars),
              4,
              cols
            )
          );
        if (op.depth != null)
          lines.push(chalk.dim('  depth: ') + String(op.depth));
        if (op.limit != null)
          lines.push(chalk.dim('  lines: ') + String(op.limit));
        if (op.offset != null)
          lines.push(chalk.dim('  offset: ') + String(op.offset));
      }
      return lines.length > 0 ? lines : null;
    }

    const entries = Object.entries(args).filter(([key]) => {
      if (key.startsWith('_')) return false;
      // __tool_use_purpose already renders inline as the description.
      if (key === '__tool_use_purpose') return false;
      return true;
    });
    if (entries.length === 0) return null;
    const lines: string[] = [];
    for (const [key, val] of entries) {
      lines.push(
        ...formatArgLines(key, val, 1, 4, cols, maxChars, perValueLineCap)
      );
    }
    return lines.length > 0 ? lines : null;
  } catch {
    return null;
  }
}

/**
 * Render a (key, value) pair with nested indentation (`indent` = 2-space
 * levels). Long string values wrap at word boundaries, padded to the parent
 * indent so the tree doesn't crash to col 0.
 */
function formatArgLines(
  key: string,
  val: unknown,
  indent: number,
  maxDepth = 4,
  termCols = 120,
  maxChars: number | null = null,
  // Per-value multi-line cap; null lifts it (block mode under "unlimited",
  // P438130055). Default 5 keeps a sensible bound for other callers.
  perValueLineCap: number | null = 5
): string[] {
  const pad = '  '.repeat(indent);
  const dimKey = chalk.dim(`${pad}${key}:`);
  const continuationCols = (indent + 1) * 2;
  if (val == null) {
    return [`${dimKey} ${chalk.dim('null')}`];
  }
  if (typeof val === 'string') {
    // Multi-line values: up to perValueLineCap source lines + a delta marker.
    // maxChars (argsMaxChars) clips per-line only — decoupled from line count
    // so each knob does what its name says; total height is bounded by
    // argsMaxLines at the outer applyLineCap.
    if (val.includes('\n')) {
      const valLines = val.split('\n');
      const visible =
        perValueLineCap == null
          ? valLines.length
          : Math.min(perValueLineCap, valLines.length);
      const head = wrapKeyedLine(
        `${pad}${key}: `,
        clipChars(valLines[0] ?? '', maxChars),
        continuationCols,
        termCols
      );
      const tail: string[] = [];
      for (const l of valLines.slice(1, visible)) {
        tail.push(
          ...wrapPlainLine(clipChars(l, maxChars), continuationCols, termCols)
        );
      }
      const out = [...head, ...tail];
      // Delta marker (lines hidden), matching the output bar's idiom.
      if (valLines.length > visible) {
        const hidden = valLines.length - visible;
        out.push(
          chalk.dim(`${'  '.repeat(indent + 1)}... (+${hidden} more lines)`)
        );
      }
      return out;
    }
    return wrapKeyedLine(
      `${pad}${key}: `,
      clipChars(val, maxChars),
      continuationCols,
      termCols
    );
  }
  if (typeof val === 'number' || typeof val === 'boolean') {
    return [`${dimKey} ${String(val)}`];
  }
  if (Array.isArray(val)) {
    if (val.length === 0) return [`${dimKey} ${chalk.dim('[]')}`];
    if (indent >= maxDepth) {
      return [`${dimKey} ${chalk.dim(safeJson(val, 200))}`];
    }
    const out: string[] = [`${dimKey}`];
    for (let i = 0; i < val.length; i++) {
      const item = val[i];
      if (item != null && typeof item === 'object' && !Array.isArray(item)) {
        // object element — emit "- " marker then nested keys
        const childPad = '  '.repeat(indent + 1);
        out.push(`${childPad}${chalk.dim('-')}`);
        for (const [k, v] of Object.entries(item)) {
          out.push(
            ...formatArgLines(k, v, indent + 2, maxDepth, termCols, maxChars)
          );
        }
      } else {
        const childPad = '  '.repeat(indent + 1);
        const scalar =
          typeof item === 'string'
            ? clipChars(formatScalar(item), maxChars)
            : formatScalar(item);
        out.push(`${childPad}${chalk.dim('-')} ${scalar}`);
      }
    }
    return out;
  }
  if (typeof val === 'object') {
    const obj = val as Record<string, unknown>;
    const entries = Object.entries(obj);
    if (entries.length === 0) return [`${dimKey} ${chalk.dim('{}')}`];
    if (indent >= maxDepth) {
      return [`${dimKey} ${chalk.dim(safeJson(obj, 200))}`];
    }
    const out: string[] = [`${dimKey}`];
    for (const [k, v] of entries) {
      out.push(
        ...formatArgLines(k, v, indent + 1, maxDepth, termCols, maxChars)
      );
    }
    return out;
  }
  return [`${dimKey} ${String(val)}`];
}

function formatScalar(val: unknown): string {
  if (val == null) return chalk.dim('null');
  if (typeof val === 'string') {
    return val.includes('\n') ? val.split('\n')[0] + chalk.dim(' …') : val;
  }
  return String(val);
}

function safeJson(val: unknown, max: number): string {
  try {
    const s = JSON.stringify(val);
    return s.length > max ? s.slice(0, max - 1) + '…' : s;
  } catch {
    return String(val);
  }
}

interface TaskInputArg {
  task_description?: unknown;
  details?: unknown;
}

/**
 * Format the built-in task list tool body (wire name `todo_list`/`task`/`todo`).
 * Relies on the agent-crate schema being stable; malformed args → null (caller
 * falls back to the generic JSON printer). Per command: create/add → numbered
 * tree of tasks + optional description block; complete → cyan id chips + notes
 * + files; remove → removed id chips + new_description; list → no body.
 * Returns the parsed command (for the inline chip) and body lines.
 */
export function formatTaskToolBody(
  content: string,
  termCols?: number,
  glyphsArg?: Glyphs
): { command: string; bodyLines: string[] } | null {
  if (!content) return null;
  let args: Record<string, unknown>;
  try {
    args = JSON.parse(content) as Record<string, unknown>;
  } catch {
    return null;
  }
  if (!args || typeof args !== 'object') return null;
  const command = args.command;
  if (typeof command !== 'string') return null;
  if (!['create', 'complete', 'add', 'remove', 'list'].includes(command)) {
    return null;
  }

  const cols = Math.max(
    40,
    termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const g = resolveGlyphs(glyphsArg);
  const indent = '  ';

  // Numbered task list with tree connectors (treeCorner on the last row);
  // subjects wrap with continuation rows aligned under the subject, with
  // optional dim `details` below.
  const renderTaskList = (tasks: TaskInputArg[]): string[] => {
    const out: string[] = [];
    if (tasks.length === 0) return out;
    const renderable = tasks
      .map((t) => ({
        subject:
          typeof t.task_description === 'string'
            ? t.task_description.trim()
            : '',
        details: typeof t.details === 'string' ? t.details.trim() : '',
      }))
      .filter((t) => t.subject);
    if (renderable.length === 0) return out;
    // Pad the index to the widest so "Task 10" doesn't shift vs "Task 1".
    const idxWidth = `${renderable.length}.`.length;
    const prefixCols = 2 + 3 + 1 + idxWidth + 1;
    const subjectAvail = Math.max(20, cols - prefixCols);
    const continuationIndent = ' '.repeat(prefixCols);
    for (let i = 0; i < renderable.length; i++) {
      const { subject, details } = renderable[i]!;
      const isLast = i === renderable.length - 1;
      const connector = isLast ? g.treeCorner : g.treeBranch;
      const num = `${i + 1}.`.padEnd(idxWidth + 1);
      const wrapped = wrapAtWords(subject, subjectAvail, subjectAvail);
      out.push(
        `${indent}${chalk.dim(connector)} ${chalk.dim(num)}${wrapped[0] ?? ''}`
      );
      for (const line of wrapped.slice(1)) {
        out.push(`${continuationIndent}${line}`);
      }
      if (details) {
        const wrappedDetails = wrapAtWords(details, subjectAvail, subjectAvail);
        for (const line of wrappedDetails) {
          out.push(`${continuationIndent}${chalk.dim(line)}`);
        }
      }
    }
    return out;
  };

  // Multi-line description block under a dim `label:` at column 4.
  const renderDescriptionBlock = (label: string, text: string): string[] => {
    const out: string[] = [`${indent}${chalk.dim(`${label}:`)}`];
    const avail = Math.max(20, cols - 4);
    const wrapped = wrapAtWords(text, avail, avail);
    for (const line of wrapped) out.push(`    ${line}`);
    return out;
  };

  const stringList = (val: unknown): string[] =>
    Array.isArray(val)
      ? val.filter((x): x is string => typeof x === 'string')
      : [];

  // Dim `label:` + cyan `#id` chips joined by a dim comma (complete/remove).
  const idChipLine = (label: string, ids: string[]): string =>
    `${indent}${chalk.dim(`${label}:`)} ${ids
      .map((id) => chalk.cyan(`#${id}`))
      .join(chalk.dim(', '))}`;

  const lines: string[] = [];

  if (command === 'create' || command === 'add') {
    const description =
      command === 'create'
        ? typeof args.task_list_description === 'string'
          ? args.task_list_description.trim()
          : ''
        : typeof args.new_description === 'string'
          ? args.new_description.trim()
          : '';
    if (description) {
      lines.push(...renderDescriptionBlock('description', description));
    }
    const taskKey = command === 'create' ? 'tasks' : 'new_tasks';
    const tasks = Array.isArray(args[taskKey])
      ? (args[taskKey] as TaskInputArg[])
      : [];
    if (tasks.length > 0) {
      lines.push(...renderTaskList(tasks));
    }
    return { command, bodyLines: lines };
  }

  if (command === 'complete') {
    const ids = stringList(args.completed_task_ids);
    if (ids.length > 0) lines.push(idChipLine('completed', ids));
    const ctxUpdate =
      typeof args.context_update === 'string' ? args.context_update.trim() : '';
    if (ctxUpdate) {
      lines.push(`${indent}${chalk.dim('notes:')}`);
      const avail = Math.max(20, cols - 4);
      const wrapped = wrapAtWords(ctxUpdate, avail, avail);
      for (const line of wrapped) lines.push(`    ${chalk.dim(line)}`);
    }
    const files = stringList(args.modified_files);
    if (files.length > 0) {
      lines.push(`${indent}${chalk.dim('files:')}`);
      for (const f of files) lines.push(`    ${chalk.dim('-')} ${f}`);
    }
    return { command, bodyLines: lines };
  }

  if (command === 'remove') {
    const ids = stringList(args.remove_task_ids);
    if (ids.length > 0) lines.push(idChipLine('removed', ids));
    const newDesc =
      typeof args.new_description === 'string'
        ? args.new_description.trim()
        : '';
    if (newDesc) {
      lines.push(...renderDescriptionBlock('description', newDesc));
    }
    return { command, bodyLines: lines };
  }

  // command === 'list' — the bare header carries all the meaning.
  return { command, bodyLines: [] };
}
export function extractSubagentOutput(result?: {
  status: string;
  error?: string;
  output?: unknown;
}): string | null {
  if (!result) return null;
  if (result.status === 'error') {
    if (result.error) return result.error;
    if (typeof result.output === 'string') return result.output;
    if (result.output != null) {
      try {
        return JSON.stringify(result.output, null, 2);
      } catch {
        return null;
      }
    }
    return null;
  }
  if (!result.output) return null;
  if (typeof result.output === 'string') return result.output;
  try {
    return JSON.stringify(result.output, null, 2);
  } catch {
    return null;
  }
}

interface SubagentStage {
  name?: string;
  role?: string;
  prompt_template?: string;
  depends_on?: string[];
}

/** Render literal stage input under the pipeline tree. */
export function renderStagePromptLines(
  prompt: string,
  avail: number,
  indent: string,
  color: (text: string) => string = chalk.white
): string[] {
  if (!prompt) return [];
  const out: string[] = [];
  for (const line of prompt.split('\n')) {
    if (line.length === 0) {
      out.push('');
      continue;
    }
    for (const visual of wrapAtWords(line, avail, avail)) {
      if (visual.trim().length === 0) continue;
      out.push(`${indent}${color(visual)}`);
    }
  }
  return out;
}

/**
 * Per-stage pipeline tree (branch/stem glyphs, `[name]` chip, role/deps chips,
 * {task}-substituted prompt) shared by the approval prompt and the final block.
 * Approval always shows role/deps/prompts; the final block gates each via `sub.*`.
 */
function renderPipelineStages(
  stages: SubagentStage[],
  task: string | null | undefined,
  opts: {
    inputColor: (name: string) => (text: string) => string;
    cols: number;
    theme?: RenderTheme;
    glyphs?: Glyphs;
    showRoles: boolean;
    showDeps: boolean;
    showPrompts: boolean;
  }
): string[] {
  const out: string[] = [];
  const g = resolveGlyphs(opts.glyphs);
  const primary = resolveTheme(opts.theme).primary;
  out.push(primary('  pipeline:'));
  for (let i = 0; i < stages.length; i++) {
    const stage = stages[i] ?? {};
    const isLast = i === stages.length - 1;
    const branch = isLast
      ? `${g.cornerBottomLeft}${g.lineHorizontal}`
      : `${g.teeRight}${g.lineHorizontal}`;
    const stem = isLast ? '  ' : `${g.lineVertical} `;
    const name = stage.name || `stage-${i + 1}`;
    const role =
      opts.showRoles && stage.role ? chalk.dim(` (${stage.role})`) : '';
    const deps =
      opts.showDeps &&
      Array.isArray(stage.depends_on) &&
      stage.depends_on.length > 0
        ? chalk.dim(` ${g.arrowLeft} ${stage.depends_on.join(', ')}`)
        : '';
    out.push(
      `    ${chalk.dim(branch)} ${opts.inputColor(name)(`[${name}]`)}${role}${deps}`
    );
    // {task} substituted on render so the display matches what the spawned
    // subagent receives (older binaries ship the raw template; backend also subs).
    const rawPrompt = stage.prompt_template;
    const prompt = rawPrompt
      ? normalizeSubagentPrompt(
          task ? rawPrompt.replace(/\{task\}/g, task) : rawPrompt
        )
      : rawPrompt;
    if (
      opts.showPrompts &&
      prompt &&
      typeof prompt === 'string' &&
      prompt.length > 0
    ) {
      const promptIndent = `    ${chalk.dim(stem)} `;
      // 7 = width of "    │ " + 1-col safety margin (stdout.columns can be off
      // by one, otherwise causing stray col-0 soft-wraps).
      const avail = Math.max(20, opts.cols - 7);
      out.push(...renderStagePromptLines(prompt, avail, promptIndent, primary));
    }
  }
  return out;
}

/**
 * "Chip at col 7 + markdown body at col 9" digest section (shared by `full
 * output:` and `response summary:`), pre-wrapped via wrapAnsiLine (SGR
 * carryover) so continuations don't crash to col 0.
 */
function renderDigestSection(
  header: string,
  entries: { stageName: string; body: string }[],
  opts: {
    chipFn: (stageName: string) => string;
    cols: number;
    glyphs?: Glyphs;
    outputMaxLines?: number | null;
    outputMaxChars?: number | null;
  }
): string[] {
  const out: string[] = [];
  const chipIndent = '       ';
  const bodyIndent = '         ';
  const avail = Math.max(20, opts.cols - visibleWidth(bodyIndent));
  out.push(header);
  for (let i = 0; i < entries.length; i++) {
    const stage = entries[i]!;
    const renderedRows = renderSubagentDigestRows(stage.body, avail, {
      glyphs: opts.glyphs,
    });
    const visibleRows =
      opts.outputMaxLines != null && opts.outputMaxLines > 0
        ? renderedRows.slice(0, opts.outputMaxLines)
        : renderedRows;
    const hiddenCount = renderedRows.length - visibleRows.length;
    out.push(`${chipIndent}${opts.chipFn(stage.stageName)}`);
    for (const row of visibleRows) {
      if (row.length === 0) {
        out.push('');
        continue;
      }
      const clipped =
        opts.outputMaxChars != null && opts.outputMaxChars > 0
          ? clipVisibleWidth(row, opts.outputMaxChars)
          : row;
      out.push(`${bodyIndent}${clipped}`);
    }
    if (hiddenCount > 0) {
      out.push(`${bodyIndent}${chalk.dim(`(+${hiddenCount} more lines)`)}`);
    }
    if (i < entries.length - 1) out.push('');
  }
  return out;
}

export function renderSubagentDigestRows(
  body: string,
  width: number,
  options: { glyphs?: Glyphs; theme?: RenderTheme } = {}
): string[] {
  const rows: string[] = [];
  const normalized = body.replace(/\r\n?/g, '\n').replace(/\n+$/, '');
  for (const markdownLine of renderMarkdownToLines(
    normalized,
    width,
    options.glyphs,
    options.theme
  )) {
    if (markdownLine.length === 0) rows.push('');
    else rows.push(...wrapAnsiLine(markdownLine, width, width));
  }
  return rows;
}

export function renderSubagentResponseSummaryLines(
  stageSummaries: readonly SubagentStageSummary[],
  cols: number,
  colors?: {
    getStageInputColor?: (stageName: string) => (text: string) => string;
    getStageOutputColor?: (stageName: string) => (text: string) => string;
    glyphs?: Glyphs;
    outputMaxLines?: number | null;
    outputMaxChars?: number | null;
  }
): string[] {
  type RenderableStage = {
    stageName: string;
    body: string;
  };
  const g = resolveGlyphs(colors?.glyphs);
  const outputColor = (name: string): ((text: string) => string) =>
    colors?.getStageOutputColor?.(name) ?? responseChip;
  // Plain responses chip in the input color so they match the prompt's stage
  // name; summaries fall back to the response chip.
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? outputColor(name);
  const renderable: RenderableStage[] = [];
  for (const s of stageSummaries) {
    const ctx = (s.contextSummary ?? '').trim();
    if (ctx.length > 0) {
      renderable.push({
        stageName: s.stageName,
        body: s.contextSummary,
      });
      continue;
    }
    const tr = (s.taskResult ?? '').trim();
    if (tr.length === 0) continue;
    renderable.push({
      stageName: s.stageName,
      body: s.taskResult,
    });
  }
  if (renderable.length === 0) return [];
  const allResponses = renderable.every((entry) => {
    const summary = stageSummaries.find((s) => s.stageName === entry.stageName);
    return summary?.kind === 'response';
  });
  const header = allResponses ? '  response:' : '  response summary:';
  // Plain-response stages chip in the input color (match the prompt); summary
  // stages keep the response/output color.
  const isResponseStage = (name: string): boolean =>
    stageSummaries.find((s) => s.stageName === name)?.kind === 'response';
  return renderDigestSection(chalk.dim(header), renderable, {
    chipFn: (name) =>
      chalk.bold(
        (isResponseStage(name) ? inputColor(name) : outputColor(name))(
          `${g.arrowRight} ${name}`
        )
      ),
    cols,
    glyphs: colors?.glyphs,
    outputMaxLines: colors?.outputMaxLines,
    outputMaxChars: colors?.outputMaxChars,
  });
}

/**
 * Approval-prompt renderer for the `subagent` tool: a per-stage pipeline tree
 * the user can read at a glance instead of a raw key:value JSON dump. Returns
 * one string per line (formatToolArgLines's contract, one <Text> per line).
 */
export function formatSubagentApprovalLines(
  content: string,
  termCols?: number,
  colors?: {
    getStageInputColor?: (stageName: string) => (text: string) => string;
    theme?: RenderTheme;
    /** Active glyph set (Unicode/ASCII connectors). */
    glyphs?: Glyphs;
  }
): string[] | null {
  if (!content) return null;
  let args: { task?: string; stages?: SubagentStage[] };
  try {
    args = JSON.parse(content);
  } catch {
    return null;
  }
  if (!args || typeof args !== 'object') return null;

  const cols = Math.max(
    40,
    termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const lines: string[] = [];

  // Labels + structural glyphs stay dim; stage names render as `[name]` in the
  // per-agent color (matching the footer/final block) so a row maps to a stage.
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  const stages = Array.isArray(args.stages) ? args.stages : [];
  // No standalone `task:` line — already surfaced via each stage's {task} sub.
  if (stages.length > 0) {
    lines.push(
      ...renderPipelineStages(stages, args.task, {
        inputColor,
        cols,
        theme: colors?.theme,
        glyphs: colors?.glyphs,
        showRoles: true,
        showDeps: true,
        showPrompts: true,
      })
    );
  }

  return lines.length > 0 ? lines : null;
}

/**
 * Render the subagent tool's final state in scrollback: header, pipeline tree,
 * optional raw `full output:` (verbose) and `response summary:` sections, and
 * an error block. The summary prefers each stage's `contextSummary`, falling
 * back to `taskResult`; configured output caps apply to every digest.
 */
export function renderSubagentFinalBlock(
  content: string,
  result: { status: string; error?: string; output?: unknown } | undefined,
  status: ToolCallRenderInfo['status'],
  elapsed?: number,
  stageSummaries?: readonly SubagentStageSummary[],
  colors?: {
    getStageInputColor?: (stageName: string) => (text: string) => string;
    getStageOutputColor?: (stageName: string) => (text: string) => string;
    /**
     * Display knobs from /verbose config. Tests that don't pass this fall
     * back to disk via getVerboseDisplay() — same pattern as renderMessageToText.
     */
    display?: VerboseDisplayConfig;
    /** Filter override for the `subagent` output gate; reads disk when omitted. */
    filtersOverride?: readonly string[];
    /** Static scrollback obeys persistOutput; live output remains expandable. */
    isStatic?: boolean;
    theme?: RenderTheme;
    /** Active glyph set, threaded to each stage's markdown body. */
    glyphs?: Glyphs;
    /** Running-tail spinner glyph (see STATUS-SLOT CONTRACT in tools.ts). */
    runningSpinner?: string;
    /** Paints a yellow ' ...' tail when this tool awaits approval; precedence over spinner. */
    awaitingApproval?: boolean;
    /** User denied at the prompt → tail is `DENIED` not `FAILED`. */
    rejected?: boolean;
  }
): string {
  const display = colors?.display ?? getVerboseDisplay();
  const sub = display.subagent;
  const cols = Math.max(
    40,
    (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const lines: string[] = [];
  const g = resolveGlyphs(colors?.glyphs);
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  const outputColor = (name: string): ((text: string) => string) =>
    colors?.getStageOutputColor?.(name) ?? responseChip;

  // Flip FAILED → DENIED when the user rejected: either the explicit flag, or
  // the backend's canonical "denied by the user" text on replayed denials.
  const errText = result?.status === 'error' ? (result.error ?? '') : '';
  const wasDeniedByUser =
    !!colors?.rejected ||
    (typeof errText === 'string' &&
      /denied by the user|rejected because the arguments supplied are forbidden/i.test(
        errText
      ));
  const tail =
    status === 'error'
      ? wasDeniedByUser
        ? chalk.red(' DENIED')
        : chalk.red(' FAILED')
      : status === 'done'
        ? display.showElapsed && elapsed != null
          ? chalk.dim(` ${formatElapsed(elapsed)}`)
          : chalk.dim(' done')
        : status === 'cancelled'
          ? // Must precede the running fallbacks: a cancelled subagent is
            // finished, else the append-only row sticks on `subagent ...`.
            chalk.yellow(` ${g.cross} cancelled`)
          : colors?.awaitingApproval
            ? chalk.yellow(' ...')
            : colors?.runningSpinner
              ? ` ${colors.runningSpinner}`
              : chalk.dim(' ...');
  lines.push(`${chalk.bold('subagent')}${tail}`);

  let task: string | null = null;
  let stages: SubagentStage[] = [];
  try {
    const args = JSON.parse(content);
    if (typeof args.task === 'string') task = args.task;
    if (Array.isArray(args.stages)) stages = args.stages;
  } catch {
    // args unparsable — fall through; the error block below still renders.
  }
  // No standalone `task:` line (duplicates the {task} substitution below);
  // `task` is parsed above only for that substitution.
  const orderedStageSummaries = orderSubagentStageItems(
    stageSummaries ?? [],
    stages.map((stage, index) => stage.name || `stage-${index + 1}`)
  );

  if (sub.pipeline && stages.length > 0) {
    lines.push(
      ...renderPipelineStages(stages, task, {
        inputColor,
        cols,
        theme: colors?.theme,
        glyphs: colors?.glyphs,
        showRoles: sub.roles,
        showDeps: sub.deps,
        showPrompts: sub.prompts,
      })
    );
  }

  const finished = status === 'done' && result?.status !== 'error';
  const terminal =
    status === 'done' || status === 'error' || status === 'cancelled';
  const showFinishedDigests =
    finished && (!colors?.isStatic || display.persistOutput);
  const showTerminalDigests =
    terminal && (!colors?.isStatic || display.persistOutput);
  const hasPlainResponses = orderedStageSummaries.some(
    (summary) => summary.kind === 'response'
  );
  const showSubagentOutput = shouldShowToolOutput(
    'subagent',
    colors?.filtersOverride
  );

  // Preserve valid stage responses when a sibling makes the aggregate fail.
  if (hasPlainResponses && showTerminalDigests && showSubagentOutput) {
    const responseStages = orderedStageSummaries
      .filter(
        (s) => s.kind === 'response' && (s.taskResult ?? '').trim().length > 0
      )
      .map((s) => ({ stageName: s.stageName, body: s.taskResult }));
    if (responseStages.length > 0) {
      lines.push(
        ...renderDigestSection(chalk.bold('  response:'), responseStages, {
          chipFn: (n) => chalk.bold(inputColor(n)(`${g.arrowRight} ${n}`)),
          cols,
          glyphs: colors?.glyphs,
          outputMaxLines: display.outputMaxLines,
          outputMaxChars: display.outputMaxChars,
        })
      );
    }
  }

  // Verbose mode (subagent passes the filter): surface the FULL per-stage
  // taskResult with red ▸ chips — what the parent literally received before
  // the joiner discarded it. Order is pipeline → raw → summary so the eye
  // lands on the digest last. Plain responses already rendered above.
  const showRawSection =
    showFinishedDigests &&
    orderedStageSummaries.length > 0 &&
    showSubagentOutput;
  if (showRawSection) {
    const rawStages = orderedStageSummaries
      .filter(
        (s) => s.kind !== 'response' && (s.taskResult ?? '').trim().length > 0
      )
      .map((s) => ({ stageName: s.stageName, body: s.taskResult }));
    if (rawStages.length > 0) {
      lines.push(
        ...renderDigestSection(chalk.red.bold('  full output:'), rawStages, {
          chipFn: (n) => chalk.red.bold(`${g.arrowRight} ${n}`),
          cols,
          glyphs: colors?.glyphs,
          outputMaxLines: display.outputMaxLines,
          outputMaxChars: display.outputMaxChars,
        })
      );
    }
  }

  // v2 summary digests (kind undefined: contextSummary/taskResult). Plain
  // responses are handled above, so this path is summaries-only now.
  const summaryStages = orderedStageSummaries.filter(
    (summary) => summary.kind !== 'response'
  );
  if (sub.responses && showFinishedDigests && summaryStages.length > 0) {
    lines.push(
      ...renderSubagentResponseSummaryLines(summaryStages, cols, {
        getStageOutputColor: outputColor,
        glyphs: colors?.glyphs,
        outputMaxLines: display.outputMaxLines,
        outputMaxChars: display.outputMaxChars,
      })
    );
  }

  // Errors still surface.
  if (result?.status === 'error') {
    const errText = result.error ?? extractSubagentOutput(result) ?? '';
    if (errText) {
      const indent = '    ';
      const avail = Math.max(
        20,
        cols - visibleWidth(`${indent}${g.lineVertical} `) - 1
      );
      lines.push(chalk.dim(`    ${g.cornerTopLeft}${g.lineHorizontal} error:`));
      for (const raw of errText.split('\n')) {
        const chunks = wrapAtWords(raw, avail, avail);
        if (chunks.length === 0) {
          lines.push(`${indent}${chalk.red(g.lineVertical)}`);
          continue;
        }
        for (const chunk of chunks) {
          lines.push(`${indent}${chalk.red(`${g.lineVertical} ${chunk}`)}`);
        }
      }
      lines.push(chalk.dim(`    ${g.cornerBottomLeft}${g.lineHorizontal}`));
    }
  }

  return lines.join('\n');
}
export function renderSystemError(message: string): string {
  // Only red the first line's `error:` prefix — wrapping the rest in chalk.red
  // would clobber embedded chalk codes on per-item lines below the header.
  const [first, ...rest] = message.split('\n');
  const head = chalk.red(`error: ${first ?? ''}`);
  if (rest.length === 0) return head;
  return [head, ...rest].join('\n');
}

export function renderSystemInfo(message: string): string {
  return chalk.dim(message);
}

export interface TurnSummaryInfo {
  meteringUsage: Array<{ value: number; unit: string; unitPlural: string }>;
  durationMs?: number;
}

export function renderTurnSummary(info: TurnSummaryInfo): string {
  const g = getActiveGlyphs();
  const parts = info.meteringUsage.map(
    (u) => `${u.value} ${u.value === 1 ? u.unit : u.unitPlural}`
  );
  const duration =
    info.durationMs != null ? ` • ${formatDuration(info.durationMs)}` : '';
  return chalk.dim.italic(`${parts.join(` ${g.smallDot} `)}${duration}`);
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(0)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`;
}

export interface MessageLike {
  id: string;
  role: 'user' | 'model' | 'tool_use' | 'system';
  content: string;
  name?: string;
  isQuestion?: boolean;
  isFinished?: boolean;
  result?: { status: string; error?: string; output?: unknown };
  /** `'rejected'` when the user denied the call. Kept as `string` (like
   *  result.status) so the store's MessageType stays structurally assignable
   *  without importing its enum. Distinct from result.status 'error'/
   *  'cancelled' — a rejected call may carry no `result` at all. */
  status?: string;
  /** ACP tool kind; write/read detection falls back to this for engines (v3/KAS)
   *  that send friendly titles absent from WRITE_TOOLS/READ_TOOL_NAMES. */
  kind?: string;
  /** MCP server hosting this tool (`_meta.kiro.mcpServerName`); the sole reliable
   *  "is MCP" signal for the `mcp` verbosity category, since the tool `name` is
   *  the bare (server-prefix-stripped) tool name. */
  mcpServerName?: string;
  origin?: ToolCallOrigin;
  originalTitle?: string;
  success?: boolean;
  standalone?: boolean;
  agentName?: string;
  startTime?: number;
  finishTime?: number;
  /** Model's freeform reasoning, shown above its reply when
   *  showThinkingContent is on. Distinct from `purpose` (per-tool-call why). */
  thinking?: string;
  /** Per-tool "why" preserved verbatim from `__tool_use_purpose`, captured at
   *  the ACP boundary before per-shape synthesis rebuilds `content` and drops
   *  it for edit-kind tools. Primary source for extractToolReasoning. */
  purpose?: string;
  /** `true` when this Model message carries `!` shell-escape output, not agent
   *  inference — routes to renderShellOutputBlock (`! ` gutter) and bypasses
   *  markdown so shell `*`/`_` aren't styled. */
  shellOutput?: boolean;
  /** Normalized denial detail for a blocked tool call (infra-safety override or
   *  permission-policy deny), derived by deriveToolDenial in the store. When
   *  set, the tool renderer appends a "Blocked by …" line — lite-mode parity
   *  with the full TUI's ToolDenialDetails card. */
  denial?: ToolDenial;
}

export interface SubagentStageSummary {
  stageName: string;
  /** KAS emits plain subagent responses; V2 emits synthesized summaries. */
  kind?: 'summary' | 'response';
  /** Compressed digest from the stage's `summary` tool call, harvested off
   *  the inner message (the agent_crew joiner discards it before the parent's
   *  combined output). May be empty — render falls back to taskResult. */
  contextSummary: string;
  /** Fallback when contextSummary is empty. */
  taskResult: string;
}

export interface RenderContext {
  /** Tool-call awaiting approval; its scrollback entry suppresses the diff
   *  (already shown in the approval prompt above). */
  pendingApprovalToolCallId?: string | null;
  termCols?: number;
  /** Per-invocation stage summaries keyed by parent `subagent` tool id;
   *  feeds renderSubagentFinalBlock's compact "Summary of findings". */
  subagentSummariesById?: Map<string, SubagentStageSummary[]>;
  /** Stage → per-agent input color for `[stage]` chips; neutral fallback. */
  getStageInputColor?: (stageName: string) => (text: string) => string;
  /** Brighter shade of the same color for `▸ stage` response chips. */
  getStageOutputColor?: (stageName: string) => (text: string) => string;
  /** /verbose display knobs; read from disk when omitted (tests override). */
  display?: VerboseDisplayConfig;
  /** Filter override for shouldShowToolOutput; the /verbosity preview passes
   *  a draft list so toggles reflect without touching saved config. */
  filtersOverride?: readonly string[];
  /** /theme accessors; falls back to kiroDark defaults for pure contexts. */
  theme?: RenderTheme;
  /** Agent name → footer chip color, so the role tag matches the footer. */
  getAgentTagColor?: (agentName: string) => (text: string) => string;
  /** Spinner glyph for the running status slot. Set only on the live path;
   *  left unset on the static path so flushed rows show settled status, not a
   *  frozen spinner. */
  runningSpinner?: string;
  /** True while baking immutable scrollback rows. */
  isStatic?: boolean;
  /** Active glyph set (Unicode/ASCII); defaults to UNICODE_GLYPHS. */
  glyphs?: Glyphs;
}

/** True when this exact tool call is the one blocking on a user approval. */
function isAwaitingApproval(msg: MessageLike, ctx: RenderContext): boolean {
  return (
    !!ctx.pendingApprovalToolCallId && ctx.pendingApprovalToolCallId === msg.id
  );
}

/** The output bar appended after a tool body (errors bypass the filter check). */
function verboseOutputSuffix(
  msg: MessageLike,
  display: VerboseDisplayConfig,
  ctx: RenderContext
): string {
  return renderVerboseOutput(
    msg.name || '',
    msg.result,
    display.outputMaxLines,
    ctx.filtersOverride,
    display.outputMaxChars,
    ctx.termCols,
    ctx.glyphs,
    isMcpMessage(msg)
  );
}

/** Render any message type to a plain text string for Static output. */
export function renderMessageToText(
  msg: MessageLike,
  mainAgentName?: string,
  ctx: RenderContext = {}
): string {
  switch (msg.role) {
    case 'user':
      return renderUserMessage(msg.content, ctx.theme);

    case 'model': {
      if (isErrorContent(msg.content)) {
        return renderSystemError(msg.content);
      }
      // Shell-escape output bypasses the agent/thinking/markdown path: no
      // `Kiro:` tag, no thinking block, and no markdown (raw bash output's
      // `*`/`_` would otherwise render as italic).
      if (msg.shellOutput) {
        return renderShellOutputBlock(msg.content, ctx.theme, ctx.termCols);
      }
      // Prefer the message's own agentName (subagent stages); fall back to main.
      const display = ctx.display ?? getVerboseDisplay();
      const agentText = renderAgentMessage(
        msg.content,
        msg.agentName ?? mainAgentName,
        ctx.theme,
        ctx.termCols,
        ctx.getAgentTagColor,
        ctx.glyphs
      );
      // Thinking block above the spoken text, gated by showThinkingContent.
      const thinkingBlock =
        display.showThinkingContent && msg.thinking
          ? renderThinkingBlock(
              msg.thinking,
              ctx.theme,
              ctx.termCols,
              ctx.glyphs
            )
          : '';
      if (!thinkingBlock) return agentText;
      if (!agentText) return thinkingBlock;
      // Blank row so the bottom rule doesn't glue to the `Kiro:` line.
      return thinkingBlock + '\n\n' + agentText;
    }

    case 'tool_use': {
      // Append the "Blocked by …" denial block to whatever this branch renders
      // (it has many return points). Applied at each return via withDenial so a
      // denied/cancelled/errored override or policy deny surfaces its rule +
      // tool under the call — lite parity with the full TUI's ToolDenialDetails.
      const withDenial = (text: string): string =>
        msg.denial
          ? text + '\n' + renderToolDenial(msg.denial, ctx.theme)
          : text;
      const isRejected = msg.status === 'rejected';
      const status: ToolCallRenderInfo['status'] = isRejected
        ? 'error'
        : msg.result
          ? msg.result.status === 'error'
            ? 'error'
            : msg.result.status === 'cancelled'
              ? 'cancelled'
              : 'done'
          : msg.isFinished
            ? 'done'
            : 'running';
      const showAgent = msg.agentName && msg.agentName !== mainAgentName;
      const agentPrefix = showAgent ? `[${msg.agentName}] ` : undefined;
      const dispatch = resolveLiteToolRenderDispatch(
        msg.name || '',
        msg.kind,
        msg.origin
      );
      const display = ctx.display ?? getVerboseDisplay();

      // Subagent tool: one canonical block per pipeline run, shown in full
      // (it's what the parent agent sees). Per-stage tool calls are hidden
      // from the chat log (they live in the footer activity strip).
      if (
        dispatch.mode === 'subagent' &&
        isParentSubagentTool(msg.name, msg.origin)
      ) {
        const elapsed =
          msg.startTime && msg.finishTime
            ? msg.finishTime - msg.startTime
            : undefined;
        const stageSummaries = ctx.subagentSummariesById?.get(msg.id);
        return renderSubagentFinalBlock(
          msg.content,
          msg.result,
          status,
          elapsed,
          stageSummaries,
          {
            getStageInputColor: ctx.getStageInputColor,
            getStageOutputColor: ctx.getStageOutputColor,
            display,
            filtersOverride: ctx.filtersOverride,
            isStatic: ctx.isStatic,
            theme: ctx.theme,
            glyphs: ctx.glyphs,
            rejected: isRejected,
            runningSpinner: ctx.runningSpinner,
            // Fires only when the parent subagent tool itself awaits approval;
            // stage approvals go through the footer activity strip instead.
            awaitingApproval: isAwaitingApproval(msg, ctx),
          }
        );
      }

      // Task list tool (todo_list / task / todo): render a per-command
      // structured body instead of the raw args JSON; display name → `tasks`
      // to match the tray + /verbose. Falls through to generic on malformed
      // args so a schema change still shows something.
      if (dispatch.mode === 'task') {
        const taskBlock = formatTaskToolBody(
          msg.content,
          ctx.termCols,
          ctx.glyphs
        );
        if (taskBlock) {
          const reasoning = display.showToolReasoning
            ? extractToolReasoning(msg.content, msg.purpose)
            : undefined;
          const info: ToolCallRenderInfo = {
            // Override the wire name (legacy alias `todo_list`) → "tasks".
            name: 'tasks',
            status,
            description: reasoning,
            inlineArg: chalk.dim(taskBlock.command),
            agentPrefix,
            rejected: isRejected,
            elapsed:
              display.showElapsed && msg.startTime && msg.finishTime
                ? msg.finishTime - msg.startTime
                : undefined,
            runningSpinner: ctx.runningSpinner,
            awaitingApproval: isAwaitingApproval(msg, ctx),
          };
          const header = renderToolCall(info, ctx.theme);
          // `off` mode gets the bare header; other modes show the full body
          // (the structured task view is the point — don't collapse to a chip).
          if (
            display.toolArgsMode === 'off' ||
            taskBlock.bodyLines.length === 0
          ) {
            return header;
          }
          // argsMaxLines bounds body height; marker counts dropped lines.
          const capped = applyLineCap(
            taskBlock.bodyLines,
            display.argsMaxLines,
            (n) => chalk.dim(`  ... (truncated; +${n} more lines)`)
          );
          const body = header + '\n' + capped.join('\n');
          // Suppress the output bar on success (the tray is authoritative);
          // on error, surface the body so the user sees the cause.
          if (msg.result?.status !== 'error') return body;
          return body + verboseOutputSuffix(msg, display, ctx);
        }
        // taskBlock === null — args malformed; fall through to generic.
      }

      const isQuestion = msg.isQuestion === true;
      // Reasoning slot (gated by showToolReasoning) only ever surfaces the
      // agent's real `__tool_use_purpose`, never a synthesized args one-liner
      // — so purple always means "the agent reasoned about this call".
      const inlineArg =
        display.toolArgsMode === 'inline'
          ? extractInlineArg(
              msg.name || '',
              msg.content,
              display.argsMaxChars,
              msg.kind,
              msg.origin
            )
          : undefined;
      const reasoning = display.showToolReasoning
        ? extractToolReasoning(msg.content, msg.purpose)
        : undefined;

      const info: ToolCallRenderInfo = {
        name: isQuestion
          ? 'Question:'
          : toolDisplayName(msg.name || 'unknown', msg.kind, msg.origin),
        inlineArg: isQuestion ? stripInlineMarkdown(msg.name || '') : inlineArg,
        isTrivial:
          !isQuestion && isTrivialTool(msg.name || '', msg.kind, msg.origin),
        status,
        description: reasoning,
        agentPrefix,
        rejected: isRejected,
        elapsed:
          display.showElapsed && msg.startTime && msg.finishTime
            ? msg.finishTime - msg.startTime
            : undefined,
        runningSpinner: ctx.runningSpinner,
        // Pending-approval target: running slot flips to a yellow ' ...'
        // (the agent isn't progressing, so the spinner would lie).
        awaitingApproval: isAwaitingApproval(msg, ctx),
      };
      if (dispatch.isWriteWithDiff && msg.content) {
        // Suppress the diff when this call is awaiting approval (already shown
        // in the prompt) or the user turned off the Write-diffs toggle; the
        // header row still records that the write fired.
        const suppressDiff =
          isAwaitingApproval(msg, ctx) || !display.showWriteDiffs;
        // Write diffs render in full (the payload being reviewed), including
        // denied calls — falling back to a raw args tree post-deny reads
        // worse than the diff. No trailing success line; errors still surface
        // below via renderVerboseOutput (which bypasses the filter check).
        const writeRender = renderWriteToolCall(info, msg.content, {
          suppressDiff,
          termCols: ctx.termCols,
          theme: ctx.theme,
        });
        if (msg.result?.status !== 'error') return withDenial(writeRender);
        return withDenial(writeRender + verboseOutputSuffix(msg, display, ctx));
      }
      // Read tools render a structured body (path header + numbered,
      // highlighted lines); the output bar is skipped (body shows content).
      // Filters still gate it — fall through to the bare line when read isn't enabled.
      if (
        dispatch.isRead &&
        msg.content &&
        !isRejected &&
        shouldShowToolOutput(
          msg.name || '',
          ctx.filtersOverride,
          isMcpMessage(msg)
        )
      ) {
        return withDenial(
          renderReadToolCall(info, msg.content, msg.result, {
            termCols: ctx.termCols,
            maxLines: display.outputMaxLines,
            maxCharsPerLine: display.outputMaxChars,
            theme: ctx.theme,
            glyphs: ctx.glyphs,
          })
        );
      }
      const toolLine = renderToolCall(info, ctx.theme);
      // Block mode: full key:value tree under the name. Inline/off: nothing
      // (the chip is all the user gets).
      if (display.toolArgsMode === 'block') {
        // perValueLineCap=null so the block-level applyLineCap below is the
        // single cap (P438130055): a per-value clamp would emit its own
        // marker that applyLineCap then miscounts as one row.
        const argsLines = formatToolArgLines(
          msg.name || '',
          msg.content,
          undefined,
          display.argsMaxChars,
          null
        );
        if (argsLines && argsLines.length > 0) {
          const capped = applyLineCap(argsLines, display.argsMaxLines, (n) =>
            chalk.dim(`  ... (truncated; +${n} more lines)`)
          );
          return withDenial(
            toolLine +
              '\n' +
              capped.join('\n') +
              verboseOutputSuffix(msg, display, ctx)
          );
        }
      }
      return withDenial(toolLine + verboseOutputSuffix(msg, display, ctx));
    }

    case 'system':
      return msg.success !== false
        ? renderSystemInfo(msg.content)
        : renderSystemError(msg.content);

    default:
      return '';
  }
}

/** Which fixture-set the /verbosity menu wants previewed (one per submenu). */
export type VerbosityPreviewKey =
  | 'top'
  | 'density'
  | 'tool'
  | 'subagent'
  | 'output'
  | 'truncation:args'
  | 'truncation:output';

/** Build a finished, successful tool-use preview fixture, hoisting the shared
 *  role/startTime/isFinished/result-envelope boilerplate the literals repeat. */
function toolFixture(f: {
  id: string;
  name: string;
  content: Record<string, unknown>;
  output?: string;
  finishTime: number;
}): MessageLike {
  return {
    id: f.id,
    role: 'tool_use',
    name: f.name,
    content: JSON.stringify(f.content),
    result: { status: 'success', output: f.output ?? '' },
    startTime: 0,
    finishTime: f.finishTime,
    isFinished: true,
  };
}

const PREVIEW_FIXTURE_SHELL = toolFixture({
  id: 'preview-shell',
  name: 'shell',
  content: {
    command: 'git status',
    __tool_use_purpose: 'check working tree state before commit',
  },
  output: [
    'On branch feature/lite-tui-mode',
    'Changes not staged for commit:',
    '\tmodified:   packages/tui/src/lite/render.ts',
  ].join('\n'),
  finishTime: 1240,
});

const PREVIEW_FIXTURE_READ = toolFixture({
  id: 'preview-read',
  name: 'fs_read',
  content: {
    operations: [{ path: '/etc/hosts', limit: 50 }],
    __tool_use_purpose: 'inspect hostnames for the dev cluster',
  },
  output: ['127.0.0.1 localhost', '127.0.1.1 cloud-desktop'].join('\n'),
  finishTime: 18,
});

// Long pattern argument so the preview shows argsMaxChars clipping one value.
const PREVIEW_FIXTURE_GREP = toolFixture({
  id: 'preview-grep',
  name: 'grep',
  content: {
    pattern: 'legacy_auth_middleware|legacyAuthMiddleware|LegacyAuthMiddleware',
    path: 'packages/tui/src',
    __tool_use_purpose: 'enumerate every flavor of the legacy middleware name',
  },
  output: [
    'packages/tui/src/api/auth/legacy.ts:42:    legacy_auth_middleware,',
    'packages/tui/src/middleware/legacy.ts:1:export const legacy_auth_middleware = (',
  ].join('\n'),
  finishTime: 96,
});

/** Long-output shell fixture (>5 rows) so outputMaxLines=5 is visibly clipped. */
const PREVIEW_FIXTURE_LONG_OUTPUT = toolFixture({
  id: 'preview-long',
  name: 'shell',
  content: {
    command: 'cat package.json',
    __tool_use_purpose: 'inspect dependencies',
  },
  output: [
    '{',
    '  "name": "@kiro/tui",',
    '  "version": "0.1.0",',
    '  "type": "module",',
    '  "dependencies": {',
    '    "ink": "^4.0.0",',
    '    "react": "^18.2.0"',
    '  }',
    '}',
  ].join('\n'),
  finishTime: 32,
});

// `mcp__*` name so the `mcp` filter category gates it.
const PREVIEW_FIXTURE_MCP = toolFixture({
  id: 'preview-mcp',
  name: 'mcp__nova-memory-mcp__recall',
  content: {
    query: 'legacy auth middleware migration',
    __tool_use_purpose: 'check prior context for the migration plan',
  },
  output: [
    '2 memories matched:',
    '  · 2026-04-02 — legacy_auth_middleware deprecation announcement',
  ].join('\n'),
  finishTime: 240,
});

// Exercises the diff renderer (write-tool path) in the preview.
const PREVIEW_FIXTURE_WRITE = toolFixture({
  id: 'preview-write',
  name: 'fs_write',
  content: {
    command: 'str_replace',
    path: 'packages/tui/src/middleware/legacy.ts',
    old_str: 'export const legacy_auth_middleware = (req, res, next) => {',
    new_str: 'export const legacyAuthMiddleware = (req, res, next) => {',
    __tool_use_purpose: 'rename the legacy middleware export to camelCase',
  },
  finishTime: 64,
});

// Agent prose so the preview isn't wall-to-wall tool blocks.
const PREVIEW_FIXTURE_AGENT: MessageLike = {
  id: 'preview-agent',
  role: 'model',
  content:
    "Found four call sites for the legacy middleware. I'll rename the export to camelCase, then fix the import sites in order: edge handler, API auth, migration script.",
};

const PREVIEW_SUBAGENT_CONTENT = {
  task: 'find every place the legacy auth middleware is wired up',
  stages: [
    {
      name: 'scan',
      role: 'searcher',
      prompt_template:
        'Search the codebase for references to legacy_auth_middleware. Return a list of file:line locations.',
      depends_on: [],
    },
    {
      name: 'summarize',
      role: 'synthesizer',
      prompt_template:
        'Given the scan results, group call sites by component and summarize the migration impact.',
      depends_on: ['scan'],
    },
  ],
};

export const PREVIEW_SUBAGENT_SUMMARIES: SubagentStageSummary[] = [
  {
    stageName: 'scan',
    contextSummary:
      '4 call sites: api/auth/, edge/handlers/, middleware/legacy.ts, scripts/migrate.ts',
    taskResult: [
      'api/auth/legacy.ts:42 — imports legacy_auth_middleware',
      'middleware/legacy.ts:1 — defines the export',
    ].join('\n'),
  },
  {
    stageName: 'summarize',
    contextSummary:
      'Three production paths (API, edge, scripts). Migration unblocks the new session store.',
    taskResult:
      'Three production paths use legacy_auth_middleware: the public-facing API, the edge login handler, and the offline migration script. All three need updating before the new session-token store can ship.',
  },
];

const PREVIEW_FIXTURE_SUBAGENT = toolFixture({
  id: 'preview-subagent',
  name: 'subagent',
  content: PREVIEW_SUBAGENT_CONTENT,
  finishTime: 4200,
});

const PREVIEW_FIXTURE_USER: MessageLike = {
  id: 'preview-user',
  role: 'user',
  content: 'find the legacy auth middleware',
};

const GENERIC_PREVIEW_MIX: MessageLike[] = [
  PREVIEW_FIXTURE_USER,
  PREVIEW_FIXTURE_READ,
  PREVIEW_FIXTURE_WRITE,
  PREVIEW_FIXTURE_GREP,
  PREVIEW_FIXTURE_MCP,
  PREVIEW_FIXTURE_LONG_OUTPUT,
  PREVIEW_FIXTURE_AGENT,
  PREVIEW_FIXTURE_SUBAGENT,
];

/** 50-key args fixture for truncation:args (one row per key → tight cap clips). */
function buildTruncationArgsFixture(): MessageLike {
  const args: Record<string, unknown> = {
    __tool_use_purpose: 'demo a tool with many args',
  };
  for (let i = 1; i <= 50; i++) {
    args[`key_${String(i).padStart(2, '0')}`] = `value-${i}`;
  }
  return toolFixture({
    id: 'preview-trunc-args',
    name: 'mcp__demo__many_args',
    content: args,
    finishTime: 50,
  });
}

/** 60-line output fixture for truncation:output. Picks a tool from the user's
 *  enabled categories (shell→read→grep→mcp) so the cap demo uses a tool they
 *  actually see; falls back to shell. */
function buildTruncationOutputFixture(
  filters: readonly string[] = ['all']
): MessageLike {
  const lines: string[] = [];
  for (let i = 1; i <= 60; i++) {
    lines.push(
      `line ${String(i).padStart(2, '0')}: lorem ipsum dolor sit amet`
    );
  }
  // Pick a tool from the user's enabled categories so the demo uses one they
  // actually see. Unmatched filters fall back to shell — renderVerbosityPreview
  // then widens the override list with shell so the bar still shows.
  const SHELL = {
    name: 'shell',
    command: 'cat fixture.txt',
    purpose: 'demo a tool with long output',
  };
  const CANDIDATES: [
    string,
    { name: string; command: string; purpose: string },
  ][] = [
    ['shell', SHELL],
    [
      'read',
      { name: 'fs_read', command: '', purpose: 'demo a long file read' },
    ],
    [
      'grep',
      { name: 'grep', command: '', purpose: 'demo a grep with many matches' },
    ],
    [
      'mcp',
      {
        name: 'mcp__demo__long-output',
        command: '',
        purpose: 'demo an MCP tool with long output',
      },
    ],
  ];
  const pick = filters.includes('all')
    ? SHELL
    : (CANDIDATES.find(([cat]) => filters.includes(cat))?.[1] ?? SHELL);
  // fs_read uses operations:[{path}]; others take a generic command/query.
  const content: Record<string, unknown> =
    pick.name === 'fs_read'
      ? {
          operations: [{ path: '/tmp/fixture.txt' }],
          __tool_use_purpose: pick.purpose,
        }
      : pick.name.startsWith('mcp__')
        ? { query: 'fixture', __tool_use_purpose: pick.purpose }
        : pick.name === 'grep'
          ? { pattern: 'fixture', path: '.', __tool_use_purpose: pick.purpose }
          : { command: pick.command, __tool_use_purpose: pick.purpose };
  return toolFixture({
    id: 'preview-trunc-output',
    name: pick.name,
    content,
    output: lines.join('\n'),
    finishTime: 80,
  });
}

/** Section-spacing for preview rendering; delegates to needsLeadingBlankByRole. */
function previewNeedsLeadingBlank(
  prev: MessageLike,
  next: MessageLike
): boolean {
  return needsLeadingBlankByRole(prev.role, next.role);
}

/**
 * The fixture message set + widened filters for a preview key. Single source
 * of truth shared by the lite text preview ({@link renderVerbosityPreview}) and
 * the TUI component preview, so both surfaces show the same synthetic scrollback.
 */
export function getVerbosityPreviewFixtures(
  key: VerbosityPreviewKey,
  filters: readonly string[]
): { messages: MessageLike[]; previewFilters: readonly string[] } {
  // truncation:output: pick the fixture tool first, then widen filters only
  // if the user's filters don't already cover it.
  let outputFixture: MessageLike | null = null;
  if (key === 'truncation:output') {
    outputFixture = buildTruncationOutputFixture(filters);
  }
  const previewFilters: readonly string[] =
    key === 'truncation:output' && outputFixture
      ? widenFiltersForPreview(filters, outputFixture.name ?? 'shell')
      : filters;

  // truncation:output reuses outputFixture (built above) so previewFilters
  // stays aligned; truncation:args puts the 50-key fixture last so the cap
  // impact is what the eye lands on.
  const PREVIEW_SETS: Record<VerbosityPreviewKey, () => MessageLike[]> = {
    // Generic mix — one of each kind the user will encounter.
    top: () => GENERIC_PREVIEW_MIX,
    density: () => GENERIC_PREVIEW_MIX,
    tool: () => GENERIC_PREVIEW_MIX,
    // One tool per category so filter toggles produce visible changes.
    output: () => [
      PREVIEW_FIXTURE_USER,
      PREVIEW_FIXTURE_SHELL,
      PREVIEW_FIXTURE_READ,
      PREVIEW_FIXTURE_GREP,
      PREVIEW_FIXTURE_MCP,
      PREVIEW_FIXTURE_AGENT,
      PREVIEW_FIXTURE_SUBAGENT,
    ],
    subagent: () => [PREVIEW_FIXTURE_USER, PREVIEW_FIXTURE_SUBAGENT],
    'truncation:args': () => [
      PREVIEW_FIXTURE_GREP,
      PREVIEW_FIXTURE_SHELL,
      buildTruncationArgsFixture(),
    ],
    'truncation:output': () => (outputFixture ? [outputFixture] : []),
  };
  return { messages: PREVIEW_SETS[key](), previewFilters };
}

export function renderVerbosityPreview(
  key: VerbosityPreviewKey,
  display: VerboseDisplayConfig,
  filters: readonly string[],
  options: { expanded?: boolean; theme?: RenderTheme } = {}
): string {
  const { messages, previewFilters } = getVerbosityPreviewFixtures(
    key,
    filters
  );
  const outputFixture =
    key === 'truncation:output' ? (messages[0] ?? null) : null;

  const g = getActiveGlyphs();
  const ctx: RenderContext = {
    display,
    filtersOverride: previewFilters,
    subagentSummariesById: new Map([
      [PREVIEW_FIXTURE_SUBAGENT.id, PREVIEW_SUBAGENT_SUMMARIES],
    ]),
    theme: options.theme,
    // Honor chat.allowAsciiArt in the preview so it mirrors real scrollback.
    glyphs: g,
  };

  const blocks: string[] = [];

  // Nudge only when we actually widened the user's filters to show the bar.
  if (
    key === 'truncation:output' &&
    outputFixture &&
    !shouldShowToolOutput(outputFixture.name ?? 'shell', filters)
  ) {
    blocks.push(
      chalk.dim(
        `(preview-only: your filters hide output for ${outputFixture.name}. Enable it in /verbosity ${g.arrow} Show output to see this cap in real scrollback.)`
      )
    );
  }

  let prevMsg: MessageLike | null = null;
  for (const msg of messages) {
    const text = renderMessageToText(msg, 'Kiro', ctx);
    if (!text) continue;
    const blank = prevMsg ? previewNeedsLeadingBlank(prevMsg, msg) : false;
    blocks.push(blank ? `\n${text}` : text);
    prevMsg = msg;
  }
  const joined = blocks.join('\n');
  // Expanded mode skips the clip — the pane viewer paginates itself.
  if (options.expanded) return joined;
  const lines = joined.split('\n');
  const MAX_PREVIEW_ROWS = 16;
  if (lines.length <= MAX_PREVIEW_ROWS) return joined;
  const head = lines.slice(0, MAX_PREVIEW_ROWS);
  head.push(
    chalk.dim(
      `${g.ellipsis} (preview clipped, +${lines.length - MAX_PREVIEW_ROWS} more rows)`
    )
  );
  return head.join('\n');
}

/**
 * Add the fixture's category to a copy of the user's filters so the
 * truncation:output bar renders (never mutates saved config).
 */
function widenFiltersForPreview(
  filters: readonly string[],
  needTool: string
): readonly string[] {
  if (filters.includes('all')) return filters;
  if (shouldShowToolOutput(needTool, filters)) return filters;
  // Widen by category so other tools in that category are covered too.
  const cat = categorize(needTool);
  if (cat == null) return [...filters, needTool];
  return [...filters, cat];
}

// ---------------------------------------------------------------------------
// Unified diff renderer (lite-mode write/edit tool calls). Row layout is
// `<line#> <gutter> <highlighted code>`; added/removed rows carry a bg tint to
// the terminal edge. Long lines wrap with a hanging indent (wrapAnsiLine carries
// SGR state across rows) so continuations align under row 0's body.
// ---------------------------------------------------------------------------

// Legacy hardcoded fallbacks used when no theme is supplied (tests, snapshot
// callers, mid-mount pure-context renders). Kept in lockstep with
// DEFAULT_RENDER_THEME.diffAddedBg / diffRemovedBg so theme-less callers see
// identical output to the pre-theming code. (= chalk.bgHex('#1F2D22'/'#2D1F22').)
const ADDED_BG_OPEN = '\x1b[48;2;31;45;34m';
const REMOVED_BG_OPEN = '\x1b[48;2;45;31;34m';
// Full SGR reset at row end: closes the bg tint AND any unclosed fg/style left
// mid-token when a wrap boundary lands inside a styled span (else cyan etc.
// bleeds into the next row's hanging indent).
const ROW_RESET = '\x1b[0m';

const ADDED_BAR = '#80ffb5';
const REMOVED_BAR = '#ff8080';

interface DiffLine {
  type: 'context' | 'added' | 'removed';
  oldNum: number | null;
  newNum: number | null;
  text: string;
}

export interface RenderUnifiedDiffOpts {
  path?: string;
  /**
   * Suppress the leading `  {path}` header row inside the diff body.
   * The path is still consumed for syntax-highlight language detection.
   * Used by the scrollback caller, where the inline arg chip on the
   * tool-call header already shows the path — printing it again as the
   * first row of the diff body would duplicate it. The approval prompt
   * leaves this off because its surrounding chrome doesn't show the
   * path anywhere else.
   */
  suppressPathHeader?: boolean;
  startLine?: number;
  contextLines?: number;
  termCols?: number;
  /** Supplies bg tint + bar colors; falls back to the legacy constants. */
  theme?: RenderTheme;
}

interface DiffStyling {
  /** Null = theme opted out of bg tints; rows render fg-only (git-style). */
  addedBgOpen: string | null;
  removedBgOpen: string | null;
  addedBarFn: (s: string) => string;
  removedBarFn: (s: string) => string;
  /** Whole-line fg colors for the fg-only mode. */
  addedFgFn: (s: string) => string;
  removedFgFn: (s: string) => string;
}

/**
 * Pull the SGR open sequence from a chalk-style bg wrapper so {@link applyBg}
 * can re-assert it after every full reset cli-highlight emits between tokens.
 * Probes on a single space (not `''`): chalk's empty-input optimization
 * returns `''` even at full truecolor support, which would silently drop
 * every theme bg. Returns `fallback` for non-SGR output or reset-shaped SGR
 * (re-asserting a reset would stack resets and drop the tint).
 */
function extractBgOpen(bgFn: (s: string) => string, fallback: string): string {
  try {
    const styled = bgFn(' ');
    // eslint-disable-next-line no-control-regex
    const m = /^\x1b\[[0-9;]*m/.exec(styled);
    if (!m) return fallback;
    const params = m[0].slice(2, -1); // strip leading "\x1b[" and trailing "m"
    if (params === '' || /^[0;]+$/.test(params)) return fallback;
    return m[0];
  } catch {
    return fallback;
  }
}

function resolveDiffStyling(theme?: RenderTheme): DiffStyling {
  if (!theme) {
    return {
      addedBgOpen: ADDED_BG_OPEN,
      removedBgOpen: REMOVED_BG_OPEN,
      addedBarFn: chalk.hex(ADDED_BAR),
      removedBarFn: chalk.hex(REMOVED_BAR),
      addedFgFn: chalk.green,
      removedFgFn: chalk.red,
    };
  }
  return {
    // A null bg slot is the theme explicitly opting out of tints (kiroSafe);
    // propagate it so renderDiffLine paints fg-only rows instead of falling
    // back to the dark constants (unreadable on light terminals).
    addedBgOpen: theme.diffAddedBg
      ? extractBgOpen(theme.diffAddedBg, ADDED_BG_OPEN)
      : null,
    removedBgOpen: theme.diffRemovedBg
      ? extractBgOpen(theme.diffRemovedBg, REMOVED_BG_OPEN)
      : null,
    addedBarFn: theme.diffAddedBar,
    removedBarFn: theme.diffRemovedBar,
    addedFgFn: theme.diffAddedFg ?? chalk.green,
    removedFgFn: theme.diffRemovedFg ?? chalk.red,
  };
}

export function renderUnifiedDiff(
  oldText: string,
  newText: string,
  opts: RenderUnifiedDiffOpts = {}
): string[] {
  const startLine = opts.startLine ?? 1;
  const contextLines = opts.contextLines ?? 3;
  const termCols = opts.termCols ?? 80;
  // Resolve bg + bar styling once per call so the per-line renderer
  // doesn't have to re-derive it for every row. Theme-driven when
  // opts.theme is supplied; legacy hardcoded constants otherwise.
  const styling = resolveDiffStyling(opts.theme);

  const normOld = oldText.replace(/\n$/, '');
  const normNew = newText.replace(/\n$/, '');

  if (normOld === normNew) return [];

  const changes = diffLines(normOld, normNew);

  const flat: DiffLine[] = [];
  let oldNum = startLine;
  let newNum = startLine;
  for (const change of changes) {
    const lines = change.value.split('\n');
    if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
    for (const line of lines) {
      if (change.added) {
        flat.push({
          type: 'added',
          oldNum: null,
          newNum: newNum++,
          text: line,
        });
      } else if (change.removed) {
        flat.push({
          type: 'removed',
          oldNum: oldNum++,
          newNum: null,
          text: line,
        });
      } else {
        flat.push({ type: 'context', oldNum, newNum, text: line });
        oldNum++;
        newNum++;
      }
    }
  }

  type Hunk = { start: number; end: number };
  const hunks: Hunk[] = [];
  let i = 0;
  while (i < flat.length) {
    if (flat[i]!.type === 'context') {
      i++;
      continue;
    }
    let start = i;
    let backCount = 0;
    while (
      start > 0 &&
      flat[start - 1]!.type === 'context' &&
      backCount < contextLines
    ) {
      start--;
      backCount++;
    }
    let end = i;
    while (end < flat.length) {
      if (flat[end]!.type !== 'context') {
        end++;
        continue;
      }
      let lookAhead = end;
      let ctxRun = 0;
      while (
        lookAhead < flat.length &&
        flat[lookAhead]!.type === 'context' &&
        ctxRun < 2 * contextLines
      ) {
        lookAhead++;
        ctxRun++;
      }
      if (lookAhead < flat.length && flat[lookAhead]!.type !== 'context') {
        end = lookAhead;
        continue;
      }
      end = Math.min(end + contextLines, flat.length);
      break;
    }
    hunks.push({ start, end });
    i = end;
  }

  if (hunks.length === 0) return [];

  const out: string[] = [];

  if (opts.path && !opts.suppressPathHeader) {
    out.push(chalk.dim(`  ${opts.path}`));
  }

  const language = resolveLanguageFromPathLite(opts.path);

  for (let h = 0; h < hunks.length; h++) {
    const hunk = hunks[h]!;
    if (h === 0 && hunk.start > 0) {
      out.push(chalk.dim('  ...'));
    } else if (h > 0) {
      const prev = hunks[h - 1]!;
      if (hunk.start > prev.end) out.push(chalk.dim('  ...'));
    }
    for (let j = hunk.start; j < hunk.end; j++) {
      out.push(renderDiffLine(flat[j]!, termCols, styling, language));
    }
    if (h === hunks.length - 1 && hunk.end < flat.length) {
      out.push(chalk.dim('  ...'));
    }
  }

  let added = 0;
  let removed = 0;
  for (const dl of flat) {
    if (dl.type === 'added') added++;
    else if (dl.type === 'removed') removed++;
  }
  const parts: string[] = [];
  if (added > 0) parts.push(`added ${added} ${added === 1 ? 'line' : 'lines'}`);
  if (removed > 0)
    parts.push(`removed ${removed} ${removed === 1 ? 'line' : 'lines'}`);
  if (parts.length > 0) out.push(chalk.dim(`  ${parts.join(', ')}`));

  return out;
}

/**
 * Wrap `inner` in a bg-tinted block so the bg extends to `termCols`.
 *
 * cli-highlight emits `\x1b[0m` (full reset) between tokens. A naive
 * bg wrapper would have the bg cleared at every reset and re-applied
 * only at the next chalk-styled segment, leaving "stripes" inside the
 * row. We re-assert the bg open code right after every reset so the
 * tint stays continuous.
 */
function applyBg(inner: string, bgOpen: string): string {
  // Re-assert bgOpen after every full reset cli-highlight emits between
  // tokens, else the bg clears mid-row and leaves "stripes".
  // eslint-disable-next-line no-control-regex
  const reasserted = inner.replace(/\x1b\[0m/g, `\x1b[0m${bgOpen}`);
  return `${bgOpen}${reasserted}${ROW_RESET}`;
}

function renderDiffLine(
  dl: DiffLine,
  termCols: number,
  styling: DiffStyling,
  language?: string
): string {
  // Anchor the gutter number on the resulting (new) file for context + added
  // rows so it stays monotonic with adjacent additions; removed rows fall back
  // to oldNum since they have no presence in the new file.
  const numStr =
    dl.type === 'removed'
      ? String(dl.oldNum ?? '').padStart(LINE_NUM_WIDTH)
      : String(dl.newNum ?? '').padStart(LINE_NUM_WIDTH);

  const gutterGlyph =
    dl.type === 'added' ? '+' : dl.type === 'removed' ? '-' : ' ';
  // chalk.bold over the bar color so the +/- glyph stays readable on light
  // pastel bar themes (e.g. kiroLight's #5de89d on near-white bg).
  const gutter =
    dl.type === 'added'
      ? chalk.bold(styling.addedBarFn(gutterGlyph))
      : dl.type === 'removed'
        ? chalk.bold(styling.removedBarFn(gutterGlyph))
        : ' ';

  // linePrefix = 2-space indent + padded line# + space. headWidth adds the
  // gutter cell; continuation rows hang-indent to headWidth so the bg block
  // forms a clean rectangle aligned under row 0's body.
  const linePrefix = `  ${numStr} `;
  const headWidth = visibleWidth(linePrefix) + 1;
  // BODY_INSET sits inside the bg block so the tint reaches the gutter on
  // every row; mirrored on (un-tinted) context rows for consistent alignment.
  const BODY_INSET = '  ';
  const innerCols = Math.max(8, termCols - headWidth - BODY_INSET.length);

  // Null bg = theme opted out of tints (kiroSafe / unknown terminal bg):
  // render git-style, whole line painted with the diff fg color. Syntax
  // highlight is skipped in that mode — cli-highlight's palette assumes a
  // controlled dark bg and is the other unreadable-color source there.
  const bgOpen =
    dl.type === 'added' ? styling.addedBgOpen : styling.removedBgOpen;
  const fgOnly = dl.type !== 'context' && bgOpen === null;

  // Highlight the FULL source line in one shot so cli-highlight's line-at-a-
  // time tokenizer colors tokens correctly even when they later wrap.
  const styled = fgOnly ? dl.text : highlightLineSafe(dl.text, language);

  // wrapAnsiLine splits into visual rows while carrying SGR state across
  // boundaries; each row then gets its own bg block + hanging indent so the
  // tint reaches the terminal edge on every row, including the last.
  const rows = wrapAnsiLine(styled, innerCols, innerCols);
  if (rows.length === 0) rows.push('');

  if (dl.type === 'context') {
    const hangingIndent = ' '.repeat(headWidth + BODY_INSET.length);
    return rows
      .map((row, i) =>
        i === 0
          ? chalk.dim(linePrefix) + gutter + BODY_INSET + chalk.dim(row)
          : hangingIndent + chalk.dim(row)
      )
      .join('\n');
  }

  if (bgOpen === null) {
    const fgFn = dl.type === 'added' ? styling.addedFgFn : styling.removedFgFn;
    const hangingIndent = ' '.repeat(headWidth + BODY_INSET.length);
    return rows
      .map((row, i) =>
        i === 0
          ? chalk.dim(linePrefix) + gutter + BODY_INSET + fgFn(row)
          : hangingIndent + fgFn(row)
      )
      .join('\n');
  }

  // added / removed: each row gets its own bg-tinted block padded to innerCols
  // (bg reaches the right edge). Continuation rows skip the +/- glyph —
  // repeating it would read as a new diff line.
  const hangingIndent = ' '.repeat(headWidth);
  return rows
    .map((row, i) => {
      const rowVis = visibleWidth(row);
      const padCount = Math.max(0, innerCols - rowVis);
      const innerPlain = BODY_INSET + row + ' '.repeat(padCount);
      if (i === 0) {
        return chalk.dim(linePrefix) + gutter + applyBg(innerPlain, bgOpen);
      }
      return hangingIndent + applyBg(innerPlain, bgOpen);
    })
    .join('\n');
}
