import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

// Inline "Preview" chrome for the mini preview + truncation editor; the
// scrollable pane has its own variant (counter + fixed-height body).
export const PreviewFrame: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { getColor } = useTheme();
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  return (
    <Box flexDirection="column" marginTop={1}>
      <Divider />
      <Box paddingX={1} flexDirection="column">
        <Text>{dim('Preview')}</Text>
        {children}
      </Box>
    </Box>
  );
};
