import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { FileList } from './FileList.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';

const PREVIEW_FILES = 5;

interface ReadOp {
  path: string;
  limit?: number;
  offset?: number;
}

export interface ReadProps {
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
 * - Shows intent and params
 * - Collapsible output with Ctrl+O expansion for large file lists
 * - Parses ops array from content JSON
 */
export const Read = React.memo(function Read({
  target,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
}: ReadProps) {
  const params = useMemo(
    () => formatToolParams(content, ['ops', 'path']),
    [content]
  );

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

  const fileNames = useMemo(
    () => ops.map((op) => op.path.split('/').pop() || op.path),
    [ops]
  );

  // Use expandable output hook
  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: ops.length,
    previewCount: PREVIEW_FILES,
    isStatic,
    unit: 'files',
  });

  const title = getToolLabel('read');

  const renderMeta = () => <ToolMeta params={params} />;

  // If content was provided and parsed, use ops for display
  if (content && ops.length > 0) {
    if (ops.length === 1) {
      const op = ops[0];
      const displayContent = (
        <Box flexDirection="column">
          <StatusInfo
            title={title}
            target={op?.path || 'file'}
            shimmer={!isFinished}
          />
          {renderMeta()}
        </Box>
      );
      if (noStatusBar) return displayContent;
      return <StatusBar status={status}>{displayContent}</StatusBar>;
    }

    // Multiple files
    const displayContent = (
      <Box flexDirection="column">
        <StatusInfo
          title={title}
          target={`(${ops.length} files)`}
          shimmer={!isFinished}
        />
        {renderMeta()}
        <FileList
          items={fileNames}
          previewCount={PREVIEW_FILES}
          expanded={expanded}
          expandHint={expandHint}
          hiddenCount={hiddenCount}
        />
      </Box>
    );
    if (noStatusBar) return displayContent;
    return <StatusBar status={status}>{displayContent}</StatusBar>;
  }

  // Simple mode: use target prop directly
  const displayContent = (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      {renderMeta()}
    </Box>
  );
  if (noStatusBar) return displayContent;
  return <StatusBar status={status}>{displayContent}</StatusBar>;
});
