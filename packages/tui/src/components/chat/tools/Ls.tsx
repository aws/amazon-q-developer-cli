import React, { useMemo } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { parseToolArg, extractResultText } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_ENTRIES = 5;

export interface LsProps {
  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the ls operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;

  /** Tool execution result */
  result?: ToolResult;
}

/**
 * Ls tool component for displaying directory listing results.
 *
 * Features:
 * - Shows directory path being listed
 * - Displays entry count summary when finished
 * - Collapsible output with Ctrl+O expansion
 */
export const Ls = React.memo(function Ls({
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: LsProps) {
  const { getColor } = useTheme();

  const dirPath = useMemo(() => parseToolArg(content, 'path'), [content]);

  // Parse the text result into lines, skipping the "User id:" prefix line
  const entries = useMemo((): string[] => {
    const text = extractResultText(result);
    if (!text) return [];
    return text
      .split('\n')
      .filter((line) => line.length > 0 && !line.startsWith('User id:'));
  }, [result]);

  const title = isFinished ? 'Listed' : 'Listing';

  const { expanded, expandHint } = useExpandableOutput({
    totalItems: entries.length,
    previewCount: PREVIEW_ENTRIES,
    isStatic,
    unit: 'entries',
  });

  // Extract just the filename from an ls -la style line
  const getEntryName = (line: string): string => {
    // ls long format: "drwxr-xr-x  user  group  size  date  name"
    // The name is the last whitespace-separated token
    const parts = line.trimEnd().split(/\s+/);
    return parts[parts.length - 1] || line;
  };

  const target = dirPath || undefined;

  const secondaryInfo =
    isFinished && entries.length > 0
      ? `${entries.length} ${entries.length === 1 ? 'entry' : 'entries'}`
      : null;

  const renderContent = () => {
    // Error state
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo
            title={title}
            target={dirPath || undefined}
            shimmer={!isFinished}
          />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    // No result yet
    if (!isFinished || entries.length === 0) {
      return (
        <StatusInfo
          title={title}
          target={dirPath || undefined}
          shimmer={!isFinished}
        />
      );
    }

    // Static view: just summary
    if (isStatic) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} />
          {secondaryInfo && <Text>{getColor('secondary')(secondaryInfo)}</Text>}
        </Box>
      );
    }

    // Expanded view
    if (expanded) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} />
          {secondaryInfo && <Text>{getColor('secondary')(secondaryInfo)}</Text>}
          {entries.map((entry, i) => (
            <Box key={i} marginLeft={2}>
              <Text>{getColor('primary')(`→ ${getEntryName(entry)}`)}</Text>
            </Box>
          ))}
        </Box>
      );
    }

    // Collapsed view
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} />
        {secondaryInfo && <Text>{getColor('secondary')(secondaryInfo)}</Text>}
        {entries.slice(0, PREVIEW_ENTRIES).map((entry, i) => (
          <Box key={i} marginLeft={2}>
            <Text>{getColor('primary')(`→ ${getEntryName(entry)}`)}</Text>
          </Box>
        ))}
        {expandHint && (
          <Box marginLeft={2}>
            <Text>{getColor('secondary')(expandHint)}</Text>
          </Box>
        )}
      </Box>
    );
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
