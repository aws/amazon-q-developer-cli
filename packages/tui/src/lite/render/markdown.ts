import chalk from 'chalk';
import {
  parseMarkdown,
  parseInlineMarkdown,
  type MarkdownSegment,
} from '../../utils/markdown.js';
import { visibleWidth } from '../../utils/text-width.js';
import { resolveHighlightLanguage } from '../../utils/highlight-languages.js';
import {
  constrainColumnWidths,
  wrapCellText,
  padCell,
  type Alignment,
} from '../../utils/table-layout.js';
import type { Glyphs } from '../../utils/glyphs.js';
import {
  resolveGlyphs,
  resolveTheme,
  brand,
  DEFAULT_USER_TAG,
  type RenderTheme,
} from './theme.js';
import { wrapStyled, stripAnsiQuick, highlightLineSafe } from './text.js';

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
  // Use the active agent's name verbatim (custom agents / swapped via /agent)
  // so the user can tell which persona answered; default "Kiro".
  const tag = agentName && agentName.trim() ? agentName : 'Kiro';
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
  const tagPrefix = `${tag}: `;
  // Reserve the role-tag width on the first line so wrapping accounts for it.
  const cols = termCols && termCols > 0 ? termCols : 0;
  const firstWidth = cols ? Math.max(20, cols - tagPrefix.length) : 0;
  const restWidth = cols ? Math.max(20, cols) : 0;

  const body = renderMarkdownToLines(
    content,
    restWidth,
    firstWidth,
    glyphs,
    theme
  );
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
  const ruleWidth = cols > 0 ? Math.max(20, Math.min(cols, 80)) : 32;
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
  // Accepted for API symmetry but unused on purpose (see fn doc).
  void termCols;
  return lines.map((line) => gutter + line).join('\n');
}

// ─── Markdown → ANSI Lines ───────────────────────────────────────────────────

/**
 * Turn markdown into ANSI-styled rows ready to `\n`-join into <Static>.
 * Blocks are blank-line separated except adjacent same-indent list items
 * (mirrors TUI MarkdownRenderer's marginTop). `restWidth`/`firstLineWidth` of
 * 0 skips width-aware wrapping (tests use 0; see COPY-PASTE INVARIANT).
 */
export function renderMarkdownToLines(
  text: string,
  restWidth: number,
  firstLineWidth: number,
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
    // explicit no-wrap path (see COPY-PASTE INVARIANT). firstLineWidth/
    // restWidth are still consumed by the structural blocks below.
    void firstLineWidth;
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
      const skipBlank =
        !!prev && prev.listItem && seg.listItem
          ? prev.listItem.indent === seg.listItem.indent
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
