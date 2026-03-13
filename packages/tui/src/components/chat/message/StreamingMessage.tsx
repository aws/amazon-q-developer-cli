import React from 'react';
import { StreamingPanel } from '../.././../renderer.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Message, MessageType, type StatusType } from './Message.js';

export interface StreamingMessageProps {
  content: string;
  type: MessageType;
  status?: StatusType;
  isStreaming: boolean;
  barColor?: string;
  onReadyToFlush?: () => void;
}

const CHROME_LINES = 13;

export const StreamingMessage = React.memo(function StreamingMessage({
  content,
  type,
  status,
  isStreaming,
  barColor,
  onReadyToFlush,
}: StreamingMessageProps) {
  const { height: terminalHeight } = useTerminalSize();
  const viewportHeight = Math.max(5, terminalHeight - CHROME_LINES);

  return (
    <StreamingPanel
      content={content}
      streaming={isStreaming}
      height={viewportHeight}
      onReadyToFlush={onReadyToFlush}
    >
      {(fullContent) => (
        <Message
          content={fullContent}
          type={type}
          status={isStreaming ? 'active' : status}
          barColor={barColor}
        />
      )}
    </StreamingPanel>
  );
});
