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
import { resolveGlyphs, resolveTheme, brand, DEFAULT_USER_TAG, type RenderTheme } from './theme.js';
import { wrapStyled, stripAnsiQuick, highlightLineSafe } from './text.js';

export function renderUserMessage(text: string, theme?: RenderTheme): string {
  // `You:` role tag at the head, then the body painted with the user's prompt
  // preset colors. /theme bundled:dark|light + /theme prompt:<id> drive these
  // via buildRenderTheme — so a Purple preset paints white-on-violet across
  // the body, matching what standard mode does with `<Box backgroundColor>`.
  // Continuation lines get no leading indent so the message copies cleanly
  // out of the terminal — the previous "  " gutter put two spaces in front of
  // every wrapped row in the clipboard.
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
 * Render an agent (model) message as styled, wrapped markdown.
 *
 * Markdown rendering is scoped here intentionally — tool outputs, user
 * messages, and system messages stay plain text. Streaming partial-block
 * text would reflow as fences/lists/tables come in (`# foo` becomes a
 * heading only after the trailing space, `|a|b|` becomes a table only
 * after the separator row), so the live region renders streaming chunks
 * verbatim and re-parses to markdown only when the message finalizes
 * here, on its way to <Static>.
 *
 * `termCols` (when known) drives soft-wrapping for paragraphs, list items,
 * blockquotes, and tables. Without it, paragraphs flow on a single logical
 * line and the terminal does its own wrap; tables fall back to natural
 * widths. Lite passes process.stdout.columns through RenderContext.termCols.
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
  // Default to "Kiro" for the user-facing tag. When the active agent has a
  // distinct name (custom agents, swapped via /agent), use it verbatim so
  // the user can tell which persona answered.
  const tag = agentName && agentName.trim() ? agentName : 'Kiro';
  // Per-agent color for the role tag so scrollback matches what the footer
  // shows for the same agent (getAgentColor in agentColors.ts). Falls back
  // to theme.brand for the default agent (getAgentColor maps kiro_default →
  // brand internally) and for callers without an agent-color resolver
  // (tests / pure contexts).
  const tagColorFn =
    getAgentTagColor && agentName
      ? getAgentTagColor(agentName)
      : (theme?.brand ?? brand);
  // `chalk.x.bold` works on chalk fns; user-provided fns from the theme
  // are plain string→string and don't compose. Wrap manually to keep the
  // bolded role tag without losing the theme color.
  const tagBold = applyBold(tagColorFn(`${tag}:`));
  const tagPrefix = `${tag}: `;
  // Reserve the role-tag width on the first markdown line so wrapping
  // accounts for it (otherwise the body line that starts with `Kiro: ` is
  // measured as if it had no prefix and overflows). Subsequent lines use
  // the full width.
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
  // Structural blocks (tables, fenced code, blockquotes, horizontal rules)
  // render their first line as visual chrome — top border `┌────┐`, fence
  // ```` ``` ````, bar `│ `, rule `─────`. Gluing the role tag onto that
  // first line shifts ONLY the first row right by `Kiro: ` width while
  // every subsequent row sits at column 0, breaking alignment (table
  // borders no longer line up with cells, code fences hang off the
  // content, blockquote bars go ragged). When the body opens with one of
  // those blocks, drop the role tag onto its own line so the entire
  // structural block stays at column 0. Inline markdown (paragraphs,
  // headers, lists) keeps the inline form so the common case still reads
  // as `Kiro: <reply>` without an awkward leading newline.
  if (firstBlockNeedsOwnLine(content)) {
    return tagBold + '\n' + body.join('\n');
  }
  if (!first || rest.length === 0) return tagBold + ' ' + (first ?? '');
  return [tagBold + ' ' + first, ...rest].join('\n');
}

/**
 * True when the first parsed markdown segment is a structural block whose
 * rendered first line is visual chrome — table top border, fenced code
 * delimiter, blockquote bar, or horizontal rule. Such blocks must paint at
 * column 0 to keep their inner geometry aligned, so {@link renderAgentMessage}
 * uses this to decide between inline (`Kiro: <body>`) and own-line
 * (`Kiro:\n<body>`) framing.
 *
 * Note: headers and list items intentionally stay inline. A header's first
 * line is plain styled text (no chrome), and list items lead with a short
 * `- ` / `1. ` marker that reads naturally after the role tag. Adding them
 * here would force a leading newline on the most common short-reply shapes
 * (bulleted answers, simple headed responses) without an alignment payoff.
 *
 * Cost: one `parseMarkdown` pass on the content. {@link renderMarkdownToLines}
 * also calls `parseMarkdown` so the same content is tokenized twice — both
 * passes are O(n) and run once per finalized agent message, never in the
 * spinner loop. The duplication is preferable to threading the segment list
 * through the renderer's signature.
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
 * Render the agent's reasoning ("thinking") block as a self-contained section
 * that lives in scrollback above the agent's spoken text. Top + bottom purple
 * rules bracket a dim italic body, indented two spaces. The body wraps at
 * the terminal width using the same {@link wrapAnsiLine} pipeline the rest of
 * lite uses, so long reasoning paragraphs don't run off the right edge.
 *
 * Callers should only invoke this when {@link VerboseDisplayConfig.showReasoningContent}
 * is true. Returns `''` when the input is empty so callers can unconditionally
 * concatenate.
 */
