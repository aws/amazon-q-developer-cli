import React from 'react';
import { Box, Text } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';

export interface DividerProps {
  color?: string; // Theme color path like 'surface'
}

export const Divider = React.memo(function Divider({
  color = 'surface',
}: DividerProps) {
  const { getColor } = useTheme();
  const { width } = useTerminalSize();
  const glyphs = useGlyphs();
  // Route the rule through the glyph set so it degrades to '-' in ASCII mode.
  // Both variants are single-width, so repeating by `width` is unchanged.
  const line = getColor(color)(glyphs.lineHorizontal.repeat(width));

  return (
    <Box width="100%">
      <Text wrap="truncate">{line}</Text>
    </Box>
  );
});
