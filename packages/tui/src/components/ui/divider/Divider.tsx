import React from 'react';
import { Box, Text } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';

export interface DividerProps {
  color?: string; // Theme color path like 'surface'
}

export const Divider = React.memo(function Divider({
  color = 'surface',
}: DividerProps) {
  const { getColor } = useTheme();
  const { width } = useTerminalSize();
  const line = getColor(color)('─'.repeat(width));

  return (
    <Box width="100%">
      <Text wrap="truncate">{line}</Text>
    </Box>
  );
});
