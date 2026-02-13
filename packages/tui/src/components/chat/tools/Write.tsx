import React, { useEffect, useRef, useMemo } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { diffLines, type Change } from 'diff';

export interface WriteProps {
  /** Old text content for diff (empty string for new files) */
  oldText: string;

  /** New text content for diff */
  newText: string;

  /** File path for syntax highlighting and display */
  filePath?: string;

  /** Line offset for status bar coloring */
  lineOffset?: number;

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
 * - Status bar line coloring for added/removed lines
 * - Parses tool call content for command type detection
 * - Supports create, strReplace, and insert operations
 */
export const Write = React.memo<WriteProps>(function Write({
  oldText,
  newText,
  filePath,
  lineOffset = 0,
  isFinished = false,
  isStatic = false,
  content,
}) {
  const { getColor } = useTheme();
  const highlightCode = useSyntaxHighlight();
  const contentRef = useRef<any>(null);

  let statusBar: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBar = useStatusBar();
  } catch {
    // Not within a StatusBar
  }

  // Parse content if provided to extract operation details
  const parsedContent = useMemo(() => {
    if (!content) return null;
    try {
      return JSON.parse(content);
    } catch {
      return null;
    }
  }, [content]);

  // Determine display values from parsed content or props
  const displayPath = parsedContent?.path || filePath;
  const displayOldText = parsedContent?.oldStr ?? oldText;
  const displayNewText =
    parsedContent?.newStr ?? parsedContent?.content ?? newText;

  const hasContent = displayNewText && displayNewText.length > 0;

  // Determine title based on command type
  const title = useMemo(() => {
    if (!parsedContent) {
      return isFinished ? 'Wrote' : 'Writing';
    }

    const { command, insertLine } = parsedContent;
    switch (command) {
      case 'create':
        return isFinished ? 'Created' : 'Creating';
      case 'strReplace':
        return isFinished ? 'Replaced' : 'Replacing';
      case 'insert':
        return isFinished ? 'Inserted' : 'Inserting';
      default:
        return isFinished ? 'Wrote' : 'Writing';
    }
  }, [parsedContent, isFinished]);

  const language = displayPath?.split('.').pop()?.toLowerCase();
  const changes = diffLines(displayOldText || '', displayNewText || '');

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

  // Calculate available width for content
  const fillsEdgeMargin = process.env.TERM_PROGRAM === 'iTerm.app';
  const terminalWidth =
    (process.stdout.columns || 80) - (fillsEdgeMargin ? 5 : 4);
  const contentWidth = terminalWidth - 7; // Line number (4) + prefix (3)

  const PREVIEW_DIFF_LINES = 20;

  // Use expandable output hook for collapsing large diffs
  const {
    expanded: expandedFromHook,
    hiddenCount,
    expandHint,
  } = useExpandableOutput({
    totalItems: totalDiffLines,
    previewCount: PREVIEW_DIFF_LINES,
    isStatic,
    unit: 'lines',
  });

  // Static/past turns always show full diff
  const expanded = isStatic || expandedFromHook;

  const truncateLine = (line: string, maxWidth: number): string => {
    if (line.length <= maxWidth) return line;
    return line.slice(0, maxWidth - 1) + '…';
  };

  // When rendered with a StatusInfo header (content prop), the diff starts after:
  // - 1 line for StatusInfo header
  // - 1 line for diff summary (when hasDiffSummary)
  const headerLines = content ? (hasDiffSummary ? 2 : 1) : 0;

  // Set bar colors for each diff line (only visible lines when collapsed)
  const visibleDiffLines = expanded
    ? totalDiffLines
    : Math.min(totalDiffLines, PREVIEW_DIFF_LINES);

  useEffect(() => {
    if (!statusBar) return;

    const colors = new Map<number, string>();
    let currentLine = lineOffset + headerLines;
    let diffLineCount = 0;
    for (const change of changes) {
      const lines = change.value.split('\n');
      if (lines[lines.length - 1] === '') lines.pop();
      for (const _ of lines) {
        if (diffLineCount >= visibleDiffLines) break;
        if (change.removed) {
          colors.set(currentLine, getColor('diff.removed.bar').hex);
        } else if (change.added) {
          colors.set(currentLine, getColor('diff.added.bar').hex);
        } else {
          colors.set(currentLine, getColor('diff.unchanged.bar').hex);
        }
        currentLine++;
        diffLineCount++;
      }
      if (diffLineCount >= visibleDiffLines) break;
    }
    statusBar.setLineColors(colors);
  }, [statusBar, changes, getColor, lineOffset, headerLines, visibleDiffLines]);

  // If content prop was provided, render with StatusInfo header
  if (content) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={displayPath} shimmer={!isFinished} />
        {hasDiffSummary && (
          <Text>
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
            {getColor('secondary')(
              ` in ${displayPath?.split('/').pop() || displayPath}`
            )}
          </Text>
        )}
        {hasContent && (
          <Box>
            <WriteContent
              changes={changes}
              highlightCode={highlightCode}
              language={language}
              contentWidth={contentWidth}
              truncateLine={truncateLine}
              getColor={getColor}
              maxLines={expanded ? undefined : PREVIEW_DIFF_LINES}
            />
          </Box>
        )}
        {expandHint && !expanded && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
      </Box>
    );
  }

