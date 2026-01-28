import React from 'react';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface DividerProps {
  character?: string;
  color?: string; // Theme color path like 'border'
  width?: number;
}

export const Divider = React.memo(function Divider({
  character = '─',
  color = 'border',
  width,
}: DividerProps) {
  const { getColor } = useTheme();
  const dividerColor = getColor(color);
  const dividerWidth = width || process.stdout.columns || 80;

  return <Text>{dividerColor(character.repeat(dividerWidth))}</Text>;
});
