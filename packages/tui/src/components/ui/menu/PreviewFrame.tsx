import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

/**
 * Shared "Preview" chrome for the verbosity menus: a top-margin divider with a
 * dim `Preview` label above the rendered body. The scrollable pane keeps its
 * own variant (counter label + fixed-height body); this covers the two inline
 * sites (mini preview, truncation editor) that share identical framing.
 */
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
