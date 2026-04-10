import React, { useEffect, useMemo } from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { StatusBar, useStatusBar } from '../chat/status-bar/StatusBar.js';
import { StatusInfo } from './status/StatusInfo.js';
import type { StatusType } from '../../types/componentTypes.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Write } from '../chat/tools/Write.js';
import { Read } from '../chat/tools/Read.js';
import { Shell } from '../chat/tools/Shell.js';
import { Grep } from '../chat/tools/Grep.js';
import { Glob } from '../chat/tools/Glob.js';
import { Ls } from '../chat/tools/Ls.js';
import { Code } from '../chat/tools/Code.js';
import { Introspect } from '../chat/tools/Introspect.js';
import { ImageRead } from '../chat/tools/ImageRead.js';
import { WebSearch } from '../chat/tools/WebSearch.js';
import { WebFetch } from '../chat/tools/WebFetch.js';
import { SessionTool } from '../chat/tools/SessionTool.js';
import { Tool } from '../chat/tools/Tool.js';
import { ToolUseStatus, type ToolResult } from '../../stores/app-store.js';
import {
  WRITE_TOOL_NAMES,
  READ_TOOL_NAMES,
  SHELL_TOOL_NAMES,
  WEB_SEARCH_TOOL_NAMES,
  WEB_FETCH_TOOL_NAMES,
  GREP_TOOL_NAMES,
  GLOB_TOOL_NAMES,
  LS_TOOL_NAMES,
  CODE_TOOL_NAMES,
  SESSION_TOOL_NAMES,
  resolveToolId,
  INTROSPECT_TOOL_NAMES,
  IMAGE_READ_TOOL_NAMES,
  TASK_TOOL_NAMES,
  type ToolKind,
  type ToolCallLocation,
} from '../../types/agent-events.js';
import { getToolLabel } from '../../types/tool-status.js';

export interface ToolUseMessageProps {
  id: string;
  name: string;
  content: string;
  liveOutput?: string;
  isFinished?: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  kind?: ToolKind;
  locations?: ToolCallLocation[];
  barColor?: string;
  isStatic?: boolean;
  /** If set, shows a colored agent name prefix (for subagent tool calls) */
  agentLabel?: string;
  agentLabelColor?: string;
}

export const ToolUseMessage = React.memo<ToolUseMessageProps>(
  function ToolUseMessage({
    name,
    content,
    isFinished = false,
    status,
    result,
    locations,
    barColor,
    isStatic = false,
    agentLabel,
    agentLabelColor,
  }) {
    const { getColor } = useTheme();
    // Map tool status to StatusBar status icon
    const statusIcon: StatusType | undefined = useMemo(() => {
      if (status === ToolUseStatus.Rejected) return 'error';
      if (result?.status === 'cancelled') return 'error';
      if (result?.status === 'error') return 'error';
      if (status === ToolUseStatus.Approved && isFinished) return 'success';
      if (status === ToolUseStatus.Pending) return 'paused';
      if (isFinished) return 'success';
      return 'executing';
    }, [status, isFinished, result]);

    const showEscHint = statusIcon === 'executing' && !isStatic;

    return (
      <StatusBar status={statusIcon} barColor={barColor}>
        {agentLabel && (
          <Box>
            <InkText color={agentLabelColor ?? 'gray'} dimColor>
              {agentLabel}
            </InkText>
          </Box>
        )}
        <ToolUseContent
          name={name}
          content={content}
          isFinished={isFinished}
          status={status}
          result={result}
          isStatic={isStatic}
          locations={locations}
        />
        {showEscHint && <Text>{getColor('muted')('esc to cancel')}</Text>}
      </StatusBar>
    );
  }
);

