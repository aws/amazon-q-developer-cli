import React from 'react';
import { Box } from 'ink';
import { useSyntaxHighlight } from '../../utils/syntax-highlight.js';
import {
  parseMarkdown,
  parseInlineMarkdown,
  type MarkdownSegment,
} from '../../utils/markdown.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import chalk from 'chalk';

interface MarkdownRendererProps {
  content: string;
  /** Chalk chain for text color (e.g. messageColor from useTheme) */
  color: any;
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
}: MarkdownRendererProps) {
  const highlightCode = useSyntaxHighlight();
  const { getColor } = useTheme();
  const linkColor = getColor('link');
  const inlineCodeColor = getColor('syntax.string');
  const secondaryColor = getColor('secondary');

  const styleSegment = (seg: MarkdownSegment): string => {
    if (seg.quote) return inlineCodeColor(seg.text);
    if (seg.link)
      return linkColor(seg.text) + secondaryColor(` (${seg.link.url})`);
    let styled = seg.text;
    if (seg.bold) styled = chalk.bold(styled);
    if (seg.italic) styled = chalk.italic(styled);
    if (seg.strikethrough) styled = chalk.strikethrough(styled);
    return color(styled);
  };

  const renderInlineText = (text: string): string => {
    return parseInlineMarkdown(text)
      .map((seg) => styleSegment(seg))
      .join('');
  };
  const segments = parseMarkdown(content);

  const blocks: RenderBlock[] = [];
  let currentTextGroup: MarkdownSegment[] = [];

  const flushTextGroup = () => {
    if (currentTextGroup.length > 0) {
      blocks.push({ type: 'text', segments: currentTextGroup });
      currentTextGroup = [];
    }
  };

  segments.forEach((segment) => {
    if (segment.codeBlock) {
      flushTextGroup();
      blocks.push({ type: 'code', segment });
    } else if (segment.header) {
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
      blocks.push({ type: 'horizontalRule' } as RenderBlock);
    } else if (segment.table) {
      flushTextGroup();
      blocks.push({ type: 'table', segment });
    } else {
      currentTextGroup.push(segment);
    }
  });
  flushTextGroup();

  // Normalize: strip trailing newlines from text blocks before headers only
  // Headers get marginTop for consistent spacing regardless of LLM output
  for (let b = 0; b < blocks.length - 1; b++) {
    const blk = blocks[b];
    if (blk && blk.type === 'text' && blocks[b + 1]?.type === 'header') {
      const last = blk.segments[blk.segments.length - 1];
      if (last) last.text = last.text.replace(/\n+$/, '');
    }
  }

  return (
    <Box flexDirection="column">
      {blocks.map((block, i) => {
        if (block.type === 'code') {
          const code = block.segment.codeBlock!.code.replace(/^\n+|\n+$/g, '');
          return (
            <Box key={i}>
              <Text>
                {highlightCode(code, block.segment.codeBlock!.language)}
              </Text>
            </Box>
          );
        }

        if (block.type === 'header') {
          return (
            <Box key={i} marginTop={i > 0 ? 1 : 0}>
              <Text wrap="wrap">{color(chalk.bold(block.segment.text))}</Text>
            </Box>
          );
        }

        if (block.type === 'listItem') {
          const { ordered, number, indent } = block.segment.listItem!;
          const prefix = ordered ? `${number}. ` : '- ';
          const indentStr = '  '.repeat(indent);
          return (
            <Box key={i}>
              <Text wrap="wrap">
                {color(indentStr + prefix)}
                {renderInlineText(block.segment.text)}
              </Text>
            </Box>
          );
        }

        if (block.type === 'blockquote') {
          return (
            <Box key={i}>
              <Text>
                {chalk.dim('│ ')}
                {color(chalk.italic(block.segment.text))}
              </Text>
            </Box>
          );
        }

        if (block.type === 'horizontalRule') {
          return (
            <Box key={i}>
              <Divider />
            </Box>
          );
        }

        if (block.type === 'table') {
          const { headers, rows, alignments } = block.segment.table!;
          const colWidths = headers.map((h, ci) => {
            const dataWidths = rows.map((r) => (r[ci] || '').length);
            return Math.max(h.length, ...dataWidths, 3);
          });
          const padCell = (text: string, width: number, align: string) => {
            const stripped = text.trim();
            const pad = width - stripped.length;
            if (pad <= 0) return stripped;
            if (align === 'right') return ' '.repeat(pad) + stripped;
            if (align === 'center') {
              const left = Math.floor(pad / 2);
              return ' '.repeat(left) + stripped + ' '.repeat(pad - left);
            }
            return stripped + ' '.repeat(pad);
          };
          const formatRow = (cells: string[]) =>
            '| ' +
            cells
              .map((c, ci) =>
                padCell(c, colWidths[ci] || 3, alignments[ci] || 'left')
              )
              .join(' | ') +
            ' |';
          const separator =
            '|' + colWidths.map((w) => '-'.repeat(w + 2)).join('|') + '|';

          return (
            <Box key={i} flexDirection="column">
              <Text>{color(formatRow(headers))}</Text>
              <Text>{chalk.dim(separator)}</Text>
              {rows.map((row, ri) => (
                <Text key={ri}>{color(formatRow(row))}</Text>
              ))}
            </Box>
          );
        }

        // Text block
        const styledText = block.segments
          .map((seg) => styleSegment(seg))
          .join('');

        return (
          <Box key={i}>
            <Text wrap="wrap">{styledText}</Text>
          </Box>
        );
      })}
    </Box>
  );
});
