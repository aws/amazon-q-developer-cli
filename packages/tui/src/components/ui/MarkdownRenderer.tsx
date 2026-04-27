import React from 'react';
import { Box } from '../../renderer.js';
import { useSyntaxHighlight } from '../../utils/syntax-highlight.js';
import {
  parseMarkdown,
  parseInlineMarkdown,
  tryAppendMarkdownDelta,
  type MarkdownSegment,
} from '../../utils/markdown.js';
import { expandTabs } from '../../utils/string.js';
import { Text } from './text/Text.js';
import type { TextProps } from '../../renderer.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { hyperlink } from '../../utils/terminal-capabilities.js';
import chalk from 'chalk';
import { visibleWidth } from '../../utils/text-width.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import {
  constrainColumnWidths,
  wrapCellText,
  padCell,
  type Alignment,
} from '../../utils/table-layout.js';

interface MarkdownRendererProps {
  content: string;
  /** Chalk chain for text color (e.g. messageColor from useTheme) */
  color: any;
  /**
   * When true, use `wrap="overflow"` on inner Text components instead of
   * `wrap="wrap"`. Used by wrapDisabled scrollback rendering so the terminal
   * soft-wraps visually but copy-paste keeps logical lines intact.
   */
  useOverflow?: boolean;
}

type RenderBlock =
  | { type: 'text'; segments: MarkdownSegment[] }
  | { type: 'code'; segment: MarkdownSegment }
  | { type: 'header'; segment: MarkdownSegment }
  | { type: 'listItem'; segment: MarkdownSegment }
  | { type: 'blockquote'; segment: MarkdownSegment }
  | { type: 'horizontalRule' }
  | { type: 'table'; segment: MarkdownSegment };