export function renderThinkingBlock(
  thinking: string,
  theme?: RenderTheme,
  termCols?: number,
  glyphs?: Glyphs
): string {
  if (!thinking || !thinking.trim()) return '';
  const cols = termCols && termCols > 0 ? termCols : 0;
  // Body sits flush with the rules at column 0 — no indent. Earlier the body
  // was indented two spaces so the rules read as a section header above an
  // inset block, but the section identifier (`─── thinking ───`) is already
  // unambiguous on its own and the inset added visual noise without payoff.
  const indent = '';
  // Brand color for the rules so the section stands out from the dim italic
  // body. Use theme.brand when available so /theme bundled:dark|light actually
  // re-skins the borders.
  const brandFn = theme?.brand ?? brand;
  const ruleWidth = cols > 0 ? Math.max(20, Math.min(cols, 80)) : 32;
  const g = resolveGlyphs(glyphs);
  // ASCII mode: lineHorizontal becomes '-', rules degrade gracefully (e.g.
  // `--- thinking ---------` instead of `─── thinking ──────────`). The label
  // stays in its slot so the section header still reads as one.
  const h = g.lineHorizontal;
  const topRule = brandFn(
    h.repeat(3) + ' thinking ' + h.repeat(Math.max(3, ruleWidth - 13))
  );
  const bottomRule = brandFn(h.repeat(Math.max(3, ruleWidth)));
  // Each source line of the thinking text becomes one logical body row.
  // The terminal soft-wraps long lines visually; the 2-space indent + dim
  // italic style apply to the first visual row, and continuation rows
  // inherit the dim italic SGR state across the soft-wrap (terminals
  // don't reset SGR on visual wrap boundaries). The previous wrapAnsiLine
  // pass made each visual row its own logical line, baking \n into the
  // rendered string and breaking copy-paste of reasoning paragraphs that
  // exceeded the terminal width.
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
    // Dim italic — the original treatment, restored. The purple
    // top/bottom rules already identify this region as the reasoning
    // section, so the body recedes into a soft "atmospheric prose" tier.
    // Different from tool output (which carries a sage-green tint
    // signaling "result") and from agent prose (full default fg);
    // three blocks, three distinct tones.
    .map((r) => indent + chalk.dim.italic(r || ' '))
    .join('\n');
  return [topRule, body, bottomRule].join('\n');
}

/**
 * Render the streaming output of a `!` shell-escape command. Each source
 * line (split on `\n`) gets a brand-purple `! ` left gutter, making the
 * row visually distinct from agent prose (`Kiro: …` tag) and from
 * thinking (`──── thinking ────` rules + dim italic).
 *
 * Visual signal: the `!` echoes the `!` glyph the user typed to enter
 * shell-escape mode in the first place, and the `! ` swap on the input
 * row while the command is in flight. Three places, one cue — gutter
 * on output, prompt on input, original `!` on the User message that
 * launched the command.
 *
 * Per-source-line gutter (not per visual row): each `\n` boundary in the
 * raw PTY output gets one gutter; the terminal soft-wraps overlong lines
 * visually, with the wrapped continuation flowing under the gutter
 * column. Same compromise {@link renderThinkingBlock} makes for its body
 * — keeps copy-paste of long shell command output as one logical line
 * per output line, which matches what a real terminal would show. The
 * wrapAnsiLine pipeline isn't used here because shell output is already
 * positioned at column 0 by the bash process; re-wrapping it would also
 * fight any cursor-positioning escapes the program emits (mwinit's PIN
 * prompt, sudo's password row, etc.).
 *
 * Trailing blank lines are trimmed so the output's last visible row sits
 * flush against whatever follows it (input box during streaming, the
 * next message after the command exits and this row commits to static).
 *
 * Returns `''` when the buffer is empty so callers can pre-render
 * whether or not data has arrived yet.
 */
