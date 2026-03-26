import React from 'react';
import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface TurnUsageSummaryProps {
  text: string;
}

export const TurnUsageSummary = React.memo(function TurnUsageSummary({
  text,
}: TurnUsageSummaryProps) {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  return (
    <StatusBar status="usage">
      <Text>{dim(text)}</Text>
    </StatusBar>
  );
});
