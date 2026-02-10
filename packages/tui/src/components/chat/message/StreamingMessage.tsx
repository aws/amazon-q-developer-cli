import { Box, measureElement, useInput } from 'ink';
import React, { useRef, useState, useLayoutEffect, useEffect } from 'react';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useStreamingBuffer } from '../../../stores/selectors.js';
import { Message, MessageType, type StatusType } from './Message.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { ActionHint } from '../../ui/hint/ActionHint.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

// Reserve space for: status bar, prompt bar, context bar, margins
const RESERVED_LINES = 8;

export interface StreamingMessageProps {
  content: string;
  type: MessageType;
  status?: StatusType;
  isStreaming: boolean;
  barColor?: string;
}

/**
 * A message component that collapses when content exceeds viewport during streaming.
 * When overflow is detected, it signals the store to stop updating and shows frozen content.
 * User can press Ctrl+R to resume normal streaming.
 */
export const StreamingMessage = React.memo(function StreamingMessage({
  content,
  type,
  status,
  isStreaming,
  barColor,
}: StreamingMessageProps) {
  const { height: terminalHeight } = useTerminalSize();
  const { getColor } = useTheme();
  const contentRef = useRef<any>(null);
  
  // Get the buffering functions from store (typed, using shallow selector)
  const { startBuffering, stopBuffering } = useStreamingBuffer();
  
  const [isBuffering, setIsBuffering] = useState(false);
  const [frozenContent, setFrozenContent] = useState('');
  // Track if user manually expanded - prevents re-buffering
  const userExpandedRef = useRef(false);

  // Calculate available height
  const availableHeight = Math.max(5, terminalHeight - RESERVED_LINES);

  // Handle Ctrl+R to resume streaming
  useInput((input, key) => {
    if (isBuffering && isStreaming && key.ctrl && input === 'r') {
      userExpandedRef.current = true;
      setIsBuffering(false);
      setFrozenContent('');
      if (stopBuffering) {
        stopBuffering();
      }
    }
  }, { isActive: isBuffering && isStreaming });

  // Check for overflow
  useLayoutEffect(() => {
    if (!isStreaming) {
      // Streaming ended - reset buffering
      if (isBuffering) {
        setIsBuffering(false);
        setFrozenContent('');
      }
      return;
    }

    // Don't re-buffer if user manually expanded
    if (userExpandedRef.current) {
      return;
    }

    if (isBuffering) {
      // Already buffering - don't check again
      return;
    }

    if (contentRef.current) {
      try {
        const { height } = measureElement(contentRef.current);
        if (height >= availableHeight) {
          // Start buffering - freeze current content and tell store to stop updating
          setIsBuffering(true);
          setFrozenContent(content);
          if (startBuffering) {
            startBuffering();
          }
        }
      } catch {
        // Measurement failed
      }
    }
  }, [content, isStreaming, isBuffering, availableHeight, startBuffering]);

  // Reset on new stream
  useEffect(() => {
    if (!isStreaming) {
      setIsBuffering(false);
      setFrozenContent('');
      userExpandedRef.current = false;
    }
  }, [isStreaming]);

  const brandColor = getColor('brand');

  // Buffering mode - show frozen content with indicator
  if (isBuffering && isStreaming) {
    return (
      <Box flexDirection="column">
        <Message content={frozenContent} type={type} status="active" barColor={barColor} />
        <StatusBar status="paused">
          <Box gap={1}>
            <Text>{brandColor('Streaming...')}</Text>
            <ActionHint text="(^R to expand)" />
          </Box>
        </StatusBar>
      </Box>
    );
  }

  // Normal rendering
  return (
    <Box ref={contentRef}>
      <Message content={content} type={type} status={status} barColor={barColor} />
    </Box>
  );
});
