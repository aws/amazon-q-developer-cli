import React, { memo } from 'react';
import { Box, Text } from 'ink';
import {
  renderContentBlock,
  type ContentBlock,
} from '../../utils/message-parser';

interface MessageContent {
  type: 'user' | 'assistant' | 'tool';
  blocks: ContentBlock[];
  isStreaming?: boolean;
  timestamp: Date;
}

interface MessageRendererProps {
  message: MessageContent;
  isStreaming?: boolean;
}

const getBorderColor = (type: string): string => {
  switch (type) {
    case 'user':
      return 'magenta';
    case 'assistant':
      return 'blue';
    case 'tool':
      return 'green';
    default:
      return 'gray';
  }
};

export const MessageRenderer: React.FC<MessageRendererProps> = memo(
  ({ message, isStreaming = false }) => {
    const borderColor = getBorderColor(message.type);

    return (
      <Box flexDirection="column" marginBottom={1}>
        <Box>
          <Text color={borderColor} bold>
            {message.type === 'user'
              ? '│ '
              : message.type === 'assistant'
                ? '│ '
                : '│ 🔧 '}
          </Text>
          <Box flexDirection="column">
            {message.blocks.map((block, index) => {
              const rendered = renderContentBlock(block);

              return (
                <Box key={index} flexDirection="column">
                  {block.type === 'code' && (
                    <Text dimColor>
                      {block.language && `Language: ${block.language}`}
                    </Text>
                  )}
                  <Text>{rendered}</Text>
                </Box>
              );
            })}
            {isStreaming && <Text color="yellow">...</Text>}
          </Box>
        </Box>
      </Box>
    );
  }
);
