import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { parseToolArg, extractResultText } from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { FileList } from './FileList.js';
import {
  parseLsEntries,
  getEntryName,
  resolveLsDisplayPath,
} from '../../../utils/ls-parse.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
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

  const rawDirPath = useMemo(() => parseToolArg(content, 'path'), [content]);

  // Parse the text result into entry lines, filtering out prefix metadata
  const entries = useMemo((): string[] => {
    const text = extractResultText(result);
    if (!text) return [];
    return parseLsEntries(text);
  }, [result]);

  // Resolve display path from entries when raw arg is relative (e.g. ".")
  const dirPath = useMemo(
    () => resolveLsDisplayPath(rawDirPath, entries),
    [rawDirPath, entries]
  );

  const title = getToolLabel('ls');

  const params = useMemo(() => formatToolParams(content, ['path']), [content]);

  const entryNames = useMemo(() => entries.map(getEntryName), [entries]);

  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: entries.length,
    previewCount: PREVIEW_ENTRIES,
    isStatic,
    unit: 'entries',
  });

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
          <ToolMeta params={params} />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    // No result yet
    if (!isFinished || entries.length === 0) {
      return (
        <Box flexDirection="column">
          <StatusInfo
            title={title}
            target={dirPath || undefined}
            shimmer={!isFinished}
          />
          <ToolMeta params={params} />
        </Box>
      );
    }

    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} />
        <ToolMeta params={params} />
        {secondaryInfo && <Text>{getColor('secondary')(secondaryInfo)}</Text>}
        <FileList
          items={entryNames}
          previewCount={PREVIEW_ENTRIES}
          expanded={expanded}
          expandHint={expandHint}
          hiddenCount={hiddenCount}
        />
      </Box>
    );
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
