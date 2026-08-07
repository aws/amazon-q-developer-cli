import React from 'react';
import { Box } from '../../renderer.js';
import { useSyntaxHighlight } from '../../utils/syntax-highlight.js';
import {
  parseMarkdown,
  tryAppendMarkdownDelta,
  type MarkdownSegment,
} from '../../utils/markdown.js';
import {
  buildMarkdownRenderBlocks,
  needsMarkdownSpacingBefore,
  renderMarkdownInlineSegment,
  renderMarkdownInlineText,
  renderMarkdownTableLines,
} from '../../utils/markdown-rendering.js';
import { expandTabs } from '../../utils/string.js';
import { Text } from './text/Text.js';
import type { TextProps } from '../../renderer.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { hyperlink } from '../../utils/terminal-capabilities.js';
import { chalk } from '../../utils/color.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';

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

export const MarkdownRenderer = React.memo(function MarkdownRenderer({
  content,
  color,
  useOverflow = false,
}: MarkdownRendererProps) {
  const wrapMode: TextProps['wrap'] = useOverflow ? 'overflow' : 'wrap';
  const highlightCode = useSyntaxHighlight();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
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

  const inlinePainters = {
    text: color,
    inlineCode: inlineCodeColor,
    bold: chalk.bold,
    italic: chalk.italic,
    strikethrough: chalk.strikethrough,
    link: (text: string, url: string, isBareUrl: boolean) => {
      const linkText = hyperlink(url, linkColor(text));
      return isBareUrl
        ? linkText
        : linkText + secondaryColor(` (${hyperlink(url, url)})`);
    },
  };

  const styleSegment = (seg: MarkdownSegment): string =>
    renderMarkdownInlineSegment(
      seg,
      inlinePainters,
      styledSegmentCacheRef.current
    );

  const renderInlineText = (text: string): string => {
    return renderMarkdownInlineText(text, inlinePainters);
  };

  const blocks = React.useMemo(
    () => buildMarkdownRenderBlocks(segments),
    [segments]
  );

  return (
    <Box flexDirection="column">
      {blocks.map((block, i) => {
        const mt =
          i > 0 && needsMarkdownSpacingBefore(blocks[i - 1]!, block) ? 1 : 0;

        if (block.type === 'code') {
          const code = expandTabs(block.segment.codeBlock!.code);
          return (
            <Box key={i} marginTop={mt}>
              <Text wrap={wrapMode}>
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
              <Text wrap={wrapMode}>
                {chalk.dim(`${glyphs.lineVertical} `)}
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
          const table = renderMarkdownTableLines(block.segment.table!, {
            termWidth,
            glyphs,
            renderInline: renderInlineText,
          });

          return (
            <Box key={i} flexDirection="column" marginTop={mt}>
              {table.lines.map((line, li) => (
                <Text key={li} wrap={table.stacked ? wrapMode : undefined}>
                  {line}
                </Text>
              ))}
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
