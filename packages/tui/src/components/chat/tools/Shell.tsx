import React, { useMemo, useEffect } from 'react';
import { Box, Text } from './../../../renderer.js';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { unwrapResultOutput } from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { expandTabs } from '../../../utils/string.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_LINES = 5;

export interface ShellProps {
  /** The tool name/action */
  name: string;

  /** The bash command to display */
  command?: string;

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
 *
 * Features:
 * - Shows command with status indicator
 * - Collapsible output with Ctrl+O expansion
 * - Error display for failed commands or timeouts
 * - Static mode for past turns (no output shown)
 */
export const Shell = React.memo(function Shell({
  name,
  command,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  result,
  content,
}: ShellProps) {
  const { getColor } = useTheme();

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

  // Check for timeout error in result
  const isTimeoutError = useMemo(() => {
    if (result?.status === 'error') {
      const errorMsg = result.error.toLowerCase();
      return errorMsg.includes('timeout') || errorMsg.includes('timed out');
    }
    return false;
  }, [result]);

  // Get error message from result if present
  const errorMessage = useMemo(() => {
    if (result?.status === 'error') {
      return isTimeoutError ? 'Command timed out' : result.error;
    }
    return null;
  }, [result, isTimeoutError]);

  // Extract output and exit status from result
  const { output, exitCode } = useMemo(() => {
    const { obj, text } = unwrapResultOutput(result);

    if (text) return { output: text, exitCode: null };
    if (!obj) return { output: null, exitCode: null };

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

    return { output: outputStr, exitCode: code };
  }, [result]);

  const hasOutput = output && output.trim().length > 0;

  // Calculate line count
  const outputLines = useMemo(() => {
    if (!output) return [];
    return expandTabs(output).split('\n');
  }, [output]);

  // Use expandable output hook
  const { expanded, expandHint } = useExpandableOutput({
    totalItems: outputLines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
    unit: 'lines',
  });

  // Set error status if command failed (non-zero exit code) or timeout error
  useEffect(() => {
    if (isFinished && exitCode !== null && exitCode !== 0) {
      setStatus('error');
    } else if (isTimeoutError) {
      setStatus('error');
    }
  }, [isFinished, exitCode, isTimeoutError, setStatus]);

  // Simple mode: just show command info (no result handling)
  if (!result) {
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

  // Static: show only command, no output
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

  // Show error message if present
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

  // Expanded view: show all output
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
          {outputLines.map((line, i) => (
            <Text key={i}>{getColor('primary')(line)}</Text>
          ))}
        </Box>
      </Box>
    );
  }

  // Collapsed view: show first N lines + hint
  return (
    <Box flexDirection="column">
      <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
      <ToolMeta params={params} />
      <Box marginLeft={2} flexDirection="column">
        {outputLines.slice(0, PREVIEW_LINES).map((line, i) => (
          <Text key={i}>{getColor('primary')(line)}</Text>
        ))}
        {expandHint && <Text>{getColor('secondary')(expandHint)}</Text>}
      </Box>
    </Box>
  );
});
