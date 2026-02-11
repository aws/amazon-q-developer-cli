import React, { useMemo, useEffect } from 'react';
import { Box, Text } from 'ink';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_LINES = 5;

export interface ShellProps {
  /** The tool name/action (e.g., "Running", "Ran") */
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
}: ShellProps) {
  const { getColor } = useTheme();

  let statusBarContext: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBarContext = useStatusBar();
  } catch {
    // Not within a StatusBar context
  }

  const { setStatus } = statusBarContext ?? {
    setStatus: () => {},
  };

  // Truncate command for display if too long
  const displayCommand = command
    ? command.length > 60
      ? command.slice(0, 57) + '...'
      : command
    : undefined;

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
    if (!result || result.status !== 'success') {
      return { output: null, exitCode: null };
    }

    const rawOutput = result.output;
    let outputStr: string | null = null;
    let code: number | null = null;

    // Handle the nested structure: {items: [{Json: {exit_status, stdout, stderr}}]}
    if (rawOutput && typeof rawOutput === 'object') {
      let obj = rawOutput as Record<string, unknown>;

      // Unwrap items array if present
      if ('items' in obj && Array.isArray(obj.items) && obj.items.length > 0) {
        const firstItem = obj.items[0] as Record<string, unknown>;
        if ('Json' in firstItem && typeof firstItem.Json === 'object') {
          obj = firstItem.Json as Record<string, unknown>;
        } else {
          obj = firstItem;
        }
      }

      // Get exit status
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

      // Prefer stdout, fall back to stderr if stdout is empty
      if ('stdout' in obj && typeof obj.stdout === 'string' && obj.stdout.trim()) {
        outputStr = obj.stdout;
      } else if ('stderr' in obj && typeof obj.stderr === 'string' && obj.stderr.trim()) {
        outputStr = obj.stderr;
      }
    } else if (typeof rawOutput === 'string') {
      outputStr = rawOutput;
    }

    return { output: outputStr, exitCode: code };
  }, [result]);

  const hasOutput = output && output.trim().length > 0;

  // Calculate line count
  const outputLines = useMemo(() => {
    if (!output) return [];
    return output.split('\n');
  }, [output]);

  // Use expandable output hook
  const { expanded, hiddenCount } = useExpandableOutput({
    totalItems: outputLines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
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
    const content = <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />;
    if (noStatusBar) return content;
    return <StatusBar status={status}>{content}</StatusBar>;
  }

  // Static: show only command, no output
  if (isStatic || (!hasOutput && !errorMessage)) {
    return <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />;
  }

  // Show error message if present
  if (errorMessage) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
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
        <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
        <Box marginLeft={2} flexDirection="column">
          {outputLines.map((line, i) => (
            <Text key={i}>{getColor('secondary')(line)}</Text>
          ))}
        </Box>
      </Box>
    );
  }

  // Collapsed view: show first N lines + hint
  return (
    <Box flexDirection="column">
      <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
      <Box marginLeft={2} flexDirection="column">
        {outputLines.slice(0, PREVIEW_LINES).map((line, i) => (
          <Text key={i}>{getColor('secondary')(line)}</Text>
        ))}
        {hiddenCount > 0 && (
          <Text>
            {getColor('secondary')(`...+${hiddenCount} lines (^O to expand)`)}
          </Text>
        )}
      </Box>
    </Box>
  );
});
