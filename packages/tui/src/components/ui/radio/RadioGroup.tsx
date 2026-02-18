import React from 'react';
import { Box } from 'ink';
import { RadioButton } from './RadioButton.js';
import { useTextStyle } from '../../../hooks/useTextStyle.js';
import { Text } from '../text/Text.js';
import { useKeypress } from '../../../hooks/useKeypress.js';

export interface RadioOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface RadioGroupProps {
  options: RadioOption[];
  selectedValue?: string;
  onChange?: (value: string, selectedLabel: string) => void;
  disabled?: boolean;
  direction?: 'vertical' | 'horizontal';
  label?: string;
}

export const RadioGroup = React.memo(function RadioGroup({
  options = [],
  selectedValue,
  onChange,
  disabled = false,
  direction = 'vertical',
  label,
}: RadioGroupProps) {
  // Use prop directly — parent controls the selected value
  const currentValue = selectedValue ?? options[0]?.value;

  // Get text styling for the label
  const labelStyle = useTextStyle('label');

  const handleSelect = (optionValue: string) => {
    if (disabled) return;
    if (onChange) {
      const selectedOption = options.find(
        (option) => option.value === optionValue
      );
      onChange(optionValue, selectedOption?.label || optionValue);
    }
  };

  // Keyboard navigation - selection follows focus
  useKeypress((_input, key) => {
    if (disabled || options.length === 0) return;

    const enabledOptions = options.filter((option) => !option.disabled);
    if (enabledOptions.length === 0) return;

    const currentEnabledIndex = enabledOptions.findIndex(
      (option) => option.value === currentValue
    );

    let targetOption: RadioOption | undefined;

    if (
      (key.upArrow && direction === 'vertical') ||
      (key.leftArrow && direction === 'horizontal')
    ) {
      const prevEnabledIndex =
        currentEnabledIndex > 0
          ? currentEnabledIndex - 1
          : enabledOptions.length - 1;
      targetOption = enabledOptions[prevEnabledIndex];
    } else if (
      (key.downArrow && direction === 'vertical') ||
      (key.rightArrow && direction === 'horizontal')
    ) {
      const nextEnabledIndex =
        currentEnabledIndex < enabledOptions.length - 1
          ? currentEnabledIndex + 1
          : 0;
      targetOption = enabledOptions[nextEnabledIndex];
    }

    if (targetOption) {
      handleSelect(targetOption.value);
    }
  });

  if (!options || options.length === 0) {
    return null;
  }

  return (
    <Box flexDirection="column">
      {label && (
        <Box marginBottom={1}>
          <Text>{labelStyle(label)}</Text>
        </Box>
      )}
      <Box flexDirection={direction === 'vertical' ? 'column' : 'row'}>
        {options.map((option) => (
          <Box
            key={option.value}
            marginRight={direction === 'horizontal' ? 2 : 0}
          >
            <RadioButton
              selected={currentValue === option.value}
              label={option.label}
              disabled={disabled || option.disabled}
              onSelect={() => handleSelect(option.value)}
            />
          </Box>
        ))}
      </Box>
    </Box>
  );
});
