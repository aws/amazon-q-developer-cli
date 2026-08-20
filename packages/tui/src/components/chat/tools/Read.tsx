import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { displayBasename } from '../../../utils/display-path.js';
import { ToolMeta } from './ToolMeta.js';
import { ToolOutput, ToolOutputHeader } from './ToolOutput.js';
import { FileList } from './FileList.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { expandTabs, normalizeLineEndings } from '../../../utils/string.js';
import { maxVisibleWidth, visibleWidth } from '../../../utils/text-width.js';
import {
  boundToolOutputLine,
  clipVisibleWidth,
  wrapAnsiLine,
} from '../../../lite/render.js';
import {
  extractResultBodyItems,
  extractResultBodyText,
  parseToolArg,
  splitBodyLines,
} from '../../../utils/tool-result.js';
import type { StatusType } from '../../../types/componentTypes.js';
import type { ToolResult } from '../../../stores/app-store.js';
import { getToolLabel, formatLineRange } from '../../../types/tool-status.js';
import { chalk } from '../../../utils/color.js';

const PREVIEW_FILES = 5;
const PREVIEW_READ_LINES = 20;
// Only surface the read body in-cohort; off-cohort keeps the mainline
// header-only render (path + line range, no content dump).
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

interface ReadOp {
  path: string;
  mode?: string;
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

