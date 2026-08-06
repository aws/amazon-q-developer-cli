import React from 'react';
import { Text } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export const PromptChip = React.memo(function PromptChip({
  label,
}: {
  label: string;
}) {
  const { getColor } = useTheme();
  const text = getColor('components.promptChip.text');
  const background = getColor('components.promptChip.background', 'bg');

  return <Text>{background(text(` ${label} `))}</Text>;
});
