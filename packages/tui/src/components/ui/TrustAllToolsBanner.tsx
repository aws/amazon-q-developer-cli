import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';

/**
 * Persistent banner displayed above the prompt bar for the entire session
 * when trust-all-tools mode is active. Yellow/orange warning colour.
 * Never scrolls away — only disappears when the session ends.
 */
export const TrustAllToolsBanner: React.FC = () => {
  const { getColor } = useTheme();
  const warning = getColor('warning');
  const secondary = getColor('secondary');

  return (
    <Box flexDirection="column">
      <Divider />
      <Box paddingLeft={1}>
        <Text>
          {warning('Trust All Tools active, confirmations are off')}
          {secondary(' \u00B7 /quit to exit')}
        </Text>
      </Box>
    </Box>
  );
};
