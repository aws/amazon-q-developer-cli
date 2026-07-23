import React, { useRef, useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { expandTabs, normalizeLineEndings } from '../../../utils/string.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { diffLines, type Change } from 'diff';
import { getToolLabel } from '../../../types/tool-status.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';

export interface WriteProps {
  /** Old text content for diff (empty string for new files). When undefined,
   *  Write falls back to parsing `oldStr` out of the JSON `content` blob. */
  oldText?: string;

  /** New text content for diff. When undefined, Write falls back to parsing
   *  `newStr`/`content` out of the JSON `content` blob. */
  newText?: string;

  /** File path for syntax highlighting and display */
  filePath?: string;

  /** Line offset for status bar coloring */
  lineOffset?: number;

  /** 1-based start line in the original file (for strReplace diffs) */
  startLine?: number;

  /** Whether the write operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /**
   * Raw JSON content from tool call (for parsing write operations).
   * Expected format: { command, path, content?, oldStr?, newStr?, insertLine? }
   */
  content?: string;
}

/**
 * Write tool component for displaying file write operations with diff view.
 *
 * Features:
 * - Syntax-highlighted diff display
 * - Parses tool call content for command type detection
 * - Supports create, strReplace, and insert operations
 */
export const Write = React.memo<WriteProps>(function Write({
  oldText,
  newText,
  filePath,
  startLine,
  isFinished = false,
  isStatic = false,
  content,
}) {
  const { getColor, colors: themeColors } = useTheme();
  const highlightCode = useSyntaxHighlight();
  // showWriteDiffs off → keep the header + added/removed summary (the write is
  // still evidenced) but drop the diff body. Mirrors lite's suppressDiff.
  // Off-cohort mainline ALWAYS shows the diff, so force it on there (a leaked
  // verbosity `showWriteDiffs:false` must not suppress the body off-cohort).
  const { showWriteDiffs: showWriteDiffsCfg } = useVerboseDisplay();
  const showWriteDiffs =
    process.env.KIRO_LITE_ROLLOUT_ENABLED === '1' ? showWriteDiffsCfg : true;

  // When foreground is set on diff colors, skip syntax highlighting and use flat color
  const addedFg = themeColors.diff.added.foreground
    ? getColor('diff.added.foreground')
    : undefined;
  const removedFg = themeColors.diff.removed.foreground
    ? getColor('diff.removed.foreground')
    : undefined;
  const contentRef = useRef<any>(null);

  // Parse content if provided to extract operation details
  const parsedContent = useMemo(() => {
    if (!content) return null;
    try {
      return JSON.parse(content);
    } catch {
      return null;
    }
  }, [content]);

  // Explicit diff props win; only in-cohort accepts snake_case wire fields.
  const portActive = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
  const displayPath = filePath ?? parsedContent?.path;
  const recoveredOld = portActive
    ? (parsedContent?.oldStr ?? parsedContent?.old_str)
    : parsedContent?.oldStr;
  const cmd = parsedContent?.command;
  const fileText = parsedContent?.file_text ?? parsedContent?.content;
  const newStr = parsedContent?.newStr ?? parsedContent?.new_str;
  const recoveredNew = portActive
    ? cmd === 'create' || cmd === 'append'
      ? (fileText ?? newStr)
      : (newStr ?? fileText)
    : (parsedContent?.newStr ?? parsedContent?.content);
  const displayOldText = oldText !== undefined ? oldText : (recoveredOld ?? '');
  const displayNewText = newText !== undefined ? newText : (recoveredNew ?? '');

  // Show diff content when there's either new text or old text (deletions)
  const hasContent =
    (displayNewText && displayNewText.length > 0) ||
    (displayOldText && displayOldText.length > 0);

  // Determine title based on tool name (state-independent)
  const title = getToolLabel('write');

  const params = useMemo(
    () =>
      formatToolParams(
        content,
        portActive
          ? ['path', 'command', 'insertLine', 'insert_line']
          : ['path', 'command', 'insertLine']
      ),
    [content, portActive]
  );
  const summaryIndent = portActive ? 2 : 0;

  // Line numbers should reflect the actual position in the source file.
  // startLine is the 1-based line number from the backend's ToolCallLocation.
  const diffStartLine = startLine ?? 1;

  const language = displayPath?.split('.').pop()?.toLowerCase();

  // Normalize trailing newlines before diffing to prevent phantom
  // "added 1 line" entries caused by mismatched trailing newlines
  // between oldStr and newStr.
  const normalizedOld = normalizeLineEndings(displayOldText || '').replace(
    /\n$/,
    ''
  );
  const normalizedNew = normalizeLineEndings(displayNewText || '').replace(
    /\n$/,
    ''
  );
  const changes = useMemo(
    () => diffLines(normalizedOld, normalizedNew),
    [normalizedOld, normalizedNew]
  );

  // Count added and removed lines
  const { linesAdded, linesRemoved } = useMemo(() => {
    let added = 0;
    let removed = 0;
    for (const change of changes) {
      const lines = change.value.split('\n');
      // diffLines always adds a trailing empty string after the last newline
      const count =
        lines[lines.length - 1] === '' ? lines.length - 1 : lines.length;
      if (change.added) added += count;
      else if (change.removed) removed += count;
    }
    return { linesAdded: added, linesRemoved: removed };
  }, [changes]);

  const hasDiffSummary = linesAdded > 0 || linesRemoved > 0;

  // Count total diff lines for expandable output
  const totalDiffLines = useMemo(() => {
    let count = 0;
    for (const change of changes) {
      const lines = change.value.split('\n');
      count += lines[lines.length - 1] === '' ? lines.length - 1 : lines.length;
    }
    return count;
  }, [changes]);

  const PREVIEW_DIFF_LINES = 20;

  // Off-cohort keeps mainline's unbounded static diff.
  const portActiveDiff = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
  const {
    expanded: expandedFromHook,
    expandHint,
    effectivePreviewCount,
    persistOutput,
  } = useExpandableOutput({
    totalItems: showWriteDiffs ? totalDiffLines : 0,
    previewCount: PREVIEW_DIFF_LINES,
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: portActiveDiff,
  });

  const expanded = isStatic
    ? portActiveDiff
      ? persistOutput
      : true
    : expandedFromHook;
  const diffMaxLines = expanded ? undefined : effectivePreviewCount;

  // If content prop was provided, render with StatusInfo header
  if (content) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={displayPath} shimmer={!isFinished} />
        <ToolMeta params={params} />
        {hasDiffSummary && (
          <Text>
            {summaryIndent > 0 && ' '.repeat(summaryIndent)}
            {linesAdded > 0 &&
              getColor('diff.added.bar')(
                `added ${linesAdded} ${linesAdded === 1 ? 'line' : 'lines'}`
              )}
            {linesAdded > 0 && linesRemoved > 0 && getColor('secondary')(', ')}
            {linesRemoved > 0 &&
              getColor('diff.removed.bar')(
                `removed ${linesRemoved} ${linesRemoved === 1 ? 'line' : 'lines'}`
              )}
            {parsedContent?.insertLine !== undefined &&
              getColor('secondary')(` at L${parsedContent.insertLine}`)}
            {diffStartLine > 1 &&
              parsedContent?.insertLine === undefined &&
              getColor('secondary')(` at L${diffStartLine}`)}
            {getColor('secondary')(
              ` in ${displayPath?.split('/').pop() || displayPath}`
            )}
          </Text>
        )}
        {hasContent && showWriteDiffs && (
          <Box>
            <WriteContent
              changes={changes}
              highlightCode={highlightCode}
              language={language}
              getColor={getColor}
              addedFg={addedFg}
              removedFg={removedFg}
              maxLines={diffMaxLines}
              diffStartLine={diffStartLine}
            />
          </Box>
        )}
        {!expanded && expandHint && showWriteDiffs && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
      </Box>
    );
  }

  // Don't render standalone diff if no content
  if (!displayNewText && !displayOldText) {
    return null;
  }

  let oldLineNum = diffStartLine;
  let newLineNum = diffStartLine;

  // Standalone diff view (no header)
  return (
    <Box flexDirection="column" flexGrow={1} ref={contentRef}>
      {changes.map((change: Change, index: number) => {
        const lines = change.value.split('\n');
        if (lines[lines.length - 1] === '') {
          lines.pop();
        }

        return lines.map((line: string, lineIdx: number) => {
          const expanded = expandTabs(line);

          if (change.removed) {
            const styledLine = removedFg
              ? removedFg(expanded)
              : highlightCode(expanded, language);
            const currentOldLine = oldLineNum++;
            const lineNumber = String(currentOldLine).padStart(4);
            return (
              <Box
                key={`${index}-${lineIdx}`}
                backgroundColor={getColor('diff.removed.background').hex}
              >
                <Text>{getColor('primary')(lineNumber)}</Text>
                <Text>
                  {getColor('diff.removed.bar')('-')}
                  {`  ${styledLine}`}
                </Text>
              </Box>
            );
          } else if (change.added) {
            const styledLine = addedFg
              ? addedFg(expanded)
              : highlightCode(expanded, language);
            const currentNewLine = newLineNum++;
            const lineNumber = String(currentNewLine).padStart(4);
            return (
              <Box
                key={`${index}-${lineIdx}`}
                backgroundColor={getColor('diff.added.background').hex}
              >
                <Text>{getColor('primary')(lineNumber)}</Text>
                <Text>
                  {getColor('diff.added.bar')('+')}
                  {`  ${styledLine}`}
                </Text>
              </Box>
            );
          } else {
            const currentLine = oldLineNum++;
            newLineNum++;
            const lineNumber = String(currentLine).padStart(4);
            const lineContent = `   ${highlightCode(expanded, language)}`;
            return (
              <Text key={`${index}-${lineIdx}`}>
                <Text>{getColor('secondary')(lineNumber)}</Text>
                <Text>{lineContent}</Text>
              </Text>
            );
          }
        });
      })}
    </Box>
  );
});

