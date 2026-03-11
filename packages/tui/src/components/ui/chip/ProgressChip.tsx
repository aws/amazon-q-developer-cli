import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ProgressChipProps {
  /** Progress value from 0-100 */
  value: number;
  /** Optional text label to show after the percentage */
  label?: string;
  /** Whether to show percentage text after the icon */
  showPercentage?: boolean;
  /** Threshold percentage where color changes from green to yellow (default: 60) */
  warningThreshold?: number;
}

export default function ProgressChip({
  value,
  label,
  showPercentage = true,
  warningThreshold = 60,
}: ProgressChipProps) {
  const { getColor } = useTheme();

  // Clamp value between 0 and 100
  const clampedValue = Math.max(0, Math.min(100, value));

  // Determine icon and color based on value
  let icon: string;
  let colorFn: (text: string) => string;

  if (clampedValue === 0) {
    icon = '◷';
    colorFn = getColor('success');
  } else if (clampedValue <= 25) {
    icon = '◔';
    colorFn = getColor('success');
  } else if (clampedValue <= 50) {
    icon = '◑';
    colorFn = getColor('success');
  } else if (clampedValue < warningThreshold) {
    icon = '◑';
    colorFn = getColor('warning');
  } else {
    icon = '◕';
    colorFn = getColor('warning');
  }

  const secondaryColor = getColor('secondary');

  return (
    <Box flexDirection="row" gap={1}>
      <Text>{colorFn(icon)}</Text>
      {showPercentage && <Text>{colorFn(`${Math.round(clampedValue)}%`)}</Text>}
      {label && <Text>{secondaryColor(label)}</Text>}
    </Box>
  );
}

export { ProgressChip };
