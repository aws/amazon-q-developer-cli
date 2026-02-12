import React, { useEffect, useRef, useMemo } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
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
  const displayNewText = parsedContent?.newStr ?? parsedContent?.content ?? newText;

  // Check if we have meaningful content to show
  const hasContent = displayNewText && displayNewText.length > 0;
  const isGenerating = !isFinished && !hasContent;

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
        return isFinished ? 'Replaced in' : 'Replacing in';
      case 'insert':
        if (insertLine !== undefined) {
          return isFinished
            ? `Inserted at line ${insertLine} in`
            : `Inserting at line ${insertLine} in`;
        }
        return isFinished ? 'Appended to' : 'Appending to';
      default:
        return isFinished ? 'Wrote' : 'Writing';
    }
  }, [parsedContent, isFinished]);

  const language = displayPath?.split('.').pop()?.toLowerCase();
  const changes = diffLines(displayOldText || '', displayNewText || '');

  // Calculate available width for content
  const fillsEdgeMargin = process.env.TERM_PROGRAM === 'iTerm.app';
  const terminalWidth = (process.stdout.columns || 80) - (fillsEdgeMargin ? 5 : 4);
  const contentWidth = terminalWidth - 7; // Line number (4) + prefix (3)

  const truncateLine = (line: string, maxWidth: number): string => {
    if (line.length <= maxWidth) return line;
    return line.slice(0, maxWidth - 1) + '…';
  };

  // When rendered with a StatusInfo header (content prop), the diff starts after:
  // - 1 line for StatusInfo header
  // - 1 line for marginTop={1}
  const headerLines = content ? 2 : 0;

  // Set bar colors for each diff line
  useEffect(() => {
    if (!statusBar) return;

    let currentLine = lineOffset + headerLines;
    changes.forEach((change: Change) => {
      const lines = change.value.split('\n');
      if (lines[lines.length - 1] === '') {
        lines.pop();
      }
      lines.forEach(() => {
        if (change.removed) {
          statusBar.setLineColor(currentLine, getColor('diff.removed.bar').hex);
        } else if (change.added) {
          statusBar.setLineColor(currentLine, getColor('diff.added.bar').hex);
        } else {
          statusBar.setLineColor(currentLine, getColor('diff.unchanged.bar').hex);
        }
        currentLine++;
      });
    });
  }, [statusBar, changes, getColor, lineOffset, headerLines]);

  // Don't render if no content
  if (!displayNewText && !displayOldText) {
    return null;
  }

  let oldLineNum = 1;
  let newLineNum = 1;

  // If content prop was provided, render with StatusInfo header
  if (content) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={displayPath} shimmer={!isFinished} />
        {isGenerating ? (
          <Box marginTop={1} marginLeft={2}>
            <Text>{getColor('secondary')('Generating content...')}</Text>
          </Box>
        ) : (
          <Box marginTop={1}>
            <WriteContent
              changes={changes}
              highlightCode={highlightCode}
              language={language}
              contentWidth={contentWidth}
              truncateLine={truncateLine}
              getColor={getColor}
            />
          </Box>
        )}
      </Box>
    );
  }

  // Standalone diff view (no header)
  return (
    <Box flexDirection="column" width="80%" ref={contentRef}>
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
}

const WriteContent: React.FC<WriteContentProps> = ({
  changes,
  highlightCode,
  language,
  contentWidth,
  truncateLine,
  getColor,
}) => {
  let oldLineNum = 1;
  let newLineNum = 1;

  return (
    <Box flexDirection="column" width="80%">
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
};
