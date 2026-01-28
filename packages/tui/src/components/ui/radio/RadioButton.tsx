import React from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTextStyle } from '../../../hooks/useTextStyle.js';
import { Text } from '../text/Text.js';

export interface RadioButtonProps {
  selected: boolean;
  label: string;
  disabled?: boolean;
  onSelect?: () => void;
}

export const RadioButton = React.memo(function RadioButton({
  selected,
  label: labelText,
  disabled = false,
  onSelect,
}: RadioButtonProps) {
  const { getColor } = useTheme();

  // Get text styling functions
  const label = useTextStyle('label');

  // Characters for radio button states
  const selectedChar = '●'; // Filled circle
  const unselectedChar = '○'; // Empty circle

  const radioChar = selected ? selectedChar : unselectedChar;

  // Radio dot color function using chalk approach
  const radioColorFn = disabled
    ? getColor('muted')
    : selected
      ? getColor('accent')
      : getColor('secondary');

  return (
    <Text>
      {radioColorFn(radioChar)}{' '}
      {disabled
        ? getColor('muted')(labelText)
        : selected
          ? label(labelText)
          : getColor('primary')(labelText)}
    </Text>
  );
});
