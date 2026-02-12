import React, { useEffect, useMemo } from 'react';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Text } from '../ui/text/Text.js';
import { StatusBar, useStatusBar } from '../chat/status-bar/StatusBar.js';
import type { StatusType } from '../../types/componentTypes.js';
import { Write } from '../chat/tools/Write.js';
import { Read } from '../chat/tools/Read.js';
import { Shell } from '../chat/tools/Shell.js';
import { Grep } from '../chat/tools/Grep.js';
import { Glob } from '../chat/tools/Glob.js';
import { WebSearch } from '../chat/tools/WebSearch.js';
import { WebFetch } from '../chat/tools/WebFetch.js';
import { Tool } from '../chat/tools/Tool.js';
import { ToolUseStatus, type ToolResult } from '../../stores/app-store.js';
import { WRITE_TOOL_NAMES, READ_TOOL_NAMES, SHELL_TOOL_NAMES, WEB_SEARCH_TOOL_NAMES, WEB_FETCH_TOOL_NAMES, GREP_TOOL_NAMES, GLOB_TOOL_NAMES, type ToolKind, type ToolCallLocation } from '../../types/agent-events.js';

export interface ToolUseMessageProps {
  id: string;
  name: string;
  content: string;
  isFinished?: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  kind?: ToolKind;
  locations?: ToolCallLocation[];
  barColor?: string;
  isStatic?: boolean;
}

export const ToolUseMessage = React.memo<ToolUseMessageProps>(function ToolUseMessage({
  id,
  name,
  content,
  isFinished = false,
  status,
  result,
  kind,
  locations,
  barColor,
  isStatic = false,
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

  return (
    <StatusBar status={statusIcon} barColor={barColor}>
      <ToolUseContent
        name={name}
        content={content}
        isFinished={isFinished}
        status={status}
        result={result}
        isStatic={isStatic}
        locations={locations}
      />
    </StatusBar>
  );
});

/** Inner component that lives inside StatusBar so it can call requestRemeasure */
function ToolUseContent({
  name,
  content,
  isFinished,
  status,
  result,
  isStatic,
  locations,
}: {
  name: string;
  content: string;
  isFinished: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  isStatic: boolean;
  locations?: ToolCallLocation[];
}) {
  const { getColor } = useTheme();
  const { requestRemeasure } = useStatusBar();

  // Remeasure when status or isFinished changes — these change the rendered content height
  useEffect(() => {
    requestRemeasure();
  }, [status, isFinished, requestRemeasure]);

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
    return (
      <Write
        oldText=""
        newText=""
        content={content}
        isFinished={isFinished}
      />
    );
  }

  if (READ_TOOL_NAMES.has(name)) {
    const title = isFinished ? 'Read' : 'Reading';
    return (
      <Read
        name={title}
        noStatusBar
        isFinished={isFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  if (SHELL_TOOL_NAMES.has(name)) {
    const title = isFinished ? 'Ran' : 'Running';
    let command: string | undefined;
    try {
      const parsed = JSON.parse(content);
      command = parsed.command;
    } catch { /* ignore */ }
    return (
      <Shell
        name={title}
        command={command}
        noStatusBar
        isFinished={isFinished}
        isStatic={isStatic}
        result={result}
      />
    );
  }

  if (WEB_SEARCH_TOOL_NAMES.has(name)) {
    return (
      <WebSearch
        isFinished={isFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (WEB_FETCH_TOOL_NAMES.has(name)) {
    return (
      <WebFetch
        isFinished={isFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (GREP_TOOL_NAMES.has(name)) {
    return (
      <Grep
        noStatusBar
        isFinished={isFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (GLOB_TOOL_NAMES.has(name)) {
    return (
      <Glob
        noStatusBar
        isFinished={isFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  // Fallback: use generic Tool component
  return (
    <Tool
      name={name}
      noStatusBar
      isFinished={isFinished}
      isStatic={isStatic}
      result={result}
      locations={locations}
    />
  );
}
