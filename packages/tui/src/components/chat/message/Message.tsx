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
  const { wrapDisabled } = useTheme();
  const messageStatus: StatusType = status || 'active';

  // Under wrapDisabled:
  //   - Always use wrap="overflow" in both live and static. Terminal soft-wraps
  //     wide lines visually; copy-paste preserves the logical line.
  //   - Drop the StatusBar left-bar chrome entirely (live and static) so
  //     there is no leading whitespace or colored bar column. Keeps the
  //     output clean in scrollback AND avoids layout-shift between live and
  //     static forms.
  const useOverflow = wrapDisabled;
  const skipStatusBar = wrapDisabled;

  if (skipStatusBar) {
    return (
      <>
        <MessageContent content={content} type={type} useOverflow={true} />
        {type === MessageType.DEVELOPER && <Text> </Text>}
      </>
    );
  }

  return (
    <StatusBar status={messageStatus} barColor={barColor}>
      <MessageContent content={content} type={type} useOverflow={useOverflow} />
      {type === MessageType.DEVELOPER && <Text> </Text>}
    </StatusBar>
  );
});

const MessageContent = React.memo(function MessageContent({
  content,
  type,
  useOverflow,
}: {
  content: string;
  type: MessageType;
  useOverflow: boolean;
}) {
  const { getUserPromptColor, getUserPromptBgHex, getUserResponseColor } =
    useTheme();
  const { requestRemeasure } = useStatusBar();

  const messageColor = useMemo(
    () =>
      type === MessageType.AGENT
        ? getUserResponseColor()
        : getUserPromptColor(),
    [type, getUserResponseColor, getUserPromptColor]
  );

  // Remeasure when content length changes (new lines during streaming)
  const lineCount = useMemo(() => content.split('\n').length, [content]);
  useEffect(() => {
    requestRemeasure();
  }, [lineCount, requestRemeasure]);

  if (type === MessageType.AGENT) {
    return (
      <MarkdownRenderer
        content={content}
        color={messageColor}
        useOverflow={useOverflow}
      />
    );
  }

  const backgroundColor = getUserPromptBgHex();
  const displayContent = expandTabs(normalizeLineEndings(content));
  // Cast to any: twinki's Text supports "overflow" but ink's type signature doesn't list it.
  const wrapMode: any = useOverflow ? 'overflow' : 'wrap';
  return (
    <Box>
      <Box backgroundColor={backgroundColor}>
        <Text wrap={wrapMode}>{messageColor(displayContent)}</Text>
      </Box>
    </Box>
  );
});
