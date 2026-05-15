import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';

interface ProgressChipProps {
  /** Progress value from 0-100 */
  value: number;
  /** Optional text label to show after the percentage */
  label?: string;
  /** Whether to show percentage text after the icon */
  showPercentage?: boolean;
  /** Threshold percentage where color changes from green to yellow (default: 60) */
  warningThreshold?: number;
  /** Optional chalk color function that overrides all colors (e.g. for dimmed state) */
  colorOverride?: (text: string) => string;
}

export default function ProgressChip({
  value,
  label,
  showPercentage = true,
  warningThreshold = 60,
  colorOverride,
}: ProgressChipProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();

  // Clamp value between 0 and 100
  const clampedValue = Math.max(0, Math.min(100, value));

  // Determine icon and color based on value
  let icon: string;
  let colorFn: (text: string) => string;

  if (clampedValue === 0) {
    icon = glyphs.progress0;
    colorFn = getColor('success');
  } else if (clampedValue <= 25) {
    icon = glyphs.progress25;
    colorFn = getColor('success');
  } else if (clampedValue <= 50) {
    icon = glyphs.progress50;
    colorFn = getColor('success');
  } else if (clampedValue < warningThreshold) {
    icon = glyphs.progress50;
    colorFn = getColor('warning');
  } else {
    icon = glyphs.progress75;
    colorFn = getColor('warning');
  }

  const secondaryColor = colorOverride ?? getColor('secondary');

  return (
    <Box flexDirection="row" gap={1}>
      {allowIcons && <Text>{(colorOverride ?? colorFn)(icon)}</Text>}
      {showPercentage && (
        <Text>
          {(colorOverride ?? colorFn)(`${Math.round(clampedValue)}%`)}
        </Text>
      )}
      {label && <Text>{secondaryColor(label)}</Text>}
    </Box>
  );
}

export { ProgressChip };