export function renderShellOutputBlock(
  content: string,
  theme?: RenderTheme,
  termCols?: number
): string {
  if (!content) return '';
  const brandFn = theme?.brand ?? brand;
  const gutter = brandFn('! ');
  // Drop only trailing blank lines — leading blanks in shell output are
  // sometimes meaningful (e.g. a tool that prints a blank row before its
  // banner) and the gutter should still appear there.
  const lines = content.split('\n');
  while (lines.length > 0 && !lines[lines.length - 1]?.trim()) {
    lines.pop();
  }
  if (lines.length === 0) return '';
  // termCols is accepted for API symmetry with renderThinkingBlock /
  // renderAgentMessage, but intentionally unused: re-wrapping shell
  // output would mangle programs that emit cursor-positioning escapes
  // or rely on column-aligned output (htop-style ascii UIs, the column
  // alignment in `ls -l`, etc.). Better to let the terminal soft-wrap
  // — same default the streaming live region's <Text wrap="overflow">
  // uses for tool bodies.
  void termCols;
  return lines.map((line) => gutter + line).join('\n');
}

/**
 * Bold-wrap a string that may already carry an ANSI color sequence. We rely
 * on chalk.bold which inserts \x1b[1m...\x1b[22m — not a full reset — so the
 * caller's foreground color survives the wrap.
 */
function applyBold(s: string): string {
  return chalk.bold(s);
}

// ─── Markdown → ANSI Lines ───────────────────────────────────────────────────

/**
 * Turn a markdown string into a list of ANSI-styled visual rows ready to
 * concatenate with `\n` and emit to <Static>. Wrapping is visible-width aware
 * so emojis, CJK, and ANSI escapes don't blow up the column count.
 *
 * Block spacing rule: every block (heading, paragraph, list, code, table,
 * blockquote, hr) is separated by a blank line, except adjacent list items
 * at the same indent level (which pack tightly). This mirrors the TUI
 * MarkdownRenderer's `marginTop` defaults so lite scrollback reads the
 * same way as standard mode.
 *
 * `restWidth`/`firstLineWidth` of 0 means "skip width-aware wrapping" —
 * paragraphs flow as one logical line and the terminal wraps. Tests use
 * width 0 so they aren't sensitive to terminal size.
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
    // Paragraphs flow as one logical line per source paragraph — only
    // splitting on \n that the markdown carries itself (e.g. <br>). No
    // width-aware wrapping. The Static <Text> in LiteLayout uses
    // wrap="overflow" so twinki doesn't re-wrap either; the terminal
    // handles the visual wrap. Copy-paste from scrollback then preserves
    // the original logical line, which is the load-bearing reason — long
    // URLs, code-like prose, and run-on sentences used to copy with hard
    // \n at every visual row boundary, breaking links the moment they
    // crossed the terminal edge. wrapStyled(s, 0, 0) is the explicit
    // "no wrap" path, returning s.split('\n').
    //
    // The firstLineWidth / restWidth parameters threaded into this
    // function are still consumed by block elements below (list items,
    // blockquotes, tables) where structural prefixes (`- `, `│ `, column
    // borders) MUST repeat on every visual row — those still wrap at
    // termCols on purpose.
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

/**
 * Append a block's lines to `out`, with a leading blank line when this is
 * not the first block in the message. Centralizing the blank-line rule here
 * keeps block separators consistent across paragraphs/lists/tables/code.
 */
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
  // Block elements (headers, bold headings, blockquotes, list items) carry
  // their content as a raw markdown string in `seg.text` — `parseMarkdown`
  // intentionally doesn't recurse into block bodies (see the parser test
  // "should keep list item text raw for inline parsing"). The renderer is
  // responsible for re-lexing that text through the inline path so
  // `**bold**`, `` `code` ``, `*italic*`, `[link](url)`, etc. surface as
  // styled output instead of bleeding through as literal markers. We use
  // `renderInlineMarkdown` here for the same reason table cells do —
  // it goes through `parseInlineMarkdown` first, while `renderInlineSegment`
  // only honors flags already set on the segment and would otherwise return
  // the raw text unchanged.
  if (seg.header) {
    const inline = renderInlineMarkdown(seg.text, theme);
    return wrapStyled(chalk.bold(inline), width, width);
  }
  if (seg.boldHeading) {
    const inline = renderInlineMarkdown(seg.text, theme);
    return wrapStyled(chalk.bold(inline), width, width);
  }
  if (seg.listItem) return renderListItem(seg, width, theme);
  if (seg.blockquote) {
    const inline = renderInlineMarkdown(seg.text, theme);
    const prefix = chalk.dim(`${g.lineVertical} `);
    const styled = chalk.italic(inline);
    // Blockquotes emit as one logical line with the leading │ prefix.
    // The terminal soft-wraps long quotes visually; only the first
    // visual row carries the bar glyph. The leading prefix is preserved
    // in copy-paste, matching the markdown source's `> ` semantics —
    // pasting a multi-row blockquote yields one logical quote string,
    // not N hard-wrapped chunks each carrying their own bar glyph.
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
  // Re-lex the list item's body through the inline path so `**bold**`,
  // `` `code` ``, links, etc. surface as styled output. See the comment
  // on `renderBlockSegment` for the contract.
  const inline = renderInlineMarkdown(seg.text, theme);
  if (!inline) return [head.trimEnd()];
  // List item bodies emit as one logical line. Long bodies (URLs, run-on
  // sentences) soft-wrap visually via the terminal, but the clipboard
  // sees one logical line — so triple-click selection of a bullet item
  // copies the whole item without injected \n. The hanging-indent
  // continuation that the previous wrap produced is sacrificed for that
  // copy fidelity; the leading bullet on the first visual row keeps the
  // list semantic clear and matches the markdown source's `- ` syntax.
  return [head + inline];
}

