import React, { useEffect, useMemo, useState } from "react";
import { Box, Scrollbar, Text, getHighlighter, getSegmenter, visibleWidth } from "twinki";
import type { OpenFile } from "../types.js";
import type { ShowcaseTheme } from "../themes.js";

const RESET_FOREGROUND = "\x1b[39m";
const TAB_SIZE = 4;
const segmenter = getSegmenter();

function expandTabs(value: string): string {
  return value
    .split("\n")
    .map((line) => {
      let column = 0;
      let expanded = "";
      for (const { segment } of segmenter.segment(line)) {
        if (segment === "\t") {
          const width = TAB_SIZE - (column % TAB_SIZE);
          expanded += " ".repeat(width);
          column += width;
        } else {
          expanded += segment;
          column += visibleWidth(segment);
        }
      }
      return expanded;
    })
    .join("\n");
}

function colorSequence(color?: string): string {
  const match = color?.match(/^#([\da-f]{2})([\da-f]{2})([\da-f]{2})$/i);
  if (!match) return "";
  return `\x1b[38;2;${parseInt(match[1]!, 16)};${parseInt(match[2]!, 16)};${parseInt(match[3]!, 16)}m`;
}

function useHighlightedLines(file: OpenFile | null, theme: ShowcaseTheme): string[] {
  const displayContent = useMemo(() => expandTabs(file?.content ?? ""), [file]);
  const plain = useMemo(() => displayContent.split("\n"), [displayContent]);
  const [lines, setLines] = useState(plain);

  useEffect(() => {
    setLines(plain);
    if (!file?.language || file.error) return;
    let cancelled = false;
    getHighlighter(theme.syntax, file.language)
      .then((highlighter) => {
        const result = highlighter.codeToTokens(displayContent, {
          lang: file.language!,
          theme: theme.syntax,
        });
        const highlighted = (result.tokens as Array<Array<{ color?: string; content: string }>>).map(
          (tokens) =>
            tokens
              .map((token: { color?: string; content: string }) => `${colorSequence(token.color)}${token.content}`)
              .join("") + RESET_FOREGROUND,
        );
        if (!cancelled) setLines(highlighted);
      })
      .catch(() => {
        if (!cancelled) setLines(plain);
      });
    return () => {
      cancelled = true;
    };
  }, [displayContent, file, plain, theme.syntax]);

  return lines;
}

export interface FileViewerProps {
  file: OpenFile | null;
  scrollTop: number;
  width: number;
  height: number;
  theme: ShowcaseTheme;
  showHeader?: boolean;
  onScroll: (value: number) => void;
}

export function FileViewer({
  file,
  scrollTop,
  width,
  height,
  theme,
  showHeader = true,
  onScroll,
}: FileViewerProps): React.ReactElement {
  const lines = useHighlightedLines(file, theme);
  const bodyHeight = Math.max(1, height - (showHeader ? 1 : 0));
  const maxScroll = Math.max(0, lines.length - bodyHeight);
  const top = Math.min(scrollTop, maxScroll);
  const gutterWidth = Math.max(2, String(lines.length).length);
  const visible = lines.slice(top, top + bodyHeight);

  return (
    <Box flexDirection="column" width={width} height={height} backgroundColor={theme.bg}>
      {showHeader ? (
        <Box height={1} paddingX={1} backgroundColor={theme.raised} selectionScope>
          <Text color={theme.accent} bold wrap="truncate-middle">
            {file?.relativePath ?? "Select a file"}
          </Text>
        </Box>
      ) : null}
      {file?.error ? (
        <Box paddingX={1} selectionScope>
          <Text color={theme.danger}>{file.error}</Text>
        </Box>
      ) : file ? (
        <Box flexDirection="row" height={bodyHeight}>
          <Box flexDirection="column" width={Math.max(1, width - 1)} selectionScope>
            {Array.from({ length: bodyHeight }, (_, index) => {
              const lineIndex = top + index;
              const line = visible[index];
              return (
                <Text key={lineIndex} color={theme.fg} wrap="truncate">
                  <Text color={theme.muted}>{`${String(lineIndex + 1).padStart(gutterWidth)} | `}</Text>
                  {line ?? ""}
                </Text>
              );
            })}
          </Box>
          <Scrollbar
            scrollTop={top}
            totalLines={lines.length}
            viewportHeight={bodyHeight}
            color={theme.border}
            thumbColor={theme.accent}
            onScrollTo={onScroll}
          />
        </Box>
      ) : (
        <Box paddingX={2} paddingY={1} selectionScope>
          <Text color={theme.muted}>Choose a file from the rail to open a read-only preview.</Text>
        </Box>
      )}
    </Box>
  );
}
