import { Box } from 'ink';
import React, { useCallback, useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { MarkdownSegment } from '../../../utils/index.js';
import { parseMarkdown, normalizeLineEndings } from '../../../utils/index.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import type { StatusType } from '../../../types/componentTypes.js';

export enum MessageType {
  DEVELOPER = 'developer',
  AGENT = 'agent',
}

export type { StatusType };

export interface MessageProps {
  content: string;
  type: MessageType;
  status?: StatusType;
  barColor?: string;
}

export const Message = React.memo(function Message({
  content,
  type,
  status,
  barColor,
}: MessageProps) {
  const { getColor } = useTheme();
  const highlightCode = useSyntaxHighlight();

  const messageColor = useMemo(() => {
    switch (type) {
      case MessageType.DEVELOPER:
        return getColor('secondary');
      case MessageType.AGENT:
        return getColor('primary');
      default:
        return getColor('primary');
    }
  }, [type, getColor]);

  // Determine status: use provided status or default to 'active'
  const messageStatus: StatusType = status || 'active';

  const renderContent = useCallback(() => {
    const backgroundColor =
      type === MessageType.DEVELOPER ? getColor('muted').hex : undefined;

    if (type === MessageType.AGENT) {
      const segments = parseMarkdown(content);

      // Group segments into blocks (text groups vs code blocks)
      type RenderBlock =
        | { type: 'text'; segments: MarkdownSegment[] }
        | { type: 'code'; segment: MarkdownSegment };

      const blocks: RenderBlock[] = [];
      let currentTextGroup: MarkdownSegment[] = [];

      segments.forEach((segment) => {
        if (segment.codeBlock) {
          // Flush current text group
          if (currentTextGroup.length > 0) {
            blocks.push({ type: 'text', segments: currentTextGroup });
            currentTextGroup = [];
          }
          // Add code block
          blocks.push({ type: 'code', segment });
        } else {
          // Collect text segments
          currentTextGroup.push(segment);
        }
      });

      // Flush remaining text
      if (currentTextGroup.length > 0) {
        blocks.push({ type: 'text', segments: currentTextGroup });
      }

      // Render blocks
      return (
        <Box flexDirection="column">
          {blocks.map((block, i) => {
            if (block.type === 'code') {
              // Remove leading and trailing newlines from code to prevent extra spacing
              const code = block.segment.codeBlock!.code.replace(
                /^\n+|\n+$/g,
                ''
              );
              const highlightedCode = highlightCode(
                code,
                block.segment.codeBlock!.language
              );
              return <Text key={i}>{highlightedCode}</Text>;
            } else {
              // Render text segments with formatting preserved
              return (
                <Text key={i} wrap="wrap">
                  {block.segments.map((segment, j) => {
                    let styledText = segment.text;
                    // Apply bold/italic formatting via chalk
                    if (segment.bold && segment.italic) {
                      styledText = messageColor.bold.italic(styledText);
                    } else if (segment.bold) {
                      styledText = messageColor.bold(styledText);
                    } else if (segment.italic) {
                      styledText = messageColor.italic(styledText);
                    } else {
                      styledText = messageColor(styledText);
                    }
                    return <Text key={j}>{styledText}</Text>;
                  })}
                </Text>
              );
            }
          })}
        </Box>
      );
    }

    // Developer messages - show full content with normalized line endings
    const displayContent = normalizeLineEndings(content);

    return (
      <Box>
        <Box backgroundColor={backgroundColor}>
          <Text wrap="wrap">{messageColor(displayContent)}</Text>
        </Box>
      </Box>
    );
  }, [content, type, messageColor, getColor, highlightCode]);

  return (
    <StatusBar status={messageStatus} barColor={barColor}>
      {renderContent()}
      {type === MessageType.DEVELOPER && <Text> </Text>}
    </StatusBar>
  );
});