function renderCodeBlock(
  code: { code: string; language?: string; isComplete: boolean },
  _width: number
): string[] {
  const lines: string[] = [];
  const lang = code.language ? ` ${code.language}` : '';
  lines.push(chalk.dim(`\`\`\`${lang}`));
  // Code lines are emitted as-is — terminal soft-wraps anything wider than
  // the available column count. The previous behavior baked \n at every
  // visual row boundary into the rendered string, which copy-pasted as
  // hard newlines mid-line and corrupted shell commands, multi-line
  // identifiers, and inline URLs in the snippet. The Static <Text> in
  // LiteLayout uses wrap="overflow" so twinki passes the long lines
  // through to the terminal verbatim, preserving single-logical-line
  // copy semantics for code blocks.
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

  // Glyphs picked from the active set — Unicode box-drawing in the default
  // mode, ASCII `+` / `-` / `|` fallbacks when `chat.allowAsciiArt=false`.
  // Note teeLeft/teeRight semantics: in modern TUI's modeling, teeLeft is `┤`
  // (joining a vertical from the right) and teeRight is `├` (joining a
  // vertical from the left). We map them through directly.
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
 * Render a single markdown segment as inline ANSI. Recurses into children
 * for nested formatting (e.g. **bold _italic_**). Block-level fields on the
 * outer segment (header/listItem/...) are ignored — callers strip them
 * before passing in so this stays inline-only.
 *
 * Color slots are pulled from {@link RenderTheme} — `inlineCode` for
 * codespans (the `seg.quote` flag set by the marked-based parser; the name
 * is historical, not a blockquote tie-in), `link` for the link label, and
 * `secondary` for the dim `(url)` trailer. When no theme is supplied
 * (tests / pure-context callers), the renderer falls back to
 * {@link DEFAULT_RENDER_THEME}, whose values match the prior hardcoded
 * `chalk.cyan` / `chalk.dim` so existing assertions stay green.
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
      // Underline applied independently of the theme color so links stay
      // visually distinct on themes whose `link` slot matches prose. Modern
      // TUI uses OSC8 hyperlinks (capability-detected) instead — lite stays
      // on the underline+color form, which works in every terminal.
      const labeled = chalk.underline(t.link(inner));
      // Hide the URL when the visible label already equals the URL — the
      // bare-link case is the common one and a `(url)` trailer doubles the
      // text. Otherwise keep the trailer so users can read where a link
      // points without an OSC8-aware terminal.
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
 * Inline markdown renderer for table cells — `parseMarkdown` doesn't recurse
 * into row text, so cells arrive as raw strings that may still contain
 * **bold**, *italic*, `code`, and link syntax. Re-lex with the parser's
 * inline path and stitch the segments together.
 */
function renderInlineMarkdown(s: string, theme?: RenderTheme): string {
  if (!s) return '';
  // Inline path uses marked's lexInline so block markers like leading `#`
  // or `-` aren't promoted into headings/lists when they appear inside a
  // table cell. Children/bold/italic/code/link composition matches the
  // paragraph renderer.
  const segs = parseInlineMarkdown(s);
  const t = resolveTheme(theme);
  return segs.map((seg) => renderInlineSegment(seg, t)).join('');
}
