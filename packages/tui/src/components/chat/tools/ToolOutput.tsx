import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { clipVisibleWidth, softSuccessOutput } from '../../../lite/render.js';
import {
  maxVisibleWidth,
  truncateToWidth,
  visibleWidth,
} from '../../../utils/text-width.js';
import { wrapCellText } from '../../../utils/table-layout.js';
import { chalk } from '../../../utils/color.js';

const BODY_INDENT = 8;

export interface ToolOutputProps {
  lines: string[];
  maxChars?: number | null;
  isError?: boolean;
  expandHint?: string;
  markerAbove?: boolean;
}

// ESC (\x1b) marks an ANSI escape; a raw regex literal trips no-control-regex.
const ESC = String.fromCharCode(27);

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

export interface ToolOutputSectionProps {
  lines: string[];
  isStatic?: boolean;
  isError?: boolean;
  previewCount?: number;
  emptyPlaceholder?: boolean;
}

export const ToolOutputSection = React.memo(function ToolOutputSection({
  lines,
  isStatic = false,
  isError = false,
  previewCount = 10,
  emptyPlaceholder = false,
}: ToolOutputSectionProps) {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const wrapped = React.useMemo(() => {
    const width = termWidth - BODY_INDENT;
    return width <= 0
      ? lines
      : lines.flatMap((line) =>
          visibleWidth(line) <= width
            ? line
            : wrapCellText(line, width, visibleWidth)
        );
  }, [lines, termWidth]);
  const {
    expanded,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  } = useExpandableOutput({
    totalItems: wrapped.length,
    previewCount,
    maxContentWidth: maxVisibleWidth(wrapped),
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: true,
    forceExpanded: isError,
  });

  if (!isError && isStatic && !persistOutput) return null;

  if (wrapped.length === 0) {
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

  const shown = expanded ? wrapped : wrapped.slice(-effectivePreviewCount);
  const hidden = wrapped.length - shown.length;
  const marker =
    hidden > 0
      ? `...+${hidden} lines above${isStatic ? '' : ' (ctrl+o to toggle)'}`
      : expandHint || undefined;

  return (
    <ToolOutput
      lines={shown}
      maxChars={isError ? null : outputMaxChars}
      isError={isError}
      expandHint={marker}
      markerAbove
    />
  );
});

export const ToolOutput = React.memo(function ToolOutput({
  lines,
  maxChars,
  isError = false,
  expandHint,
  markerAbove = false,
}: ToolOutputProps) {
  const { getColor } = useTheme();

  const bodyColor = isError ? chalk.red : softSuccessOutput;

  return (
    <Box marginLeft={2} flexDirection="column">
      <OutputLabel />
      <Box marginLeft={4} flexDirection="column">
        {markerAbove && expandHint && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
        {lines.map((line, i) => {
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
        {!markerAbove && expandHint && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
      </Box>
    </Box>
  );
});
