import React, { useMemo } from 'react';
import { Box, Text } from 'ink';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { unwrapResultOutput } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import type { StatusType } from '../../../types/componentTypes.js';
import type { ToolCallLocation } from '../../../types/agent-events.js';

const PREVIEW_LINES = 3;

export interface ToolProps {
  /** The tool name to display */
  name: string;

  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the tool operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /** Tool execution result containing output/error */
  result?: ToolResult;

  /** File locations associated with the tool call */
  locations?: ToolCallLocation[];

  /** Error message to display */
  errorMessage?: string | null;
}

/**
 * Generic tool component for displaying tool calls with locations and collapsible output.
 *
 * Features:
 * - Shows tool name with status indicator
 * - Displays file locations when provided
 * - Collapsible output with Ctrl+O expansion
 * - Error display for failed tools
 * - Static mode for past turns (no output shown)
 */
export const Tool = React.memo(function Tool({
  name,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  result,
  locations,
  errorMessage,
}: ToolProps) {
  const { getColor } = useTheme();

  const title = isFinished ? 'Used' : 'Using';

  // Format locations for display - each on its own row
  const formattedLocations = useMemo(() => {
    if (!locations || locations.length === 0) return null;

    return locations.map((loc) => {
      const fileName = loc.path.split('/').pop() || loc.path;
      return loc.line ? `${fileName}:${loc.line}` : fileName;
    });
  }, [locations]);

  // Extract and format output from result
  const { output, outputLines } = useMemo(() => {
    const { obj, text } = unwrapResultOutput(result);

    if (text) return { output: text, outputLines: text.split('\n') };
    if (!obj) return { output: null, outputLines: [] };

    let outputStr: string | null = null;
    if ('text' in obj && typeof obj.text === 'string') {
      outputStr = obj.text;
    } else if ('content' in obj && typeof obj.content === 'string') {
      outputStr = obj.content;
    } else if ('result' in obj && typeof obj.result === 'string') {
      outputStr = obj.result;
    } else {
      try {
        outputStr = JSON.stringify(obj, null, 2);
      } catch {
        outputStr = null;
      }
    }

    const lines = outputStr ? outputStr.split('\n') : [];
    return { output: outputStr, outputLines: lines };
  }, [result]);

  const hasOutput = output && output.trim().length > 0;

  // Use expandable output hook
  const { expanded, hiddenCount } = useExpandableOutput({
    totalItems: outputLines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
  });

  const renderLocations = () => {
    if (!formattedLocations) return null;
    return (
      <Box marginLeft={2} flexDirection="column">
        {formattedLocations.map((loc, i) => (
          <Text key={i}>{getColor('secondary')(`→ ${loc}`)}</Text>
        ))}
      </Box>
    );
  };

  const renderContent = () => {
    // Error display
    if (errorMessage) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={name} shimmer={!isFinished} />
          {renderLocations()}
          <Box marginLeft={2}>
            <Text>{getColor('error')(errorMessage)}</Text>
          </Box>
        </Box>
      );
    }

    // Static view or no output: show title + locations only
    if (isStatic || !hasOutput) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={name} shimmer={!isFinished} />
          {renderLocations()}
        </Box>
      );
    }

    // Expanded view: show all output
    if (expanded) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={name} shimmer={!isFinished} />
          {renderLocations()}
          <Box marginLeft={2} flexDirection="column">
            {outputLines.map((line, i) => (
              <Text key={i}>{getColor('secondary')(line)}</Text>
            ))}
          </Box>
        </Box>
      );
    }

    // Collapsed view: show preview + hint
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={name} shimmer={!isFinished} />
        {renderLocations()}
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
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
