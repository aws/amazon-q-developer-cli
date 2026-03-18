import React, { useMemo } from 'react';
import { Box, Text } from './../../../renderer.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import {
  parseToolArg,
  unwrapResultOutput,
} from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { FileList } from './FileList.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
const PREVIEW_FILES = 3;

/** Parsed glob output structure */
interface GlobOutput {
  filePaths: string[];
  totalFiles: number;
  truncated: boolean;
  message?: string;
}

export interface GlobProps {
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
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: GlobProps) {
  const { getColor } = useTheme();

  // Parse glob pattern from content (tool args)
  const globPattern = useMemo(
    () => parseToolArg(content, 'pattern'),
    [content]
  );

  // Parse glob output from result
  const globOutput = useMemo((): GlobOutput | null => {
    const { obj } = unwrapResultOutput(result);
    if (!obj) return null;

    return {
      filePaths: Array.isArray(obj.filePaths)
        ? (obj.filePaths as string[])
        : [],
      totalFiles: typeof obj.totalFiles === 'number' ? obj.totalFiles : 0,
      truncated: obj.truncated === true,
      message: typeof obj.message === 'string' ? obj.message : undefined,
    };
  }, [result]);

  const title = getToolLabel('glob');

  const params = useMemo(
    () => formatToolParams(content, ['pattern']),
    [content]
  );
  const filePaths = globOutput?.filePaths || [];

  const fileNames = useMemo(
    () => filePaths.map((p) => p.split('/').pop() || p),
    [filePaths]
  );

  // Use expandable output hook
  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: filePaths.length,
    previewCount: PREVIEW_FILES,
    isStatic,
    unit: 'files',
  });

  // Build secondary summary text (shown on second line)
  const getSecondarySummary = (): string | null => {
    if (!globOutput || !isFinished) return null;
    if (globOutput.message) return globOutput.message;
    if (globOutput.totalFiles === 0) return 'no matches';
    return `${globOutput.totalFiles} file${globOutput.totalFiles !== 1 ? 's' : ''}`;
  };

  const target = globPattern ? `"${globPattern}"` : undefined;

  const renderContent = () => {
    const secondarySummary = getSecondarySummary();

    // Error state
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <ToolMeta params={params} />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    // No result yet or still searching
    if (!globOutput) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <ToolMeta params={params} />
        </Box>
      );
    }

    // No files found or message
    if (globOutput.totalFiles === 0 || globOutput.message) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <ToolMeta params={params} />
          {secondarySummary && (
            <Text>{getColor('secondary')(secondarySummary)}</Text>
          )}
        </Box>
      );
    }

    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={!isFinished} />
        <ToolMeta params={params} />
        {secondarySummary && (
          <Text>{getColor('secondary')(secondarySummary)}</Text>
        )}
        <FileList
          items={fileNames}
          previewCount={PREVIEW_FILES}
          expanded={expanded}
          expandHint={expandHint}
          hiddenCount={hiddenCount}
        />
        {globOutput.truncated && expanded && (
          <Box marginLeft={2}>
            <Text>
              {getColor('warning')(
                `(showing ${filePaths.length} of ${globOutput.totalFiles} files)`
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
