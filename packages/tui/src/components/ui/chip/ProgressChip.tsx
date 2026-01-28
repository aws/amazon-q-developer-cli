import React from 'react';
import { Box } from 'ink';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ProgressChipProps {
  /** Progress value from 0-100 */
  value: number;
  /** Optional text label to show after the percentage */
  label?: string;
  /** Whether to show percentage text after the bar */
  showPercentage?: boolean;
  /** Width of the progress bar in characters (default: 5) */
  barWidth?: number;
  /** Color for the filled portion of the bar (default: 'primary') */
  barColor?: 'primary' | 'success' | 'warning' | 'error' | 'info' | 'brand';
  /** Show remaining percentage (100 - value) instead of value */
  showRemaining?: boolean;
}

export default function ProgressChip({
  value,
  label,
  showPercentage = true,
  barWidth = 5,
  barColor = 'primary',
  showRemaining = false,
}: ProgressChipProps) {
  const { getColor } = useTheme();

  // Clamp value between 0 and 100
  const clampedValue = Math.max(0, Math.min(100, value));

  // Calculate filled and empty segments
  const filledCount = Math.round((clampedValue / 100) * barWidth);
  const emptyCount = barWidth - filledCount;

  const barColorFn = getColor(barColor);
  const primaryColor = getColor('primary');
  const secondaryColor = getColor('secondary');
  const mutedColor = getColor('muted');

  // Build the progress bar
  const filledBar = '█'.repeat(filledCount);
  const emptyBar = '░'.repeat(emptyCount);

  // Calculate display percentage
  const displayPercentage = showRemaining ? 100 - clampedValue : clampedValue;

  return (
    <Box flexDirection="row" gap={1}>
      <Text>
        {mutedColor('[')}
        {barColorFn(filledBar)}
        {secondaryColor(emptyBar)}
        {mutedColor(']')}
      </Text>
      {showPercentage && (
        <Text>{primaryColor(`${Math.round(displayPercentage)}%`)}</Text>
      )}
      {label && <Text>{secondaryColor(label)}</Text>}
    </Box>
  );
}

export { ProgressChip };
