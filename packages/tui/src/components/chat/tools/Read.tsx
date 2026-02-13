import React, { useMemo } from 'react';
import { Box } from 'ink';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_FILES = 5;

interface ReadOp {
  path: string;
  limit?: number;
  offset?: number;
}

export interface ReadProps {
  /** The tool name/action (e.g., "Reading", "Read") */
  name: string;

  /** File path or target description */
  target?: string;

  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the read operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /**
   * Raw JSON content from tool call (for parsing multiple file ops).
   * Expected format: { ops: [{ path, limit?, offset? }] }
   */
  content?: string;
}

/**
 * Read tool component for displaying file read operations.
 *
 * Features:
 * - Single file display with path
 * - Multiple file display with nested list
 * - Collapsible output with Ctrl+O expansion for large file lists
 * - Parses ops array from content JSON
 */
export const Read = React.memo(function Read({
  name,
  target,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
}: ReadProps) {
  const { getColor } = useTheme();

  // Parse operations from content if provided
  const ops = useMemo((): ReadOp[] => {
    if (!content) return [];
    try {
      const parsed = JSON.parse(content);
      if (parsed.ops && Array.isArray(parsed.ops)) {
        return parsed.ops.map(
          (op: { path?: string; limit?: number; offset?: number }): ReadOp => ({
            path: op.path || '',
            limit: op.limit,
            offset: op.offset,
          })
        );
      }
      return [];
    } catch {
      return [];
    }
  }, [content]);

  // Use expandable output hook
  const { expanded, expandHint } = useExpandableOutput({
    totalItems: ops.length,
    previewCount: PREVIEW_FILES,
    isStatic,
    unit: 'files',
  });

  const title = isFinished ? 'Read' : name;

  // Extract filename from path
  const getFileName = (path: string): string => {
    return path.split('/').pop() || path;
  };

  // If content was provided and parsed, use ops for display
  if (content && ops.length > 0) {
    if (ops.length === 1) {
      const op = ops[0];
      const displayContent = (
        <StatusInfo
          title={title}
          target={op?.path || 'file'}
          shimmer={!isFinished}
        />
      );
      if (noStatusBar) return displayContent;
      return <StatusBar status={status}>{displayContent}</StatusBar>;
    }

    // Multiple files - static view: just show count
    if (isStatic) {
      const displayContent = (
        <StatusInfo
          title={title}
          target={`${ops.length} files`}
          shimmer={!isFinished}
        />
      );
      if (noStatusBar) return displayContent;
      return <StatusBar status={status}>{displayContent}</StatusBar>;
    }

    // Multiple files - expanded view
    if (expanded) {
      const displayContent = (
        <Box flexDirection="column">
          <StatusInfo
            title={title}
            target={`${ops.length} files`}
            shimmer={!isFinished}
          />
          {ops.map((op, i) => (
            <Box key={i} marginLeft={2}>
              <Text>{getColor('secondary')(`→ ${getFileName(op.path)}`)}</Text>
            </Box>
          ))}
        </Box>
      );
      if (noStatusBar) return displayContent;
      return <StatusBar status={status}>{displayContent}</StatusBar>;
    }

    // Multiple files - collapsed view
    const displayContent = (
      <Box flexDirection="column">
        <StatusInfo
          title={title}
          target={`${ops.length} files`}
          shimmer={!isFinished}
        />
        {ops.slice(0, PREVIEW_FILES).map((op, i) => (
          <Box key={i} marginLeft={2}>
            <Text>{getColor('secondary')(`→ ${getFileName(op.path)}`)}</Text>
          </Box>
        ))}
        {expandHint && (
          <Box marginLeft={2}>
            <Text>{getColor('secondary')(expandHint)}</Text>
          </Box>
        )}
      </Box>
    );
    if (noStatusBar) return displayContent;
    return <StatusBar status={status}>{displayContent}</StatusBar>;
  }

  // Simple mode: use target prop directly
  const displayContent = (
    <StatusInfo title={title} target={target} shimmer={!isFinished} />
  );
  if (noStatusBar) return displayContent;
  return <StatusBar status={status}>{displayContent}</StatusBar>;
});
