import React, { useMemo, useEffect } from 'react';
import { Box, Text } from 'ink';
import { Shell } from '../chat/tools/Shell.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useStatusBar } from '../chat/status-bar/StatusBar.js';
import { useAppStore, type ToolResult } from '../../stores/app-store.js';

export interface ShellToolMessageProps {
  id: string;
  content: string;
  isFinished?: boolean;
  result?: ToolResult;
}

/**
 * Renders shell/execute_bash tool calls with collapsible output.
 * Uses Shell component with noStatusBar since ToolUseMessage provides the StatusBar.
 * 
 * - Collapsed: shows first 3 lines + "...+X lines (^O to expand)"
 * - Expanded: shows full output (controlled by global toolOutputsExpanded state)
 * - Sets error status on non-zero exit code
 */
export const ShellToolMessage: React.FC<ShellToolMessageProps> = ({ 
  id,
  content, 
  isFinished = false,
  result,
}) => {
  const { getColor } = useTheme();
  const { setStatus, requestRemeasure } = useStatusBar();
  const expanded = useAppStore((state) => state.toolOutputsExpanded);
  const setHasExpandableToolOutputs = useAppStore((state) => state.setHasExpandableToolOutputs);
  
  const command = useMemo(() => {
    try {
      const parsed = JSON.parse(content);
      return parsed.command || 'command';
    } catch {
      return 'command';
    }
  }, [content]);

  // Truncate command for display if too long
  const displayCommand = command.length > 60 
    ? command.slice(0, 57) + '...' 
    : command;

  const title = isFinished ? 'Ran' : 'Running';

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
        // Unwrap Json wrapper if present
        if ('Json' in firstItem && typeof firstItem.Json === 'object') {
          obj = firstItem.Json as Record<string, unknown>;
        } else {
          obj = firstItem;
        }
      }
      
      // Get exit status
      if ('exit_status' in obj) {
        const status = obj.exit_status;
        if (typeof status === 'number') {
          code = status;
        } else if (typeof status === 'string') {
          // Parse "exit status: 0" or just "0"
          const match = status.match(/(\d+)/);
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

  const lineCount = outputLines.length;

  // Preview lines for collapsed view
  const previewLines = 3;
  const remainingLines = lineCount - previewLines;
  const hasExpandableOutput = hasOutput && remainingLines > 0;

  // Register that we have expandable output
  useEffect(() => {
    if (hasExpandableOutput) {
      setHasExpandableToolOutputs(true);
    }
  }, [hasExpandableOutput, setHasExpandableToolOutputs]);

  // Request remeasure when expanded state changes
  useEffect(() => {
    requestRemeasure();
  }, [expanded, requestRemeasure]);

  // Set error status if command failed (non-zero exit code)
  useEffect(() => {
    if (isFinished && exitCode !== null && exitCode !== 0) {
      setStatus('error');
    }
  }, [isFinished, exitCode, setStatus]);

  if (!hasOutput) {
    return <Shell name={title} command={displayCommand} noStatusBar />;
  }

  if (expanded) {
    return (
      <Box flexDirection="column">
        <Shell name={title} command={displayCommand} noStatusBar />
        <Box marginLeft={2} flexDirection="column">
          {outputLines.map((line, i) => (
            <Text key={i}>{getColor('secondary')(line)}</Text>
          ))}
        </Box>
      </Box>
    );
  }

  // Collapsed view: show first 3 lines + hint
  return (
    <Box flexDirection="column">
      <Shell name={title} command={displayCommand} noStatusBar />
      <Box marginLeft={2} flexDirection="column">
        {outputLines.slice(0, previewLines).map((line, i) => (
          <Text key={i}>{getColor('secondary')(line)}</Text>
        ))}
        {remainingLines > 0 && (
          <Text>{getColor('secondary')(`...+${remainingLines} lines (^O to expand)`)}</Text>
        )}
      </Box>
    </Box>
  );
};
