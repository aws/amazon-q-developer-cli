import React, { useMemo, useEffect } from 'react';
import { Box, Text } from './../../../renderer.js';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { unwrapResultOutput } from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { normalizeLineEndings } from '../../../utils/string.js';
import { useAppStore, type ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_LINES = 5;

export interface ShellProps {
  /** The tool name/action */
  name: string;
  /** The bash command to display */
  command?: string;
  /** Tool call ID — used to subscribe to live output from the store */
  toolCallId?: string;
  /** Tool status type */
  status?: StatusType;
  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;
  /** Whether the command has finished executing */
  isFinished?: boolean;
  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;
  /** Tool execution result containing output/error */
  result?: ToolResult;
  /** Raw JSON content from tool call args */
  content?: string;
}

/**
 * Shell tool component for displaying bash command execution.
 */
export const Shell = React.memo(function Shell({
  name,
  command,
  toolCallId,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  result,
  content,
}: ShellProps) {
  const { getColor } = useTheme();

  // Subscribe to live output directly from the store — only this Shell
  // re-renders when new output arrives, not the entire ConversationView.
  const liveOutput = useAppStore((s) =>
    toolCallId ? s.liveOutputs.get(toolCallId) : undefined
  );

  const params = useMemo(
    () => formatToolParams(content, ['command']),
    [content]
  );

  let statusBarContext: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBarContext = useStatusBar();
  } catch {
    // Not within a StatusBar context
  }

  const { setStatus } = statusBarContext ?? {
    setStatus: () => {},
  };

  const displayCommand = command || undefined;

  const isTimeoutError = useMemo(() => {
    if (result?.status === 'error') {
      const errorMsg = result.error.toLowerCase();
      return errorMsg.includes('timeout') || errorMsg.includes('timed out');
    }
    return false;
  }, [result]);

  const errorMessage = useMemo(() => {
    if (result?.status === 'error') {
      return isTimeoutError ? 'Command timed out' : result.error;
    }
    return null;
  }, [result, isTimeoutError]);

  // Unify output: result lines when finished, live output during execution.
  const { outputLines, exitCode } = useMemo(() => {
    if (result) {
      const { obj, text } = unwrapResultOutput(result);

      if (text) {
        return {
          outputLines: normalizeLineEndings(text).split('\n'),
          exitCode: null as number | null,
        };
      }
      if (!obj)
        return { outputLines: [] as string[], exitCode: null as number | null };

      let code: number | null = null;
      if ('exit_status' in obj) {
        const exitStatus = obj.exit_status;
        if (typeof exitStatus === 'number') {
          code = exitStatus;
        } else if (typeof exitStatus === 'string') {
          const match = exitStatus.match(/(\d+)/);
          if (match && match[1]) {
            code = parseInt(match[1], 10);
          }
        }
      }

      let outputStr: string | null = null;
      if (
        'stdout' in obj &&
        typeof obj.stdout === 'string' &&
        obj.stdout.trim()
      ) {
        outputStr = obj.stdout;
      } else if (
        'stderr' in obj &&
        typeof obj.stderr === 'string' &&
        obj.stderr.trim()
      ) {
        outputStr = obj.stderr;
      }

      return {
        outputLines: outputStr
          ? normalizeLineEndings(outputStr).split('\n')
          : [],
        exitCode: code,
      };
    }

    if (liveOutput && liveOutput.length > 0) {
      return { outputLines: liveOutput, exitCode: null as number | null };
    }

    return { outputLines: [] as string[], exitCode: null as number | null };
  }, [result, liveOutput]);

  const hasOutput = outputLines.length > 0;

  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: outputLines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
    unit: 'lines',
  });

  useEffect(() => {
    if (isFinished && exitCode !== null && exitCode !== 0) {
      setStatus('error');
    } else if (isTimeoutError) {
      setStatus('error');
    }
  }, [isFinished, exitCode, isTimeoutError, setStatus]);

  // Simple mode: no output yet
  if (!result && !liveOutput) {
    const simpleContent = (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
      </Box>
    );
    if (noStatusBar) return simpleContent;
    return <StatusBar status={status}>{simpleContent}</StatusBar>;
  }

  // Static or empty
  if (isStatic || (!hasOutput && !errorMessage)) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
      </Box>
    );
  }

  // Error
  if (errorMessage) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
        <Box marginLeft={2}>
          <Text>{getColor('error')(errorMessage)}</Text>
        </Box>
      </Box>
    );
  }

  // Expanded: single <Text> with all output lines.
  if (expanded) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
        <Box marginLeft={2} flexDirection="column">
          <Text>{getColor('primary')(outputLines.join('\n'))}</Text>
        </Box>
      </Box>
    );
  }

  // Collapsed: single <Text>, tail during execution, head after completion.
  const previewLines = isFinished
    ? outputLines.slice(0, PREVIEW_LINES)
    : outputLines.slice(-PREVIEW_LINES);

  return (
    <Box flexDirection="column">
      <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
      <ToolMeta params={params} />
      <Box marginLeft={2} flexDirection="column">
        {!isFinished && hiddenCount > 0 && (
          <Text>
            {getColor('secondary')(
              `...+${hiddenCount} lines above (ctrl+o to toggle)`
            )}
          </Text>
        )}
        <Text>{getColor('primary')(previewLines.join('\n'))}</Text>
        {isFinished && expandHint && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
      </Box>
    </Box>
  );
});
