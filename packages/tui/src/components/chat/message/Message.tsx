import { Box } from './../../../renderer.js';
import React, { useEffect, useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { normalizeLineEndings, expandTabs } from '../../../utils/index.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
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
  const messageStatus: StatusType = status || 'active';

  return (
    <StatusBar status={messageStatus} barColor={barColor}>
      <MessageContent content={content} type={type} />
      {type === MessageType.DEVELOPER && <Text> </Text>}
    </StatusBar>
  );
});

const MessageContent = React.memo(function MessageContent({
  content,
  type,
}: {
  content: string;
  type: MessageType;
}) {
  const { getColor } = useTheme();
  const { requestRemeasure } = useStatusBar();

  const messageColor = useMemo(() => getColor('primary'), [getColor]);

  // Remeasure when content length changes (new lines during streaming)
  const lineCount = useMemo(() => content.split('\n').length, [content]);
  useEffect(() => {
    requestRemeasure();
  }, [lineCount, requestRemeasure]);

  if (type === MessageType.AGENT) {
    return <MarkdownRenderer content={content} color={messageColor} />;
  }

  const backgroundColor = getColor('surface').hex;
  const displayContent = expandTabs(normalizeLineEndings(content));
  return (
    <Box>
      <Box backgroundColor={backgroundColor}>
        <Text wrap="wrap">{messageColor(displayContent)}</Text>
      </Box>
    </Box>
  );
});
