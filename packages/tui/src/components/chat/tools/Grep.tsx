import React, { useMemo } from 'react';
import { Box, Text } from 'ink';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { parseToolArg, unwrapResultOutput } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_FILES = 3;
const PREVIEW_MATCHES_PER_FILE = 3;

/** Grep result for a single file */
interface GrepFileResult {
  file: string;
  count: number;
  matches?: string[];
}

/** Parsed grep output structure */
interface GrepOutput {
  numMatches: number;
  numFiles: number;
  truncated: boolean;
  results?: GrepFileResult[];
  message?: string;
}

export interface GrepProps {
  /** The tool name/action (e.g., "Searching", "Searched") */
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
 * Grep tool component for displaying text search results.
 *
 * Features:
 * - Shows search pattern and match summary
 * - Displays file results with match counts
 * - Collapsible output with Ctrl+O expansion
 * - Shows matched lines with line numbers
 */
export const Grep = React.memo(function Grep({
  name,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: GrepProps) {
  const { getColor } = useTheme();

  // Parse search pattern from content (tool args)
  const searchPattern = useMemo(() => parseToolArg(content, 'pattern'), [content]);

  // Parse grep output from result
  const grepOutput = useMemo((): GrepOutput | null => {
    const { obj } = unwrapResultOutput(result);
    if (!obj) return null;

    return {
      numMatches: typeof obj.numMatches === 'number' ? obj.numMatches : 0,
      numFiles: typeof obj.numFiles === 'number' ? obj.numFiles : 0,
      truncated: obj.truncated === true,
      results: Array.isArray(obj.results) ? obj.results as GrepFileResult[] : undefined,
      message: typeof obj.message === 'string' ? obj.message : undefined,
    };
  }, [result]);

  const title = isFinished ? 'Grepped' : (name || 'Grepping');
  const results = grepOutput?.results || [];

  // Use expandable output hook
  const { expanded, hiddenCount } = useExpandableOutput({
    totalItems: results.length,
    previewCount: PREVIEW_FILES,
    isStatic,
  });

  // Extract filename from path
  const getFileName = (path: string): string => {
    return path.split('/').pop() || path;
  };

  // Build summary text
  const getSummary = (): string => {
    if (!grepOutput) {
      return searchPattern ? `"${searchPattern}"` : '';
    }
    if (grepOutput.message) {
      return grepOutput.message;
    }
    const pattern = searchPattern ? `"${searchPattern}"` : 'pattern';
    return `${pattern} → ${grepOutput.numMatches} match${grepOutput.numMatches !== 1 ? 'es' : ''} in ${grepOutput.numFiles} file${grepOutput.numFiles !== 1 ? 's' : ''}`;
  };

  const renderContent = () => {
    // Error state
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={searchPattern ? `"${searchPattern}"` : undefined} shimmer={!isFinished} />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    // No result yet or still searching
    if (!grepOutput) {
      return <StatusInfo title={title} target={searchPattern ? `"${searchPattern}"` : undefined} shimmer={!isFinished} />;
    }

    // No matches found
    if (grepOutput.numMatches === 0 || grepOutput.message) {
      return <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />;
    }

    // Static view: just show summary
    if (isStatic) {
      return <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />;
    }

    // Expanded view: show all results
    if (expanded) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />
          {results.map((fileResult, i) => (
            <Box key={i} flexDirection="column" marginLeft={2}>
              <Text>
                {getColor('secondary')(`→ ${getFileName(fileResult.file)}`)}
                {' '}
                {getColor('muted')(`(${fileResult.count})`)}
              </Text>
              {fileResult.matches?.map((match, j) => (
                <Box key={j} marginLeft={2}>
                  <Text>{getColor('muted')(match)}</Text>
                </Box>
              ))}
            </Box>
          ))}
          {grepOutput.truncated && (
            <Box marginLeft={2}>
              <Text>{getColor('warning')('(results truncated)')}</Text>
            </Box>
          )}
        </Box>
      );
    }

    // Collapsed view: show preview
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={getSummary()} shimmer={!isFinished} />
        {results.slice(0, PREVIEW_FILES).map((fileResult, i) => (
          <Box key={i} flexDirection="column" marginLeft={2}>
            <Text>
              {getColor('secondary')(`→ ${getFileName(fileResult.file)}`)}
              {' '}
              {getColor('muted')(`(${fileResult.count})`)}
            </Text>
            {fileResult.matches?.slice(0, PREVIEW_MATCHES_PER_FILE).map((match, j) => (
              <Box key={j} marginLeft={2}>
                <Text>{getColor('muted')(match)}</Text>
              </Box>
            ))}
            {(fileResult.matches?.length || 0) > PREVIEW_MATCHES_PER_FILE && (
              <Box marginLeft={2}>
                <Text>{getColor('muted')(`...+${(fileResult.matches?.length || 0) - PREVIEW_MATCHES_PER_FILE} more`)}</Text>
              </Box>
            )}
          </Box>
        ))}
        {(hiddenCount > 0 || grepOutput.truncated) && (
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
