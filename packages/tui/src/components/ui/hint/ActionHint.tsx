import React from 'react';
import { Box, Text as InkText } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface ActionHintProps {
  text: string;
  visible?: boolean;
  /** When true, renders as an overlay with blue background badge + standard hint */
  overlay?: {
    badge: string;
    hint: string;
  };
}

export const ActionHint: React.FC<ActionHintProps> = ({
  text,
  visible = true,
  overlay,
}) => {
  const { getColor } = useTheme();

  if (!visible) return null;

  if (overlay) {
    const highlightHex = getColor('highlight').hex;
    // Use snackbar text color (white) for good contrast on blue background
    const whiteHex = getColor('components.snackbar.text').hex;

    return (
      <Box paddingX={1} marginBottom={1} flexDirection="row" gap={0}>
        <InkText backgroundColor={highlightHex} color={whiteHex}>
          {overlay.badge}
        </InkText>
        <Text> · {getColor('primary')(overlay.hint)}</Text>
      </Box>
    );
  }

  const dim = getColor('muted');

  return (
    <Box paddingX={1} marginBottom={1} justifyContent="flex-end">
      <Text>{dim(text)}</Text>
    </Box>
  );
};
