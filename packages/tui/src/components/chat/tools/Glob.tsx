import React, { useMemo } from 'react';
import { Box, Text } from 'ink';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_FILES = 5;

/** Parsed glob output structure */
interface GlobOutput {
  filePaths: string[];
  totalFiles: number;
  truncated: boolean;
  message?: string;
}

export interface GlobProps {
  /** The tool name/action (e.g., "Finding", "Found") */
  name?: string;

  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the search has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;

  /** Tool execution result */
  result?: ToolResult;
}

/**
 * Glob tool component for displaying file pattern matching results.
 *
 * Features:
 * - Shows glob pattern and file count summary
 * - Displays matched file paths
 * - Collapsible output with Ctrl+O expansion
 */
export const Glob = React.memo(function Glob({
  name,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: GlobProps) {
  const { getColor } = useTheme();

  // Parse glob pattern from content (tool args)
  const globPattern = useMemo(() => {
    if (!content) return null;
    try {
      const parsed = JSON.parse(content);
      return parsed.pattern || null;
    } catch {
      return null;
    }
  }, [content]);

  // Parse glob output from result
  const globOutput = useMemo((): GlobOutput | null => {
    if (!result || result.status !== 'success') return null;

    const rawOutput = result.output;
    if (!rawOutput || typeof rawOutput !== 'object') return null;

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

    return {
      filePaths: Array.isArray(obj.filePaths) ? obj.filePaths as string[] : [],
      totalFiles: typeof obj.totalFiles === 'number' ? obj.totalFiles : 0,
      truncated: obj.truncated === true,
      message: typeof obj.message === 'string' ? obj.message : undefined,
    };
  }, [result]);

  const title = isFinished ? 'Globbed' : (name || 'Globbing');
  const filePaths = globOutput?.filePaths || [];

  // Use expandable output hook
  const { expanded, hiddenCount } = useExpandableOutput({
    totalItems: filePaths.length,
    previewCount: PREVIEW_FILES,
    isStatic,
  });

  // Extract filename from path
  const getFileName = (path: string): string => {
    return path.split('/').pop() || path;
  };

  // Build summary text
  const getSummary = (): string => {
    if (!globOutput) {
      return globPattern ? `"${globPattern}"` : '';
    }
    if (globOutput.message) {
      return globOutput.message;
    }
    const pattern = globPattern ? `"${globPattern}"` : 'pattern';
    return `${globOutput.totalFiles} file${globOutput.totalFiles !== 1 ? 's' : ''} matching ${pattern}`;
  };

  const renderContent = () => {
    // Error state
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={globPattern ? `"${globPattern}"` : undefined} shimmer={!isFinished} />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    // No result yet or still searching
    if (!globOutput) {
      return <StatusInfo title={title} target={globPattern ? `"${globPattern}"` : undefined} shimmer={!isFinished} />;
    }

    // No files found
    if (globOutput.totalFiles === 0 || globOutput.message) {
      return <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />;
    }

    // Static view: just show summary
    if (isStatic) {
      return <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />;
    }

    // Expanded view: show all files
    if (expanded) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />
          {filePaths.map((filePath, i) => (
            <Box key={i} marginLeft={2}>
              <Text>{getColor('secondary')(`→ ${getFileName(filePath)}`)}</Text>
            </Box>
          ))}
          {globOutput.truncated && (
            <Box marginLeft={2}>
              <Text>{getColor('warning')(`(showing ${filePaths.length} of ${globOutput.totalFiles} files)`)}</Text>
            </Box>
          )}
        </Box>
      );
    }

    // Collapsed view: show preview
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />
        {filePaths.slice(0, PREVIEW_FILES).map((filePath, i) => (
          <Box key={i} marginLeft={2}>
            <Text>{getColor('secondary')(`→ ${getFileName(filePath)}`)}</Text>
          </Box>
        ))}
        {(hiddenCount > 0 || globOutput.truncated) && (
          <Box marginLeft={2}>
            <Text>
              {getColor('secondary')(
                hiddenCount > 0
                  ? `...+${hiddenCount} files (^O to expand)`
                  : '(^O to expand)'
              )}
            </Text>
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
