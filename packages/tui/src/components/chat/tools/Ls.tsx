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
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import {
  parseLsEntries,
  getEntryName,
  resolveLsDisplayPath,
} from '../../../utils/ls-parse.js';
import { maxVisibleWidth } from '../../../utils/text-width.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
const PREVIEW_ENTRIES = 5;
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

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
  const outputVisible = useToolOutputVisible();
  const expandableOutputVisible = !PORT_ACTIVE() || outputVisible;

  const {
    expanded,
    expandHint,
    hiddenCount,
    effectivePreviewCount,
    outputMaxChars,
  } = useExpandableOutput({
    totalItems: expandableOutputVisible ? entries.length : 0,
    previewCount: PREVIEW_ENTRIES,
    maxContentWidth: expandableOutputVisible ? maxVisibleWidth(entryNames) : 0,
    isStatic,
    unit: 'entries',
    applyVerbosityOutputCap: true,
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

    if (!outputVisible) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} />
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
          previewCount={effectivePreviewCount}
          expanded={expanded}
          expandHint={expandHint}
          hiddenCount={hiddenCount}
          maxChars={outputMaxChars}
        />
      </Box>
    );
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
