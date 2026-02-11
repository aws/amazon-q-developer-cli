import React from 'react';
import { Box } from 'ink';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface DividerProps {
  color?: string; // Theme color path like 'border'
}

export const Divider = React.memo(function Divider({
  color = 'border',
}: DividerProps) {
  const { getColor } = useTheme();
  const dividerColor = getColor(color).hex;

  return (
    <Box
      width="100%"
      borderStyle="single"
      borderTop={true}
      borderBottom={false}
      borderLeft={false}
      borderRight={false}
      borderTopColor={dividerColor}
    />
  );
});
