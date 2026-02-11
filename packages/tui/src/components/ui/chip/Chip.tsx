import React from 'react';
import { Box } from 'ink';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export enum ChipColor {
  PRIMARY = 'primary',
  SECONDARY = 'secondary',
  BRAND = 'brand',
  SUCCESS = 'success',
  WARNING = 'warning',
  ERROR = 'error',
}

interface ChipProps {
  /** The value/name to display */
  value: string;
  /** Color preset for the chip */
  color?: ChipColor;
  /** Custom hex color (overrides color preset) */
  hexColor?: string;
  /** Prefix text to display before the value */
  prefix?: string;
  /** Whether to wrap the value in parentheses */
  wrap?: boolean;
  /** Whether to show a muted background */
  background?: boolean;
}

export default function Chip({
  value,
  color = ChipColor.PRIMARY,
  hexColor,
  prefix,
  wrap = false,
  background = false,
}: ChipProps) {
  const { getColor } = useTheme();

  // Don't render anything if no value
  if (!value) return null;

  const content = (
    <Text>
      {prefix && getColor(ChipColor.SECONDARY)(prefix)}
      {wrap && getColor(ChipColor.SECONDARY)('(')}
      {hexColor ? <Text color={hexColor}>{value}</Text> : getColor(color)(value)}
      {wrap && getColor(ChipColor.SECONDARY)(')')}
    </Text>
  );

  if (background) {
    return (
      <Box backgroundColor={getColor('muted').hex}>
        {content}
      </Box>
    );
  }

  return content;
}