export const MarkdownRenderer = React.memo(function MarkdownRenderer({
  content,
  color,
  useOverflow = false,
}: MarkdownRendererProps) {
  const wrapMode: TextProps['wrap'] = useOverflow ? 'overflow' : 'wrap';
  const highlightCode = useSyntaxHighlight();
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const linkColor = getColor('link');
  const inlineCodeColor = getColor('highlight');
  const secondaryColor = getColor('secondary');

  const parseCacheRef = React.useRef<{
    content: string;
    segments: MarkdownSegment[];
  } | null>(null);
  const styledSegmentCacheRef = React.useRef<WeakMap<MarkdownSegment, string>>(
    new WeakMap()
  );

  // Derive a stable cache key from actual color output, not function references.
  // getColor() returns new chalk chain objects on every render, so using them
  // directly as deps would clear the cache every frame during streaming.
  const colorCacheKey = `${color('_')}|${inlineCodeColor('_')}|${linkColor('_')}|${secondaryColor('_')}`;
  const prevColorKeyRef = React.useRef(colorCacheKey);
  React.useEffect(() => {
    if (prevColorKeyRef.current !== colorCacheKey) {
      prevColorKeyRef.current = colorCacheKey;
      styledSegmentCacheRef.current = new WeakMap();
    }
  }, [colorCacheKey]);

  const segments = React.useMemo(() => {
    const cached = parseCacheRef.current;
    if (cached) {
      if (content === cached.content) {
        return cached.segments;
      }

      if (content.startsWith(cached.content)) {
        const delta = content.slice(cached.content.length);
        const incrementallyAppended = tryAppendMarkdownDelta(
          cached.segments,
          delta,
          cached.content
        );
        if (incrementallyAppended) {
          parseCacheRef.current = {
            content,
            segments: incrementallyAppended,
          };
          return incrementallyAppended;
        }
      }
    }

    const parsed = parseMarkdown(content);
    parseCacheRef.current = { content, segments: parsed };
    return parsed;
  }, [content]);

  const styleSegment = (seg: MarkdownSegment): string => {
    const cached = styledSegmentCacheRef.current.get(seg);
    if (cached !== undefined) {
      return cached;
    }

    // Resolve inner content: recurse into children or use leaf text with color
    const inner = seg.children
      ? seg.children.map((child) => styleSegment(child)).join('')
      : color(seg.text);

    let styled: string;
    if (seg.quote) {
      styled = inlineCodeColor(seg.children ? inner : seg.text);
      styledSegmentCacheRef.current.set(seg, styled);
      return styled;
    }
    if (seg.link) {
      styled =
        hyperlink(seg.link.url, linkColor(inner)) +
        secondaryColor(` (${seg.link.url})`);
      styledSegmentCacheRef.current.set(seg, styled);
      return styled;
    }
    styled = inner;
    if (seg.bold) styled = chalk.bold(styled);
    if (seg.italic) styled = chalk.italic(styled);
    if (seg.strikethrough) styled = chalk.strikethrough(styled);

    styledSegmentCacheRef.current.set(seg, styled);
    return styled;
  };

  const renderInlineText = (text: string): string => {
    return parseInlineMarkdown(text)
      .map((seg) => styleSegment(seg))
      .join('');
  };

  const blocks = React.useMemo(() => {
    const computedBlocks: RenderBlock[] = [];
    let currentTextGroup: MarkdownSegment[] = [];

    const flushTextGroup = () => {
      if (currentTextGroup.length > 0) {
        computedBlocks.push({ type: 'text', segments: currentTextGroup });
        currentTextGroup = [];
      }
    };

    segments.forEach((segment) => {
      if (segment.codeBlock) {
        flushTextGroup();
        computedBlocks.push({ type: 'code', segment });
      } else if (segment.header || segment.boldHeading) {
        flushTextGroup();
        computedBlocks.push({ type: 'header', segment });
      } else if (segment.listItem) {
        flushTextGroup();
        computedBlocks.push({ type: 'listItem', segment });
      } else if (segment.blockquote) {
        flushTextGroup();
        computedBlocks.push({ type: 'blockquote', segment });
      } else if (segment.horizontalRule) {
        flushTextGroup();
        computedBlocks.push({ type: 'horizontalRule' });
      } else if (segment.table) {
        flushTextGroup();
        computedBlocks.push({ type: 'table', segment });
      } else {
        currentTextGroup.push(segment);
      }
    });
    flushTextGroup();

    return computedBlocks;
  }, [segments]);

  const needsSpacingBefore = (
    prev: RenderBlock,
    curr: RenderBlock
  ): boolean => {
    if (prev.type === 'listItem' && curr.type === 'listItem') {
      const prevIndent = prev.segment.listItem!.indent;
      const currIndent = curr.segment.listItem!.indent;
      // Add spacing when de-indenting (e.g. sub-item → new top-level item)
      return currIndent < prevIndent;
    }
    if (prev.type === 'blockquote' && curr.type === 'blockquote') return false;
    return true;
  };

  return (
    <Box flexDirection="column">
      {blocks.map((block, i) => {
        const mt = i > 0 && needsSpacingBefore(blocks[i - 1]!, block) ? 1 : 0;

        if (block.type === 'code') {
          const code = expandTabs(block.segment.codeBlock!.code);
          return (
            <Box key={i} marginTop={mt}>
              <Text>
                {highlightCode(code, block.segment.codeBlock!.language)}
              </Text>
            </Box>
          );
        }

        if (block.type === 'header') {
          return (
            <Box key={i} marginTop={mt}>
              <Text wrap={wrapMode}>
                {chalk.bold(renderInlineText(block.segment.text))}
              </Text>
            </Box>
          );
        }

        if (block.type === 'listItem') {
          const { ordered, number, indent } = block.segment.listItem!;
          const prefix = ordered ? `${number}. ` : '- ';
          const indentStr = '  '.repeat(indent);
          return (
            <Box key={i} marginTop={mt}>
              <Text wrap={wrapMode}>
                {color(indentStr + prefix)}
                {renderInlineText(block.segment.text)}
              </Text>
            </Box>
          );
        }

        if (block.type === 'blockquote') {
          return (
            <Box key={i} marginTop={mt}>
              <Text>
                {chalk.dim('│ ')}
                {chalk.italic(renderInlineText(block.segment.text))}
              </Text>
            </Box>
          );
        }

        if (block.type === 'horizontalRule') {
          return (
            <Box key={i} marginTop={mt}>
              <Divider />
            </Box>
          );
        }

        if (block.type === 'table') {
          const { headers, rows, alignments } = block.segment.table!;
          const measureRendered = (s: string) =>
            visibleWidth(renderInlineText(s));

          // Measure natural widths based on rendered text (markdown stripped)
          const colWidths = headers.map((h, ci) => {
            const headerWidth = measureRendered(h);
            const dataWidths = rows.map((r) => measureRendered(r[ci] || ''));
            return Math.max(headerWidth, ...dataWidths, 3);
          });

          constrainColumnWidths(colWidths, termWidth);

          const border = (
            left: string,
            mid: string,
            right: string,
            fill: string
          ) =>
            left + colWidths.map((w) => fill.repeat(w + 2)).join(mid) + right;

          // Render inline markdown first, then wrap styled text, then pad
          const renderWrappedRow = (rawCells: string[], bold?: boolean) => {
            const styledCells = rawCells.map((c) => {
              let s = renderInlineText(c);
              if (bold && s) s = chalk.bold(s);
              return s;
            });
            const wrapped = styledCells.map((c, ci) =>
              wrapCellText(c, colWidths[ci]!, visibleWidth)
            );
            const maxLines = Math.max(...wrapped.map((w) => w.length));
            const lines: string[] = [];
            for (let li = 0; li < maxLines; li++) {
              const line = rawCells
                .map((_, ci) => {
                  const styled = wrapped[ci]?.[li] || '';
                  return padCell(
                    styled,
                    colWidths[ci]!,
                    (alignments[ci] || 'left') as Alignment,
                    visibleWidth
                  );
                })
                .join(` ${chalk.dim('│')} `);
              lines.push(`${chalk.dim('│')} ${line} ${chalk.dim('│')}`);
            }
            return lines;
          };

          const headerLines = renderWrappedRow(headers, true);
          const rowSeparator =
            rows.length > 0 ? chalk.dim(border('├', '┼', '┤', '─')) : undefined;

          return (
            <Box key={i} flexDirection="column" marginTop={mt}>
              <Text>{chalk.dim(border('┌', '┬', '┐', '─'))}</Text>
              {headerLines.map((line, li) => (
                <Text key={`h${li}`}>{line}</Text>
              ))}
              {rowSeparator && <Text>{rowSeparator}</Text>}
              {rows.map((row, ri) => {
                const lines = renderWrappedRow(row);
                const isLast = ri === rows.length - 1;
                return (
                  <React.Fragment key={ri}>
                    {lines.map((line, li) => (
                      <Text key={`${ri}-${li}`}>{line}</Text>
                    ))}
                    {!isLast && (
                      <Text>{chalk.dim(border('├', '┼', '┤', '─'))}</Text>
                    )}
                  </React.Fragment>
                );
              })}
              <Text>{chalk.dim(border('└', '┴', '┘', '─'))}</Text>
            </Box>
          );
        }

        // Text block
        const styledText = block.segments
          .map((seg) => styleSegment(seg))
          .join('');

        return (
          <Box key={i} marginTop={mt}>
            <Text wrap={wrapMode}>{styledText}</Text>
          </Box>
        );
      })}
    </Box>
  );
});
