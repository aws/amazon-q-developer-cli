import { Box, Text as InkText } from 'ink';
import React, { useCallback } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { MarkdownSegment } from '../../../utils/index.js';
import { parseMarkdown, normalizeLineEndings } from '../../../utils/index.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { Text } from '../../ui/text/Text.js';
import { getColorHex } from '../../../utils/colorUtils.js';
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

export const Message = React.memo(function Message({ content, type, status, barColor }: MessageProps) {
  const { getColor } = useTheme();
  const highlightCode = useSyntaxHighlight();

  const getMessageColor = () => {
    switch (type) {
      case MessageType.DEVELOPER:
        return getColor('secondary');
      case MessageType.AGENT:
        return getColor('primary');
      default:
        return getColor('primary');
    }
  };

  // Determine status: use provided status or default to 'active'
  const messageStatus: StatusType = status || 'active';

  const renderContent = useCallback(() => {
    const messageColorHex = getColorHex(
      getMessageColor(),
      getColor('primary').hex || '#ffffff'
    );

    const backgroundColor = type === MessageType.DEVELOPER ? getColor('muted').hex : undefined;
    
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
              const code = block.segment.codeBlock!.code.replace(/^\n+|\n+$/g, '');
              const highlightedCode = highlightCode(code, block.segment.codeBlock!.language);
              return <Text key={i}>{highlightedCode}</Text>;
            } else {
              // Render text segments with formatting preserved
              return (
                <InkText key={i} wrap="wrap" color={messageColorHex}>
                  {block.segments.map((segment, j) => (
                    <InkText
                      key={j}
                      bold={segment.bold}
                      italic={segment.italic}
                    >
                      {segment.text}
                    </InkText>
                  ))}
                </InkText>
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
          <InkText wrap="wrap" color={messageColorHex}>
            {displayContent}
          </InkText>
        </Box>
      </Box>
    );
  }, [content, type, getMessageColor, getColor, highlightCode]);

  return (
    <StatusBar status={messageStatus} barColor={barColor}>
      {renderContent()}
      {type === MessageType.DEVELOPER && <Text> </Text>}
    </StatusBar>
  );
});
