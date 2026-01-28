import React, { useEffect, useRef } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useSyntaxHighlight } from '../../../utils/syntax-highlight.js';
import { useStatusBar } from '../status-bar/StatusBar.js';

// Simple diff implementation
interface Change {
  added?: boolean;
  removed?: boolean;
  value: string;
}

const simpleDiff = (oldText: string, newText: string): Change[] => {
  const oldLines = oldText.split('\n');
  const newLines = newText.split('\n');
  const changes: Change[] = [];

  // Very basic diff - just compare line by line
  const maxLines = Math.max(oldLines.length, newLines.length);

  for (let i = 0; i < maxLines; i++) {
    const oldLine = oldLines[i];
    const newLine = newLines[i];

    if (oldLine === undefined) {
      // Line was added
      changes.push({ added: true, value: newLine + '\n' });
    } else if (newLine === undefined) {
      // Line was removed
      changes.push({ removed: true, value: oldLine + '\n' });
    } else if (oldLine !== newLine) {
      // Line was changed - show as remove + add
      changes.push({ removed: true, value: oldLine + '\n' });
      changes.push({ added: true, value: newLine + '\n' });
    } else {
      // Line unchanged
      changes.push({ value: oldLine + '\n' });
    }
  }

  return changes;
};

export interface WriteProps {
  oldText: string;
  newText: string;
  filePath?: string;
  /** Line offset for status bar coloring (to account for parent content above this component) */
  lineOffset?: number;
}

export const Write = React.memo<WriteProps>(function Write({
  oldText,
  newText,
  filePath,
  lineOffset = 0,
}) {
  const { getColor } = useTheme();
  const highlightCode = useSyntaxHighlight();
  const contentRef = useRef<any>(null);

  let statusBar: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBar = useStatusBar();
  } catch {
    // Not within a StatusBar, that's okay
  }

  const language = filePath?.split('.').pop()?.toLowerCase();
  const changes = simpleDiff(oldText, newText);
  
  // Calculate available width for content
  // Terminal width - status bar (1) - margin (1) - some buffer for edge cases
  const fillsEdgeMargin = process.env.TERM_PROGRAM === 'iTerm.app';
  const terminalWidth = (process.stdout.columns || 80) - (fillsEdgeMargin ? 5 : 4);
  
  // Line number (4) + prefix (+/- and spaces, 3) = 7 chars before content
  const contentWidth = terminalWidth - 7;
  
  // Truncate a string to fit within the available width
  const truncateLine = (line: string, maxWidth: number): string => {
    if (line.length <= maxWidth) return line;
    return line.slice(0, maxWidth - 1) + '…';
  };

  // Set bar colors for each diff line - colors are relative to this component's position
  // The parent component is responsible for reserving lines before this component
  useEffect(() => {
    if (!statusBar) return;

    let currentLine = lineOffset;
    changes.forEach((change: Change) => {
      // Split and remove only the trailing empty string from the \n at the end
      const lines = change.value.split('\n');
      if (lines[lines.length - 1] === '') {
        lines.pop();
      }
      lines.forEach(() => {
        if (change.removed) {
          statusBar.setLineColor(currentLine, getColor('diff.removed.bar').hex);
        } else if (change.added) {
          statusBar.setLineColor(currentLine, getColor('diff.added.bar').hex);
        }
        // Unchanged lines use default bar color
        currentLine++;
      });
    });
  }, [statusBar, changes, getColor, lineOffset]);

  let oldLineNum = 1;
  let newLineNum = 1;

  return (
    <Box flexDirection="column" width="80%" ref={contentRef}>
      {changes.map((change: Change, index: number) => {
        // Split and remove only the trailing empty string from the \n at the end
        const lines = change.value.split('\n');
        if (lines[lines.length - 1] === '') {
          lines.pop();
        }

        return lines.map((line: string, lineIdx: number) => {
          // Truncate line to prevent wrapping
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
