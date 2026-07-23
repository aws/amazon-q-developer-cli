import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { clipChars, wrapStyled } from '../../../lite/render.js';
import { normalizeLineEndings } from '../../../utils/string.js';
import {
  useToolArgsExpanded,
  useVerbosityToolContext,
} from '../../ui/VerbosityToolContext.js';

const LEFT_MARGIN = 2;
const ROW_PREFIX_WIDTH = 2;

export interface ToolMetaProps {
  params: string[] | null;
}

export const ToolMeta = React.memo(function ToolMeta({
  params,
}: ToolMetaProps) {
  return process.env.KIRO_LITE_ROLLOUT_ENABLED === '1' ? (
    <VerbosityToolMeta params={params} />
  ) : (
    <LegacyToolMeta params={params} />
  );
});

const LegacyToolMeta = React.memo(function LegacyToolMeta({
  params,
}: ToolMetaProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  if (!params || params.length === 0) return null;

  const color = getColor('muted');
  return (
    <Box marginLeft={LEFT_MARGIN}>
      <Text wrap="wrap">
        {color(`${glyphs.cornerBottomLeftRound} `)}
        {color(params.join(', '))}
      </Text>
    </Box>
  );
});

const VerbosityToolMeta = React.memo(function VerbosityToolMeta({
  params,
}: ToolMetaProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const display = useVerboseDisplay();
  const { width: termWidth } = useTerminalSize();
  const { isStatic } = useVerbosityToolContext();

  const fullText = normalizeLineEndings(params?.join(', ') ?? '');
  const clippedText = fullText
    .split('\n')
    .map((line) => clipChars(line, display.argsMaxChars))
    .join('\n');
  const rowWidth = Math.max(1, termWidth - LEFT_MARGIN - ROW_PREFIX_WIDTH);
  const fullRows = wrapStyled(fullText, rowWidth, rowWidth);
  const clippedRows = wrapStyled(clippedText, rowWidth, rowWidth);
  const cap = display.argsMaxLines;
  const collapsedLines =
    cap != null && cap > 0 ? clippedRows.slice(0, cap) : clippedRows;
  const dropped = Math.max(0, clippedRows.length - collapsedLines.length);
  const charsClipped = clippedText !== fullText;
  const expanded = useToolArgsExpanded(dropped > 0 || charsClipped);
  const lines = expanded ? fullRows : collapsedLines;

  if (!params || params.length === 0) return null;
  if (display.toolArgsMode === 'off') return null;

  const color = getColor('muted');
  const truncationMarker =
    dropped > 0
      ? `  ... (+${dropped} more lines)${isStatic ? '' : ' (ctrl+o to toggle)'}`
      : charsClipped && !isStatic
        ? '  ... (ctrl+o to toggle)'
        : null;

  return (
    <Box marginLeft={LEFT_MARGIN} flexDirection="column">
      {lines.map((line, index) => (
        <Text key={index}>
          {color(index === 0 ? `${glyphs.cornerBottomLeftRound} ` : '  ')}
          {color(line)}
        </Text>
      ))}
      {!expanded && truncationMarker && <Text>{color(truncationMarker)}</Text>}
    </Box>
  );
});
