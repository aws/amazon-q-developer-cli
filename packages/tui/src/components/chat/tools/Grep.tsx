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
import { sanitizeUntrustedText } from '../../../utils/sanitize-terminal-content.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { displayBasename } from '../../../utils/display-path.js';
import { ToolMeta } from './ToolMeta.js';
import { ToolOutput } from './ToolOutput.js';
import { expandTabs } from '../../../utils/string.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';
const PREVIEW_FILES = 3;
const PREVIEW_MATCHES_PER_FILE = 3;
// The `╰ output:` tree is the in-cohort output-differentiation feature;
// off-cohort renders the mainline layout (results with no header).
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

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
  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 32 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line sonarjs/cognitive-complexity
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
      : (text ??
        (obj && typeof obj.message === 'string'
          ? sanitizeUntrustedText(obj.message)
          : null));
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
            file:
              typeof r.file === 'string'
                ? sanitizeUntrustedText(r.file)
                : r.file,
            matches: r.matches?.map((mm) =>
              expandTabs(sanitizeUntrustedText(mm))
            ),
          }))
        : undefined,
      message:
        typeof obj.message === 'string'
          ? sanitizeUntrustedText(obj.message)
          : undefined,
    };
  }, [result]);

  const title = getToolLabel('grep');

  const params = useMemo(
    () => formatToolParams(content, ['pattern', 'query', 'explanation']),
    [content]
  );
  const results = grepOutput?.results || [];

  // Count match rows so one file with many matches still registers Ctrl+O.
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
  const outputVisible = useToolOutputVisible();

  const { expanded, expandHint, hiddenCount } = useExpandableOutput({
    totalItems: !PORT_ACTIVE() || outputVisible ? totalMatchLines : 0,
    previewCount: shownMatchLines,
    isStatic,
    unit: 'matches',
    applyVerbosityOutputCap: true,
  });

  // Build secondary summary text (shown on second line)
  const getSecondarySummary = (): string | null => {
    if (!grepOutput || !isFinished) return null;
    if (grepOutput.numMatches === 0) return grepOutput.message || 'no matches';
    const count = `${grepOutput.numMatches} match${grepOutput.numMatches !== 1 ? 'es' : ''} in ${grepOutput.numFiles} file${grepOutput.numFiles !== 1 ? 's' : ''}`;
    return grepOutput.truncated ? `${count} (showing first results)` : count;
  };

  const target = searchPattern ? `"${searchPattern}"` : undefined;

  const head = (
    <>
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
    </>
  );

  const bodyRows = useMemo(() => {
    if (!grepOutput || grepOutput.numMatches === 0) return [];
    const rows: string[] = [];
    const summary = getSecondarySummary();
    if (summary) rows.push(summary);
    for (const f of results) {
      rows.push(`${glyphs.arrow} ${displayBasename(f.file)} (${f.count})`);
      for (const m of f.matches ?? []) rows.push(`  ${m}`);
    }
    if (grepOutput.truncated) rows.push('(results truncated)');
    return rows;
    // LINT-DEBT(react-hooks/exhaustive-deps): pre-existing suppression accepted at gate adoption; listed values cover state read through the render-local summary helper
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [grepOutput, results, isFinished, glyphs.arrow]);

  const renderContent = () => {
    if (PORT_ACTIVE()) {
      let lines: string[] | null = null;
      let isError = false;
      if (result?.status === 'error') {
        lines = result.error.split('\n');
        isError = true;
      } else if (grepOutput && outputVisible) {
        if (grepOutput.numMatches === 0) {
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
    if (!grepOutput) {
      return <Box flexDirection="column">{head}</Box>;
    }
    if (grepOutput.numMatches === 0) {
      return (
        <Box flexDirection="column">
          {head}
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
          {head}
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
          {head}
          {secondarySummary && (
            <Text>{getColor('secondary')(secondarySummary)}</Text>
          )}
          {results.map((fileResult, i) => (
            <Box key={i} flexDirection="column" marginLeft={2}>
              <Text>
                {getColor('primary')(
                  `${glyphs.arrow} ${displayBasename(fileResult.file)}`
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
        {head}
        {secondarySummary && (
          <Text>{getColor('secondary')(secondarySummary)}</Text>
        )}
        {results.slice(0, PREVIEW_FILES).map((fileResult, i) => (
          <Box key={i} flexDirection="column" marginLeft={2}>
            <Text>
              {getColor('primary')(
                `${glyphs.arrow} ${displayBasename(fileResult.file)}`
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