/** Inner component — lives inside StatusBar to access requestRemeasure */
const ToolUseContent = React.memo(function ToolUseContent({
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
  const { requestRemeasure } = useStatusBar();

  // A tool is only visually complete if it's finished AND no longer pending approval
  const effectiveFinished = isFinished && status !== ToolUseStatus.Pending;

  // Remeasure when status or isFinished changes — these change the rendered content height
  useEffect(() => {
    requestRemeasure();
  }, [status, isFinished, requestRemeasure]);

  if (status === ToolUseStatus.Rejected || result?.status === 'cancelled') {
    try {
      const parsed = JSON.parse(content);
      const path = parsed.path || parsed.command || 'file';
      const label = result?.status === 'cancelled' ? 'Cancelled' : 'Rejected';
      return <StatusInfo title={label} target={path} />;
    } catch {
      const label = result?.status === 'cancelled' ? 'Cancelled' : 'Rejected';
      return <StatusInfo title={label} target={name} />;
    }
  }

  if (WRITE_TOOL_NAMES.has(name)) {
    // Extract start line from locations for accurate diff line numbers
    const startLine = locations?.[0]?.line;
    return (
      <Write
        oldText=""
        newText=""
        content={content}
        isFinished={effectiveFinished}
        isStatic={isStatic}
        startLine={startLine}
      />
    );
  }

  if (READ_TOOL_NAMES.has(name)) {
    return (
      <Read
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  if (SHELL_TOOL_NAMES.has(name)) {
    const title = getToolLabel('shell');
    let command: string | undefined;
    try {
      const parsed = JSON.parse(content);
      command = parsed.command;
    } catch {
      /* ignore */
    }
    return (
      <Shell
        name={title}
        command={command}
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        result={result}
        content={content}
      />
    );
  }

  if (WEB_SEARCH_TOOL_NAMES.has(name)) {
    return (
      <WebSearch
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (WEB_FETCH_TOOL_NAMES.has(name)) {
    return (
      <WebFetch
        isFinished={effectiveFinished}
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
        isFinished={effectiveFinished}
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
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  // TODO: Remove Ls and ImageRead branches once legacy tool names are cleaned up.
  // These only render for old saved conversations that had separate ls/imageRead tool calls.
  if (LS_TOOL_NAMES.has(name)) {
    return (
      <Ls
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (CODE_TOOL_NAMES.has(name)) {
    return (
      <Code
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (SESSION_TOOL_NAMES.has(name)) {
    return (
      <SessionTool
        name={name}
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (INTROSPECT_TOOL_NAMES.has(name)) {
    return (
      <Introspect
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (IMAGE_READ_TOOL_NAMES.has(name)) {
    return (
      <ImageRead
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  // Task tool — show a compact one-liner since the Activity Tray surfaces task state
  if (TASK_TOOL_NAMES.has(name)) {
    const labels: Record<string, string> = {
      create: 'Task list created',
      complete: 'Tasks updated',
      add: 'Tasks added',
      remove: 'Tasks removed',
      list: 'Tasks listed',
    };
    let label = 'Task';
    try {
      const parsed = JSON.parse(content);
      if (parsed.command && labels[parsed.command]) {
        label = labels[parsed.command]!;
      }
    } catch {
      /* ignore */
    }
    return <StatusInfo title={label} />;
  }

  // Fallback: use generic Tool component
  // For unrecognized tools that failed, show a one-liner matching Rejected/Cancelled pattern
  if (result?.status === 'error' && effectiveFinished) {
    return <FallbackError name={name} content={content} error={result.error} />;
  }

  const toolId = resolveToolId(name);
  const fallbackName = toolId ? getToolLabel(toolId) : name;
  return (
    <Tool
      name={fallbackName}
      noStatusBar
      isFinished={effectiveFinished}
      isStatic={isStatic}
      result={result}
      locations={locations}
      content={content}
    />
  );
});

/** Shows a failed tool call with the error message visible to the user. */
const FallbackError = React.memo(function FallbackError({
  name,
  content,
  error,
}: {
  name: string;
  content: string;
  error: string;
}) {
  const { getColor } = useTheme();
  let target: string;
  try {
    const parsed = JSON.parse(content);
    target = parsed.path || parsed.command || parsed.pattern || name;
  } catch {
    target = name;
  }
  return (
    <Box flexDirection="column">
      <StatusInfo title="Failed" target={target} />
      <Box marginLeft={2}>
        <Text>{getColor('error')(error)}</Text>
      </Box>
    </Box>
  );
});
