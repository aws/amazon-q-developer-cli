import { chalk } from './color.js';
import { visibleWidth } from './text-width.js';
import {
  constrainColumnWidths,
  formatStackedTable,
  padCell,
  shouldStackTable,
  wrapCellText,
  type Alignment,
} from './table-layout.js';
import type { Glyphs } from './glyphs.js';
import { parseInlineMarkdown, type MarkdownSegment } from './markdown.js';

export interface InlineMarkdownPainters {
  text: (text: string) => string;
  inlineCode: (text: string) => string;
  bold: (text: string) => string;
  italic: (text: string) => string;
  strikethrough: (text: string) => string;
  link: (text: string, url: string, isBareUrl: boolean) => string;
}

export function renderMarkdownInlineSegment(
  segment: MarkdownSegment,
  painters: InlineMarkdownPainters,
  cache?: WeakMap<MarkdownSegment, string>
): string {
  const cached = cache?.get(segment);
  if (cached !== undefined) return cached;

  const inner = segment.children
    ? segment.children
        .map((child) => renderMarkdownInlineSegment(child, painters, cache))
        .join('')
    : painters.text(segment.text);

  let styled: string;
  if (segment.quote) {
    styled = painters.inlineCode(segment.children ? inner : segment.text);
  } else if (segment.link) {
    styled = painters.link(
      inner,
      segment.link.url,
      markdownSegmentText(segment) === segment.link.url
    );
  } else {
    styled = inner;
    if (segment.bold) styled = painters.bold(styled);
    if (segment.italic) styled = painters.italic(styled);
    if (segment.strikethrough) styled = painters.strikethrough(styled);
  }

  cache?.set(segment, styled);
  return styled;
}

function markdownSegmentText(segment: MarkdownSegment): string {
  return segment.children
    ? segment.children.map(markdownSegmentText).join('')
    : segment.text;
}

export function renderMarkdownInlineText(
  text: string,
  painters: InlineMarkdownPainters
): string {
  return parseInlineMarkdown(text)
    .map((segment) => renderMarkdownInlineSegment(segment, painters))
    .join('');
}

export type MarkdownRenderBlock =
  | { type: 'text'; segments: MarkdownSegment[] }
  | { type: 'code'; segment: MarkdownSegment }
  | { type: 'header'; segment: MarkdownSegment }
  | { type: 'listItem'; segment: MarkdownSegment }
  | { type: 'blockquote'; segment: MarkdownSegment }
  | { type: 'horizontalRule' }
  | { type: 'table'; segment: MarkdownSegment };

export function buildMarkdownRenderBlocks(
  segments: MarkdownSegment[]
): MarkdownRenderBlock[] {
  const blocks: MarkdownRenderBlock[] = [];
  let textGroup: MarkdownSegment[] = [];

  const flushTextGroup = () => {
    if (textGroup.length > 0) {
      blocks.push({ type: 'text', segments: textGroup });
      textGroup = [];
    }
  };

  for (const segment of segments) {
    if (segment.codeBlock) {
      flushTextGroup();
      blocks.push({ type: 'code', segment });
    } else if (segment.header || segment.boldHeading) {
      flushTextGroup();
      blocks.push({ type: 'header', segment });
    } else if (segment.listItem) {
      flushTextGroup();
      blocks.push({ type: 'listItem', segment });
    } else if (segment.blockquote) {
      flushTextGroup();
      blocks.push({ type: 'blockquote', segment });
    } else if (segment.horizontalRule) {
      flushTextGroup();
      blocks.push({ type: 'horizontalRule' });
    } else if (segment.table) {
      flushTextGroup();
      blocks.push({ type: 'table', segment });
    } else {
      textGroup.push(segment);
    }
  }

  flushTextGroup();
  return blocks;
}

export function needsMarkdownSpacingBefore(
  previous: MarkdownRenderBlock,
  current: MarkdownRenderBlock
): boolean {
  if (previous.type === 'listItem' && current.type === 'listItem') {
    return current.segment.listItem!.indent < previous.segment.listItem!.indent;
  }
  if (previous.type === 'blockquote' && current.type === 'blockquote') {
    return false;
  }
  return true;
}

export function renderMarkdownTableLines(
  table: NonNullable<MarkdownSegment['table']>,
  {
    termWidth,
    glyphs,
    renderInline,
  }: {
    termWidth: number;
    glyphs: Glyphs;
    renderInline: (s: string) => string;
  }
): { lines: string[]; stacked: boolean } {
  const { headers, rows, alignments } = table;
  if (headers.length === 0) return { lines: [], stacked: false };

  const measureRendered = (s: string) => visibleWidth(renderInline(s));
  const colWidths = headers.map((h, ci) => {
    const headerWidth = measureRendered(h);
    const dataWidths = rows.map((r) => measureRendered(r[ci] || ''));
    return Math.max(headerWidth, ...dataWidths, 3);
  });

  if (shouldStackTable(colWidths, termWidth)) {
    return {
      lines: formatStackedTable(headers, rows, renderInline, chalk.bold),
      stacked: true,
    };
  }

  if (termWidth > 0) constrainColumnWidths(colWidths, termWidth);

  const border = (left: string, mid: string, right: string, fill: string) =>
    left + colWidths.map((w) => fill.repeat(w + 2)).join(mid) + right;

  const renderRow = (rawCells: string[], bold?: boolean): string[] => {
    const styledCells = rawCells.map((c) => {
      let s = renderInline(c);
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
          (alignments[ci] || 'left') as Alignment,
          visibleWidth
        );
      });
      const joined = cells.join(` ${chalk.dim(glyphs.lineVertical)} `);
      lines.push(
        `${chalk.dim(glyphs.lineVertical)} ${joined} ${chalk.dim(glyphs.lineVertical)}`
      );
    }
    return lines;
  };

  const lines: string[] = [];
  lines.push(
    chalk.dim(
      border(
        glyphs.cornerTopLeft,
        glyphs.teeTop,
        glyphs.cornerTopRight,
        glyphs.lineHorizontal
      )
    )
  );
  lines.push(...renderRow(headers, true));
  if (rows.length > 0) {
    const separator = chalk.dim(
      border(
        glyphs.teeRight,
        glyphs.tableCross,
        glyphs.teeLeft,
        glyphs.lineHorizontal
      )
    );
    lines.push(separator);
    for (let ri = 0; ri < rows.length; ri++) {
      lines.push(...renderRow(rows[ri] ?? []));
      if (ri < rows.length - 1) lines.push(separator);
    }
  }
  lines.push(
    chalk.dim(
      border(
        glyphs.cornerBottomLeft,
        glyphs.teeBottom,
        glyphs.cornerBottomRight,
        glyphs.lineHorizontal
      )
    )
  );
  return { lines, stacked: false };
}
