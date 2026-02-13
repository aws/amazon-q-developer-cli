import React, { useState } from 'react';
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
  selectedValue: initialSelectedValue,
  onChange,
  disabled = false,
  direction = 'vertical',
  label,
}: RadioGroupProps) {
  const [selectedValue, setSelectedValue] = useState(
    initialSelectedValue || options[0]?.value
  );

  // Get text styling for the label
  const labelStyle = useTextStyle('label');

  const handleSelect = (optionValue: string) => {
    if (disabled) return;

    // Update internal selected state
    setSelectedValue(optionValue);

    // Notify parent if onChange is provided
    if (onChange) {
      const selectedOption = options.find(
        (option) => option.value === optionValue
      );
      const selectedLabel = selectedOption?.label || optionValue;
      onChange(optionValue, selectedLabel);
    }
  };

  // Keyboard navigation - selection follows focus
  useKeypress((_input, key) => {
    if (disabled || options.length === 0) return;

    const enabledOptions = options.filter((option) => !option.disabled);
    if (enabledOptions.length === 0) return;

    // Find current position in enabled options based on selected value
    const currentEnabledIndex = enabledOptions.findIndex(
      (option) => option.value === selectedValue
    );

    let targetOption: RadioOption | undefined;

    if (
      (key.upArrow && direction === 'vertical') ||
      (key.leftArrow && direction === 'horizontal')
    ) {
      // Move to previous enabled option
      const prevEnabledIndex =
        currentEnabledIndex > 0
          ? currentEnabledIndex - 1
          : enabledOptions.length - 1;
      targetOption = enabledOptions[prevEnabledIndex];
    } else if (
      (key.downArrow && direction === 'vertical') ||
      (key.rightArrow && direction === 'horizontal')
    ) {
      // Move to next enabled option
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

  // Handle empty or undefined options
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
              selected={selectedValue === option.value}
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