  /** Tool result — carries the file section the read returned (in-cohort body). */
  result?: ToolResult;
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
// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'Read' has a complexity of 33. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
export const Read = React.memo(function Read({
  target,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: ReadProps) {
  const params = useMemo(
    // Exclude fields already reflected in the header/line-range or used for
    // parsing: operations/path/paths (targets) and offset/limit (shown as L#-#).
    () =>
      formatToolParams(content, [
        'operations',
        'path',
        'paths',
        'offset',
        'limit',
      ]),
    [content]
  );

  // Parse operations from content if provided
  const ops = useMemo((): ReadOp[] => {
    if (!content) return [];
    try {
      const parsed = JSON.parse(content);
      const rawOps = parsed.operations ?? parsed.ops;
      if (Array.isArray(rawOps)) {
        return rawOps.flatMap((op: Record<string, unknown>): ReadOp[] => {
          const mode = op.mode as string | undefined;
          if (mode === 'Directory') {
            return [{ path: (op.path as string) || '', mode }];
          }
          if (mode === 'Image') {
            const paths = (op.image_paths ?? op.paths) as string[] | undefined;
            return (paths || []).map((p) => ({ path: p, mode }));
          }
          // Line mode or legacy ops without mode
          return [
            {
              path: (op.path as string) || '',
              mode,
              limit: op.limit as number | undefined,
              offset: op.offset as number | undefined,
            },
          ];
        });
      }
      // Flat format: KAS read_file sends { path, offset?, limit? } directly
      if (typeof parsed.path === 'string') {
        return [
          {
            path: parsed.path,
            limit: parsed.limit as number | undefined,
            offset: parsed.offset as number | undefined,
          },
        ];
      }
      // Multi-file format: KAS read_files sends { paths: string[] }
      if (Array.isArray(parsed.paths)) {
        return parsed.paths.map((p: string) => ({ path: p }));
      }
      return [];
    } catch {
      return [];
    }
  }, [content]);

  const fileNames = useMemo(
    () => ops.map((op) => displayBasename(op.path)),
    [ops]
  );
  const outputVisible = useToolOutputVisible();
  const expandableOutputVisible = !PORT_ACTIVE() || outputVisible;

  const {
    expanded,
    expandHint,
    hiddenCount,
    effectivePreviewCount,
    outputMaxChars,
  } = useExpandableOutput({
    totalItems: expandableOutputVisible ? ops.length : 0,
    previewCount: PREVIEW_FILES,
    maxContentWidth: expandableOutputVisible ? maxVisibleWidth(fileNames) : 0,
    isStatic,
    unit: 'files',
    applyVerbosityOutputCap: true,
  });

  const title = getToolLabel('read');

  const renderMeta = () => <ToolMeta params={params} />;

  // Code-intelligence and directory reads return plain output, not numbered source.
  const showBody = PORT_ACTIVE() && outputVisible && isFinished;
  const bodyItems = useMemo(() => extractResultBodyItems(result), [result]);
  const mixedRead =
    ops.some((op) => op.mode === 'Directory') &&
    ops.some((op) => op.mode !== 'Directory') &&
    bodyItems.length === ops.length;
  const plainOutputBody =
    parseToolArg(content, 'operation') ||
    (ops.length > 0 && ops.every((op) => op.mode === 'Directory')) ? (
      <ToolOutput
        lines={splitBodyLines(extractResultBodyText(result))}
        isStatic={isStatic}
        emptyPlaceholder
      />
    ) : null;
  const mixedOutputBody = mixedRead
    ? ops.map((op, index) =>
        op.mode === 'Directory' ? (
          <ToolOutput
            key={`${op.path}-${index}`}
            lines={splitBodyLines(bodyItems[index] ?? null)}
            isStatic={isStatic}
            emptyPlaceholder
          />
        ) : (
          <ReadBody
            key={`${op.path}-${index}`}
            body={bodyItems[index]}
            path={op.path}
            startLine={(op.offset ?? 0) + 1}
            isStatic={isStatic}
          />
        )
      )
    : null;

  // If content was provided and parsed, use ops for display
  if (content && ops.length > 0) {
    if (ops.length === 1) {
      const op = ops[0];
      const lineRange = formatLineRange(op!);
      const displayContent = (
        <Box flexDirection="column">
          <StatusInfo
            title={title}
            target={(op?.path || 'file') + lineRange}
            shimmer={!isFinished}
          />
          {renderMeta()}
          {showBody &&
            (plainOutputBody ?? (
              <ReadBody
                result={result}
                path={op?.path}
                startLine={(op?.offset ?? 0) + 1}
                isStatic={isStatic}
              />
            ))}
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
        {outputVisible && (
          <FileList
            items={fileNames}
            previewCount={effectivePreviewCount}
            expanded={expanded}
            expandHint={expandHint}
            hiddenCount={hiddenCount}
            maxChars={outputMaxChars}
          />
        )}
        {showBody &&
          (mixedOutputBody ?? plainOutputBody ?? (
            <ReadBody result={result} isStatic={isStatic} />
          ))}
      </Box>
    );
    if (noStatusBar) return displayContent;
    return <StatusBar status={status}>{displayContent}</StatusBar>;
  }

  const displayContent = (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      {renderMeta()}
      {showBody && plainOutputBody}
    </Box>
  );
  if (noStatusBar) return displayContent;
  return <StatusBar status={status}>{displayContent}</StatusBar>;
});

const LINE_NUM_WIDTH = 4;
const READ_BODY_INDENT = 8;
const LINE_NUM_GAP = 2;

/** Renders the bounded, expandable body of an in-cohort file read. */
const ReadBody = React.memo(function ReadBody({
  result,
  body,
  path,
  startLine = 1,
  isStatic,
}: {
  result?: ToolResult;
  body?: string;
  path?: string;
  startLine?: number;
  isStatic: boolean;
}) {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const highlightCode = useSyntaxHighlight();

  const text = useMemo(
    () => body ?? extractResultBodyText(result),
    [body, result]
  );
  const lines = useMemo(
    () =>
      text
        ? normalizeLineEndings(text)
            .replace(/\n+$/, '')
            .split('\n')
            .map(expandTabs)
        : [],
    [text]
  );
  const language = path?.split('.').pop()?.toLowerCase();
  const visualRows = useMemo(() => {
    const codeWidth = Math.max(
      1,
      termWidth - READ_BODY_INDENT - LINE_NUM_WIDTH - LINE_NUM_GAP
    );
    const rows: Array<{
      text: string;
      lineNumber: number | null;
    }> = [];
    let maxWidth = 0;

    lines.forEach((line, index) => {
      const bounded = boundToolOutputLine(line, 'end');
      if (bounded.droppedChars > 0) {
        rows.push({
          text: chalk.dim(
            `... (line clipped; +${bounded.droppedChars} chars before)`
          ),
          lineNumber: null,
        });
      }
      const highlighted = highlightCode(bounded.text, language);
      const lineWidth = visibleWidth(highlighted);
      const wrapped =
        lineWidth <= codeWidth
          ? [highlighted]
          : wrapAnsiLine(highlighted, codeWidth, codeWidth);
      wrapped.forEach((row, rowIndex) => {
        rows.push({
          text: row,
          lineNumber: rowIndex === 0 ? startLine + index : null,
        });
        maxWidth = Math.max(maxWidth, visibleWidth(row));
      });
    });

    return { rows, maxWidth };
  }, [highlightCode, language, lines, startLine, termWidth]);

  const {
    expanded,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  } = useExpandableOutput({
    totalItems: visualRows.rows.length,
    previewCount: PREVIEW_READ_LINES,
    maxContentWidth: visualRows.maxWidth,
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: true,
  });

  if (isStatic && !persistOutput) return null;

  if (result && result.status === 'success' && lines.length === 0) {
    return (
      <>
        <ToolOutputHeader />
        <Box marginLeft={4}>
          <Text>{getColor('muted')('(no output)')}</Text>
        </Box>
      </>
    );
  }
  if (lines.length === 0) return null;

  const shown = expanded
    ? visualRows.rows
    : visualRows.rows.slice(-effectivePreviewCount);
  const hidden = visualRows.rows.length - shown.length;
  // Clip the already-highlighted row without severing ANSI escapes.
  const clip = (s: string) =>
    outputMaxChars != null && outputMaxChars > 0
      ? clipVisibleWidth(s, outputMaxChars)
      : s;
  const truncMarker =
    hidden > 0
      ? isStatic
        ? `...+${hidden} lines above`
        : `...+${hidden} lines above (ctrl+o to toggle)`
      : expandHint || undefined;

  return (
    <>
      <ToolOutputHeader />
      <Box marginLeft={4} flexDirection="column">
        {truncMarker && <Text>{getColor('secondary')(truncMarker)}</Text>}
        {shown.map((row, i) => (
          <Text key={i}>
            {getColor('secondary')(
              row.lineNumber == null
                ? ' '.repeat(LINE_NUM_WIDTH)
                : String(row.lineNumber).padStart(LINE_NUM_WIDTH)
            )}
            {`  ${clip(row.text)}`}
          </Text>
        ))}
      </Box>
    </>
  );
});
