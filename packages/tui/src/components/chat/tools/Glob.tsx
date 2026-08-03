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
import { ToolOutput } from './ToolOutput.js';
import { FileList } from './FileList.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
const PREVIEW_FILES = 3;
// `╰ output:` tree is the in-cohort output-differentiation feature.
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

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

  // Parse glob pattern from content (tool args).
  // KAS sends `query` instead of `pattern` for file_search.
  const globPattern = useMemo(
    () => parseToolArg(content, 'pattern') ?? parseToolArg(content, 'query'),
    [content]
  );

  // Parse glob output from result
  const globOutput = useMemo((): GlobOutput | null => {
    const { obj, text } = unwrapResultOutput(result);

    // KAS sends results as plain text in `message`: "You searched for X and
    // received...\n---\nfile1\nfile2\n---\n[trailing message]". Structured
    // output (filePaths/totalFiles) is the V2/legacy shape. Only text-parse
    // when there is NO structured signal — otherwise the V2 no-files `message`
    // (e.g. "No files found matching pattern: …") gets hijacked as a fake file.
    const hasStructured =
      !!obj &&
      (Array.isArray(obj.filePaths) || typeof obj.totalFiles === 'number');
    const rawText = hasStructured
      ? null
      : (text ?? (obj && typeof obj.message === 'string' ? obj.message : null));
    if (rawText) {
      const parts = rawText.split(/^---$/m);
      if (parts.length >= 2) {
        // Successfully parsed the delimited format.
        const fileBlock = parts[1]?.trim() ?? '';
        const filePaths = fileBlock
          ? fileBlock.split('\n').filter((l) => l.trim())
          : [];
        const trailing = parts[2]?.trim();
        // Only the header (segment before the first ---) signals truncation;
        // scanning the whole text false-positives on a file path with
        // "incomplete" in it.
        const truncated = (parts[0] ?? '').includes('incomplete');
        return {
          filePaths,
          totalFiles: filePaths.length,
          truncated,
          message: trailing || undefined,
        };
      }
      // No --- delimiters: strip a leading "You searched for…" header line so
      // the envelope doesn't leak, then either report no matches or treat the
      // remaining lines as file paths (so the expandable preview still works).
      const body = rawText.replace(/^You searched for[^\n]*\n?/, '');
      if (/no (matches|results)|not found/i.test(body)) {
        return { filePaths: [], totalFiles: 0, truncated: false };
      }
      const lines = body.split('\n').filter((l) => l.trim());
      return {
        filePaths: lines,
        totalFiles: lines.length,
        truncated: false,
        message: undefined,
      };
    }

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
    () => formatToolParams(content, ['pattern', 'query', 'explanation']),
    [content]
  );
  const filePaths = globOutput?.filePaths || [];

  const fileNames = useMemo(
    () => filePaths.map((p) => p.split('/').pop() || p),
    [filePaths]
  );
  const outputVisible = useToolOutputVisible();

  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: !PORT_ACTIVE() || outputVisible ? filePaths.length : 0,
    previewCount: PREVIEW_FILES,
    isStatic,
    unit: 'files',
    applyVerbosityOutputCap: true,
  });

  // Build secondary summary text (shown on second line)
  const getSecondarySummary = (): string | null => {
    if (!globOutput || !isFinished) return null;
    // No files: show the backend message (e.g. "no matches") if any.
    if (globOutput.totalFiles === 0) return globOutput.message || 'no matches';
    // Files present: show the count, noting truncation so the user knows to
    // refine. The raw "Refine your search…" trailer is folded into this.
    const count = `${globOutput.totalFiles} file${globOutput.totalFiles !== 1 ? 's' : ''}`;
    return globOutput.truncated ? `${count} (showing first results)` : count;
  };

  const target = globPattern ? `"${globPattern}"` : undefined;

  const head = (
    <>
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
    </>
  );

  const bodyRows = useMemo(() => {
    if (!globOutput || globOutput.totalFiles === 0) return [];
    const rows: string[] = [];
    const summary = getSecondarySummary();
    if (summary) rows.push(summary);
    rows.push(...fileNames);
    if (globOutput.truncated) {
      rows.push(
        `(showing ${filePaths.length} of ${globOutput.totalFiles} files)`
      );
    }
    return rows;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [globOutput, fileNames, filePaths.length, isFinished]);

  const renderContent = () => {
    if (PORT_ACTIVE()) {
      let lines: string[] | null = null;
      let isError = false;
      if (result?.status === 'error') {
        lines = result.error.split('\n');
        isError = true;
      } else if (globOutput && outputVisible) {
        if (globOutput.totalFiles === 0) {
          const summary = getSecondarySummary();
          lines = summary ? [summary] : [];
        } else {
          lines = bodyRows;
        }
      }
      return (
        <Box flexDirection="column">
          {head}
          {lines && (
            <ToolOutput lines={lines} isError={isError} isStatic={isStatic} />
          )}
        </Box>
      );
    }

    const secondarySummary = getSecondarySummary();

    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          {head}
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }
    if (!globOutput) {
      return <Box flexDirection="column">{head}</Box>;
    }
    if (globOutput.totalFiles === 0) {
      return (
        <Box flexDirection="column">
          {head}
          {secondarySummary && (
            <Text>{getColor('secondary')(secondarySummary)}</Text>
          )}
        </Box>
      );
    }
    return (
      <Box flexDirection="column">
        {head}
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