  // Don't render standalone diff if no content
  if (!displayNewText && !displayOldText) {
    return null;
  }

  let oldLineNum = 1;
  let newLineNum = 1;

  // Standalone diff view (no header)
  return (
    <Box flexDirection="column" width="90%" ref={contentRef}>
      {changes.map((change: Change, index: number) => {
        const lines = change.value.split('\n');
        if (lines[lines.length - 1] === '') {
          lines.pop();
        }

        return lines.map((line: string, lineIdx: number) => {
          const truncatedLine = truncateLine(line, contentWidth);
          const highlightedLine = highlightCode(truncatedLine, language);

          if (change.removed) {
            const currentOldLine = oldLineNum++;
            const lineNumber = String(currentOldLine).padStart(4);
            const lineContent = `-  ${highlightedLine}`;
            return (
              <Box
                key={`${index}-${lineIdx}`}
                backgroundColor={getColor('diff.removed.background').hex}
              >
                <Text>{getColor('primary')(lineNumber)}</Text>
                <Text>{lineContent}</Text>
              </Box>
            );
          } else if (change.added) {
            const currentNewLine = newLineNum++;
            const lineNumber = String(currentNewLine).padStart(4);
            const lineContent = `+  ${highlightedLine}`;
            return (
              <Box
                key={`${index}-${lineIdx}`}
                backgroundColor={getColor('diff.added.background').hex}
              >
                <Text>{getColor('primary')(lineNumber)}</Text>
                <Text>{lineContent}</Text>
              </Box>
            );
          } else {
            const currentLine = oldLineNum++;
            newLineNum++;
            const lineNumber = String(currentLine).padStart(4);
            const lineContent = `   ${highlightedLine}`;
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
  contentWidth: number;
  truncateLine: (line: string, maxWidth: number) => string;
  getColor: (path: string) => any;
  maxLines?: number;
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
  contentWidth,
  truncateLine,
  getColor,
  maxLines,
}) => {
  // Flatten all changes into individual lines
  const allLines: DiffLine[] = [];
  let oldLineNum = 1;
  let newLineNum = 1;

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
    <Box flexDirection="column" width="90%">
      {visibleLines.map((dl) => {
        const truncated = truncateLine(dl.line, contentWidth);
        const highlighted = highlightCode(truncated, language);
        const lineNumber = String(dl.lineNum).padStart(4);

        if (dl.type === 'removed') {
          return (
            <Box
              key={dl.key}
              backgroundColor={getColor('diff.removed.background').hex}
            >
              <Text>{getColor('primary')(lineNumber)}</Text>
              <Text>{`-  ${highlighted}`}</Text>
            </Box>
          );
        } else if (dl.type === 'added') {
          return (
            <Box
              key={dl.key}
              backgroundColor={getColor('diff.added.background').hex}
            >
              <Text>{getColor('primary')(lineNumber)}</Text>
              <Text>{`+  ${highlighted}`}</Text>
            </Box>
          );
        } else {
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
