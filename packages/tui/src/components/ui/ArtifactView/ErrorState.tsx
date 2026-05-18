import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface Props {
  message: string;
}

export const ErrorState: React.FC<Props> = ({ message }) => {
  const { getColor } = useTheme();
  const error = getColor('error');
  return (
    <Box marginTop={1} marginBottom={1}>
      <Text>{error(message)}</Text>
    </Box>
  );
};
