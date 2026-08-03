import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import {
  boundToolOutputLine,
  clipVisibleWidth,
  softSuccessOutput,
  takeTailRows,
  wrapAnsiLine,
} from '../../../lite/render.js';
import { truncateToWidth, visibleWidth } from '../../../utils/text-width.js';
import { chalk } from '../../../utils/color.js';

const BODY_INDENT = 8;

export interface ToolOutputProps {
  lines?: readonly string[];
  /** Preserve immutable streaming chunks so prior output is not rewrapped. */
  chunks?: readonly (readonly string[])[];
  isStatic?: boolean;
  isError?: boolean;
  previewPosition?: 'start' | 'end';
  emptyPlaceholder?: boolean;
}

// ESC (\x1b) marks an ANSI escape; a raw regex literal trips no-control-regex.
const ESC = String.fromCharCode(27);

interface WrappedChunk {
  rows: string[];
  maxWidth: number;
}

const wrappedChunkCache = new WeakMap<
  readonly string[],
  Map<string, WrappedChunk>
>();

function wrapChunk(
  lines: readonly string[],
  width: number,
  position: 'start' | 'end'
): WrappedChunk {
  let byWidth = wrappedChunkCache.get(lines);
  const cacheKey = `${width}:${position}:${chalk.level}`;
  const cached = byWidth?.get(cacheKey);
  if (cached) return cached;

  const rows: string[] = [];
  let maxWidth = 0;
  const appendWrapped = (text: string) => {
    const textWidth = visibleWidth(text);
    const wrapped =
      textWidth <= width ? [text] : wrapAnsiLine(text, width, width);
    rows.push(...wrapped);
    for (const row of wrapped) {
      maxWidth = Math.max(maxWidth, visibleWidth(row));
    }
  };

  for (const line of lines) {
    const bounded = boundToolOutputLine(line, position);
    const clipMarker =
      bounded.droppedChars > 0
        ? `... (line clipped; +${bounded.droppedChars} chars ${position === 'start' ? 'after' : 'before'})`
        : null;

    if (clipMarker && position === 'end') {
      appendWrapped(chalk.dim(clipMarker));
    }
    appendWrapped(bounded.text);
    if (clipMarker && position === 'start') {
      appendWrapped(chalk.dim(clipMarker));
    }
  }

  const result = { rows, maxWidth };
  if (!byWidth) {
    byWidth = new Map();
    wrappedChunkCache.set(lines, byWidth);
  }
  byWidth.set(cacheKey, result);
  return result;
}

function takeWrappedRows(
  chunks: readonly WrappedChunk[],
  count: number,
  position: 'start' | 'end'
): string[] {
  if (count <= 0) return [];
  if (position === 'start') {
    const rows: string[] = [];
    for (const chunk of chunks) {
      const remaining = count - rows.length;
      if (remaining <= 0) break;
      rows.push(...chunk.rows.slice(0, remaining));
    }
    return rows;
  }

  return takeTailRows(chunks, count);
}

function OutputLabel() {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const muted = getColor('muted');
  return (
    <Text>
      {muted(`${glyphs.cornerBottomLeftRound} `)}
      {muted('output:')}
    </Text>
  );
}

export const ToolOutputHeader = React.memo(function ToolOutputHeader() {
  return (
    <Box marginLeft={2}>
      <OutputLabel />
    </Box>
  );
});

export const ToolOutput = React.memo(function ToolOutput({
  lines = [],
  chunks,
  isStatic = false,
  isError = false,
  previewPosition = 'end',
  emptyPlaceholder = false,
}: ToolOutputProps) {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const singletonChunks = React.useMemo(() => [lines], [lines]);
  const sourceChunks = chunks ?? singletonChunks;
  const wrapped = React.useMemo(() => {
    // Hard-wrap before slicing so terminal soft wraps count against the row cap.
    const width = Math.max(1, termWidth - BODY_INDENT);
    const wrappedChunks = sourceChunks.map((chunk) =>
      wrapChunk(chunk, width, previewPosition)
    );
    let totalRows = 0;
    let maxWidth = 0;
    for (const chunk of wrappedChunks) {
      totalRows += chunk.rows.length;
      maxWidth = Math.max(maxWidth, chunk.maxWidth);
    }
    return { chunks: wrappedChunks, totalRows, maxWidth };
  }, [sourceChunks, termWidth, previewPosition]);
  const {
    expanded,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  } = useExpandableOutput({
    totalItems: wrapped.totalRows,
    previewCount: wrapped.totalRows,
    maxContentWidth: wrapped.maxWidth,
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: true,
    forceExpanded: isError,
  });

  if (!isError && isStatic && !persistOutput) return null;

  if (wrapped.totalRows === 0) {
    if (!emptyPlaceholder) return null;
    return (
      <Box marginLeft={2} flexDirection="column">
        <OutputLabel />
        <Box marginLeft={4}>
          <Text>{getColor('muted')('(no output)')}</Text>
        </Box>
      </Box>
    );
  }

  const shown = expanded
    ? wrapped.chunks.flatMap((chunk) => chunk.rows)
    : takeWrappedRows(wrapped.chunks, effectivePreviewCount, previewPosition);
  const hidden = wrapped.totalRows - shown.length;
  const marker =
    hidden > 0
      ? `...+${hidden} lines${previewPosition === 'end' ? ' above' : ''}${isStatic ? '' : ' (ctrl+o to toggle)'}`
      : expandHint || undefined;
  const bodyColor = isError ? chalk.red : softSuccessOutput;
  const markerAbove = previewPosition === 'end';
  const maxChars = isError ? null : outputMaxChars;

  return (
    <Box marginLeft={2} flexDirection="column">
      <OutputLabel />
      <Box marginLeft={4} flexDirection="column">
        {markerAbove && marker && <Text>{getColor('secondary')(marker)}</Text>}
        {shown.map((line, i) => {
          // Preserve embedded ANSI styling and append its reset when clipped.
          const hasAnsi = line.includes(ESC);
          const cap = maxChars != null && maxChars > 0;
          const clipped = cap
            ? hasAnsi
              ? clipVisibleWidth(line, maxChars)
              : truncateToWidth(line, maxChars)
            : line;
          return (
            <Text key={i} wrap="wrap">
              {hasAnsi ? clipped : bodyColor(clipped)}
            </Text>
          );
        })}
        {!markerAbove && marker && <Text>{getColor('secondary')(marker)}</Text>}
      </Box>
    </Box>
  );
});
