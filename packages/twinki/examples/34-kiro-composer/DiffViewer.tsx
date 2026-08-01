import React from 'react';
import { Box, DiffView, Scrollbar, Text } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';

export function DiffViewer({
  path,
  before,
  after,
  language,
  scrollTop,
  width,
  height,
  theme,
  showHeader = true,
  onScroll,
}: {
  path: string;
  before: string;
  after: string;
  language?: string;
  scrollTop: number;
  width: number;
  height: number;
  theme: ShowcaseTheme;
  showHeader?: boolean;
  onScroll: (value: number) => void;
}): React.ReactElement {
  const bodyHeight = Math.max(1, height - (showHeader ? 1 : 0));
  const horizontal = width >= 96;
  const beforeLines = before.split('\n').length;
  const afterLines = after.split('\n').length;
  const totalLines = horizontal ? Math.max(beforeLines, afterLines) + 1 : beforeLines + afterLines;
  const top = Math.min(scrollTop, Math.max(0, totalLines - bodyHeight));

  return (
    <Box width={width} height={height} flexDirection="column" backgroundColor={theme.bg}>
      {showHeader ? (
        <Box height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.raised} selectionScope>
          <Text color={theme.accent} bold wrap="truncate-middle">
            {path}
          </Text>
          <Text color={theme.muted} wrap="truncate">
            {horizontal ? 'SIDE BY SIDE' : 'UNIFIED'}
          </Text>
        </Box>
      ) : null}
      <Box height={bodyHeight} flexDirection="row">
        <Box width={Math.max(1, width - 1)} height={bodyHeight} overflow="hidden" scrollTop={top} selectionScope>
          <DiffView
            values={[before, after]}
            layout={horizontal ? 'horizontal' : 'vertical'}
            highlight={Boolean(language)}
            lang={language}
            theme={theme.syntax}
          />
        </Box>
        <Scrollbar
          scrollTop={top}
          totalLines={totalLines}
          viewportHeight={bodyHeight}
          color={theme.border}
          thumbColor={theme.accent}
          onScrollTo={onScroll}
        />
      </Box>
    </Box>
  );
}
