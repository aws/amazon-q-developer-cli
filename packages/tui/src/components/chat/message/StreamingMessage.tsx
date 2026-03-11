import React from 'react';
import { Box } from '../.././../renderer.js';
import { Message, MessageType, type StatusType } from './Message.js';

export interface StreamingMessageProps {
  content: string;
  type: MessageType;
  status?: StatusType;
  isStreaming: boolean;
  barColor?: string;
}

export const StreamingMessage = React.memo(function StreamingMessage({
  content,
  type,
  status,
  isStreaming,
  barColor,
}: StreamingMessageProps) {
  return (
    <Box flexDirection="column">
      <Message
        content={content}
        type={type}
        status={isStreaming ? 'active' : status}
        barColor={barColor}
      />
    </Box>
  );
});
