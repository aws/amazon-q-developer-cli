import React from 'react';
import { Box } from 'ink';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface ActionHintProps {
  text: string;
  visible?: boolean;
}

export const ActionHint: React.FC<ActionHintProps> = ({ text, visible = true }) => {
  const { getColor } = useTheme();
  const secondary = getColor('secondary');

  if (!visible) return null;

  return (
    <Box paddingX={1} marginBottom={1}>
      <Text>{secondary.italic(text)}</Text>
    </Box>
  );
};
