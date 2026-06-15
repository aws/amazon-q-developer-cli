import React, { useMemo } from 'react';
import { Box, Text } from './../../../renderer.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import {
  parseToolArg,
  unwrapResultOutput,
} from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { expandTabs } from '../../../utils/string.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
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
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: GrepProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  // Parse search pattern from content (tool args).
  // KAS sends `query` instead of `pattern` for grep_search.
  const searchPattern = useMemo(
    () => parseToolArg(content, 'pattern') ?? parseToolArg(content, 'query'),
    [content]
  );

  // Parse grep output from result
  const grepOutput = useMemo((): GrepOutput | null => {
    const { obj, text } = unwrapResultOutput(result);

    // KAS sends results as plain text in `message`. Format (from kiro-agent
    // grep-search): a header line followed by per-file blocks:
    //   You searched for <q> and received the following results:
    //   <filepath>
    //   <lineNo>:<matched line>      (":" = match, "-" = context)
    //   <lineNo>-<context line>
    //   <filepath>
    //   ...
    // Only text-parse when there is NO structured signal — otherwise the V2
    // no-results shape (numMatches/numFiles/results) with an optional `message`
    // would be hijacked and misparsed as a fake file.
    const hasStructured =
      !!obj &&
      (typeof obj.numMatches === 'number' ||
        typeof obj.numFiles === 'number' ||
        Array.isArray(obj.results));
    const rawText = hasStructured
      ? null
      : (text ?? (obj && typeof obj.message === 'string' ? obj.message : null));
    if (rawText) {
      // Drop the "You searched for … results:" header line if present. Strip
      // only the first line (colon-safe): a query containing ":" (e.g. "TODO:")
      // must not truncate the header mid-line and leak a fragment as a file.
      const body = rawText.replace(/^You searched for[^\n]*\n?/, '');
      if (/no matches found/i.test(body)) {
        return { numMatches: 0, numFiles: 0, truncated: false };
      }
      const lineRe = /^(\d+)([:-])(.*)$/;
      const results: GrepFileResult[] = [];
      let current: GrepFileResult | null = null;
      let totalMatches = 0;
      let truncated = /\.\.\.\s*\+?\d+\s*more/i.test(rawText);
      for (const line of body.split('\n')) {
        if (!line.trim()) continue;
        if (/\.\.\.\s*\+?\d+\s*more/i.test(line)) {
          truncated = true;
          continue;
        }
        const m = line.match(lineRe);
        if (m && current) {
          current.matches!.push(expandTabs(line));
          if (m[2] === ':') {
            current.count += 1;
            totalMatches += 1;
          }
        } else if (!m) {
          // A non-numbered line is a file path header.
          current = { file: line.trim(), count: 0, matches: [] };
          results.push(current);
        }
      }
      return {
        numMatches: totalMatches,
        numFiles: results.length,
        truncated,
        results: results.length > 0 ? results : undefined,
        message: undefined,
      };
    }

    if (!obj) return null;

    return {
      numMatches: typeof obj.numMatches === 'number' ? obj.numMatches : 0,
      numFiles: typeof obj.numFiles === 'number' ? obj.numFiles : 0,
      truncated: obj.truncated === true,
      results: Array.isArray(obj.results)
        ? (obj.results as GrepFileResult[]).map((r) => ({
            ...r,
            matches: r.matches?.map(expandTabs),
          }))
        : undefined,
      message: typeof obj.message === 'string' ? obj.message : undefined,
    };
  }, [result]);

  const title = getToolLabel('grep');

  const params = useMemo(
    () => formatToolParams(content, ['pattern', 'query', 'explanation']),
    [content]
  );
  const results = grepOutput?.results || [];

  // Expandability must account for BOTH dimensions: more files than the
  // preview shows, AND per-file matches beyond PREVIEW_MATCHES_PER_FILE.
  // Keying only on file count missed the common case of many matches in a
  // few files (ctrl+o never registered). Count total vs. shown match lines.
  const totalMatchLines = results.reduce(
    (sum, f) => sum + (f.matches?.length ?? 0),
    0
  );
  const shownMatchLines = results
    .slice(0, PREVIEW_FILES)
    .reduce(
      (sum, f) =>
        sum + Math.min(f.matches?.length ?? 0, PREVIEW_MATCHES_PER_FILE),
      0
    );

  // Use expandable output hook
  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: totalMatchLines,
    previewCount: shownMatchLines,
    isStatic,
    unit: 'matches',
  });

  // Extract filename from path
  const getFileName = (path: string): string => {
    return path.split('/').pop() || path;
  };

  // Build secondary summary text (shown on second line)
  const getSecondarySummary = (): string | null => {
    if (!grepOutput || !isFinished) return null;
    if (grepOutput.numMatches === 0) return grepOutput.message || 'no matches';
    const count = `${grepOutput.numMatches} match${grepOutput.numMatches !== 1 ? 'es' : ''} in ${grepOutput.numFiles} file${grepOutput.numFiles !== 1 ? 's' : ''}`;
    return grepOutput.truncated ? `${count} (showing first results)` : count;
  };

  const target = searchPattern ? `"${searchPattern}"` : undefined;

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
    if (!grepOutput) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <ToolMeta params={params} />
        </Box>
      );
    }

    // No matches found — show only the summary.
    if (grepOutput.numMatches === 0) {
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

    // Static view: just show summary
    if (isStatic && !expanded) {
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

    // Expanded view: show all results
    if (expanded) {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <ToolMeta params={params} />
          {secondarySummary && (
            <Text>{getColor('secondary')(secondarySummary)}</Text>
          )}
          {results.map((fileResult, i) => (
            <Box key={i} flexDirection="column" marginLeft={2}>
              <Text>
                {getColor('primary')(
                  `${glyphs.arrow} ${getFileName(fileResult.file)}`
                )}{' '}
                {getColor('secondary')(`(${fileResult.count})`)}
              </Text>
              {fileResult.matches?.map((match, j) => (
                <Box key={j} marginLeft={2}>
                  <Text>{getColor('secondary')(match)}</Text>
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
        <StatusInfo title={title} target={target} shimmer={!isFinished} />
        <ToolMeta params={params} />
        {secondarySummary && (
          <Text>{getColor('secondary')(secondarySummary)}</Text>
        )}
        {results.slice(0, PREVIEW_FILES).map((fileResult, i) => (
          <Box key={i} flexDirection="column" marginLeft={2}>
            <Text>
              {getColor('primary')(
                `${glyphs.arrow} ${getFileName(fileResult.file)}`
              )}{' '}
              {getColor('secondary')(`(${fileResult.count})`)}
            </Text>
            {fileResult.matches
              ?.slice(0, PREVIEW_MATCHES_PER_FILE)
              .map((match, j) => (
                <Box key={j} marginLeft={2}>
                  <Text>{getColor('secondary')(match)}</Text>
                </Box>
              ))}
            {(fileResult.matches?.length || 0) > PREVIEW_MATCHES_PER_FILE && (
              <Box marginLeft={2}>
                <Text>
                  {getColor('secondary')(
                    `...+${(fileResult.matches?.length || 0) - PREVIEW_MATCHES_PER_FILE} more`
                  )}
                </Text>
              </Box>
            )}
          </Box>
        ))}
        {(hiddenCount > 0 || grepOutput.truncated) && (
          <Box marginLeft={2}>
            <Text>
              {getColor('secondary')(expandHint || '(ctrl+o to toggle)')}
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
