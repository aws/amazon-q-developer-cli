import React, { useMemo } from 'react';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Text } from '../ui/text/Text.js';
import { StatusBar } from '../chat/status-bar/StatusBar.js';
import type { StatusType } from '../../types/componentTypes.js';
import { WriteToolMessage } from './WriteToolMessage.js';
import { ReadToolMessage } from './ReadToolMessage.js';
import { ShellToolMessage } from './ShellToolMessage.js';
import { ToolUseStatus, type ToolResult } from '../../stores/app-store.js';
import { WRITE_TOOL_NAMES, READ_TOOL_NAMES, SHELL_TOOL_NAMES } from '../../types/agent-events.js';

export interface ToolUseMessageProps {
  id: string;
  name: string;
  content: string;
  isFinished?: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
}

export const ToolUseMessage = React.memo<ToolUseMessageProps>(function ToolUseMessage({
  id,
  name,
  content,
  isFinished = false,
  status,
  result,
}) {
  const { getColor } = useTheme();

  // Map tool status to StatusBar status icon
  const statusIcon: StatusType | undefined = useMemo(() => {
    if (status === ToolUseStatus.Rejected) return 'error';
    if (status === ToolUseStatus.Approved && isFinished) return 'success';
    if (status === ToolUseStatus.Pending) return 'warning';
    if (isFinished) return 'success';
    return undefined; // In progress, no icon
  }, [status, isFinished]);

  const renderContent = () => {
    if (status === ToolUseStatus.Rejected) {
      try {
        const parsed = JSON.parse(content);
        const path = parsed.path || parsed.command || 'file';
        return <Text>{getColor('error')(`Rejected: ${path}`)}</Text>;
      } catch {
        return <Text>{getColor('error')(`Rejected: ${name}`)}</Text>;
      }
    }

    if (WRITE_TOOL_NAMES.has(name)) {
      return <WriteToolMessage content={content} />;
    }

    if (READ_TOOL_NAMES.has(name)) {
      return <ReadToolMessage content={content} isFinished={isFinished} />;
    }

    if (SHELL_TOOL_NAMES.has(name)) {
      return <ShellToolMessage id={id} content={content} isFinished={isFinished} result={result} />;
    }

    if (!isFinished) {
      return <Text>{getColor('secondary')(`Using ${name}...`)}</Text>;
    }

    return <Text>{getColor('info')(`Used ${name}`)}</Text>;
  };

  return (
    <StatusBar status={statusIcon}>
      {renderContent()}
    </StatusBar>
  );
});