// Helper component for rendering diff content
interface WriteContentProps {
  changes: Change[];
  highlightCode: (code: string, language?: string) => string;
  language?: string;
  getColor: (path: string) => any;
  /** Flat foreground color for added lines — when set, skips syntax highlighting */
  addedFg?: ((text: string) => string) | undefined;
  /** Flat foreground color for removed lines — when set, skips syntax highlighting */
  removedFg?: ((text: string) => string) | undefined;
  maxLines?: number;
  /** 1-based start line for the diff (defaults to 1) */
  diffStartLine?: number;
}

/** A single flattened diff line with its metadata */
interface DiffLine {
  line: string;
  type: 'added' | 'removed' | 'unchanged';
  lineNum: number;
  key: string;
}

const WriteContent: React.FC<WriteContentProps> = ({
  changes,
  highlightCode,
  language,
  getColor,
  addedFg,
  removedFg,
  maxLines,
  diffStartLine = 1,
}) => {
  // Flatten all changes into individual lines
  const allLines: DiffLine[] = [];
  let oldLineNum = diffStartLine;
  let newLineNum = diffStartLine;

  for (let i = 0; i < changes.length; i++) {
    const change = changes[i]!;
    const lines = change.value.split('\n');
    if (lines[lines.length - 1] === '') lines.pop();

    for (let j = 0; j < lines.length; j++) {
      const lineText = lines[j] ?? '';
      if (change.removed) {
        allLines.push({
          line: lineText,
          type: 'removed',
          lineNum: oldLineNum++,
          key: `${i}-${j}`,
        });
      } else if (change.added) {
        allLines.push({
          line: lineText,
          type: 'added',
          lineNum: newLineNum++,
          key: `${i}-${j}`,
        });
      } else {
        allLines.push({
          line: lineText,
          type: 'unchanged',
          lineNum: oldLineNum,
          key: `${i}-${j}`,
        });
        oldLineNum++;
        newLineNum++;
      }
    }
  }

  const visibleLines = maxLines ? allLines.slice(0, maxLines) : allLines;

  return (
    <Box flexDirection="column" flexGrow={1}>
      {visibleLines.map((dl) => {
        const expanded = expandTabs(dl.line);
        const lineNumber = String(dl.lineNum).padStart(4);

        if (dl.type === 'removed') {
          const styled = removedFg
            ? removedFg(expanded)
            : highlightCode(expanded, language);
          return (
            <Box
              key={dl.key}
              backgroundColor={getColor('diff.removed.background').hex}
            >
              <Text>{getColor('primary')(lineNumber)}</Text>
              <Text>
                {getColor('diff.removed.bar')('-')}
                {`  ${styled}`}
              </Text>
            </Box>
          );
        } else if (dl.type === 'added') {
          const styled = addedFg
            ? addedFg(expanded)
            : highlightCode(expanded, language);
          return (
            <Box
              key={dl.key}
              backgroundColor={getColor('diff.added.background').hex}
            >
              <Text>{getColor('primary')(lineNumber)}</Text>
              <Text>
                {getColor('diff.added.bar')('+')}
                {`  ${styled}`}
              </Text>
            </Box>
          );
        } else {
          const highlighted = highlightCode(expanded, language);
          return (
            <Text key={dl.key}>
              <Text>{getColor('secondary')(lineNumber)}</Text>
              <Text>{`   ${highlighted}`}</Text>
            </Text>
          );
        }
      })}
    </Box>
  );
};
