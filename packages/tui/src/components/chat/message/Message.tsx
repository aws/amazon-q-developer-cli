import { Box } from './../../../renderer.js';
import React, { useCallback, useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { normalizeLineEndings, expandTabs } from '../../../utils/index.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { MarkdownRenderer } from '../../ui/MarkdownRenderer.js';
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

  const messageColor = useMemo(() => {
    switch (type) {
      case MessageType.DEVELOPER:
        return getColor('primary');
      case MessageType.AGENT:
        return getColor('primary');
      default:
        return getColor('primary');
    }
  }, [type, getColor]);

  const messageStatus: StatusType = status || 'active';

  const renderContent = useCallback(() => {
    if (type === MessageType.AGENT) {
      return <MarkdownRenderer content={content} color={messageColor} />;
    }

    // Developer messages
    const backgroundColor = getColor('surface').hex;
    const displayContent = expandTabs(normalizeLineEndings(content));
    return (
      <Box>
        <Box backgroundColor={backgroundColor}>
          <Text wrap="wrap">{messageColor(displayContent)}</Text>
        </Box>
      </Box>
    );
  }, [content, type, messageColor, getColor]);

  return (
    <StatusBar status={messageStatus} barColor={barColor}>
      {renderContent()}
      {type === MessageType.DEVELOPER && <Text> </Text>}
    </StatusBar>
  );
});
