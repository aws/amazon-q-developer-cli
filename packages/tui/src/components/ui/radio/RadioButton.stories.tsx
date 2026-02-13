import React, { useState } from 'react';
import { RadioButton, type RadioButtonProps } from './RadioButton.js';
import { RadioGroup, type RadioGroupProps } from './RadioGroup.js';
import { Box } from 'ink';

const meta = {
  component: RadioButton,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Selected', 'Unselected', 'Disabled', 'Interactive'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Selected = {
  parameters: {
    docs: {
      storyDescription: 'Radio button in selected state with filled circle',
    },
  },
  args: {
    selected: true,
    label: 'Selected option',
  } as RadioButtonProps,
};

export const Unselected = {
  parameters: {
    docs: {
      storyDescription: 'Radio button in unselected state with empty circle',
    },
  },
  args: {
    selected: false,
    label: 'Unselected option',
  } as RadioButtonProps,
};

export const Disabled = {
  parameters: {
    docs: {
      storyDescription: 'Disabled radio button with muted colors',
    },
  },
  args: {
    selected: false,
    label: 'Disabled option',
    disabled: true,
  } as RadioButtonProps,
};

export const Interactive = {
  parameters: {
    docs: {
      storyDescription: 'Interactive radio button that can be toggled',
    },
  },
  render: () => {
    const [selected, setSelected] = useState(false);

    return (
      <RadioButton
        selected={selected}
        label="Click to toggle"
        onSelect={() => setSelected(!selected)}
      />
    );
  },
};

// RadioGroup stories
export const RadioGroupVertical = {
  parameters: {
    docs: {
      storyDescription: 'Vertical radio group with multiple options',
    },
  },
  render: () => {
    const [value, setValue] = useState('option1');

    return (
      <RadioGroup
        options={[
          { value: 'option1', label: 'First option' },
          { value: 'option2', label: 'Second option' },
          { value: 'option3', label: 'Third option' },
        ]}
        selectedValue={value}
        onChange={(newValue) => setValue(newValue)}
        direction="vertical"
      />
    );
  },
};

export const RadioGroupHorizontal = {
  parameters: {
    docs: {
      storyDescription: 'Horizontal radio group with multiple options',
    },
  },
  render: () => {
    const [value, setValue] = useState('small');

    return (
      <RadioGroup
        options={[
          { value: 'small', label: 'Small' },
          { value: 'medium', label: 'Medium' },
          { value: 'large', label: 'Large' },
        ]}
        selectedValue={value}
        onChange={(newValue) => setValue(newValue)}
        direction="horizontal"
      />
    );
  },
};

export const RadioGroupWithDisabled = {
  parameters: {
    docs: {
      storyDescription: 'Radio group with some disabled options',
    },
  },
  render: () => {
    const [value, setValue] = useState('available');

    return (
      <RadioGroup
        options={[
          { value: 'available', label: 'Available option' },
          { value: 'disabled1', label: 'Disabled option', disabled: true },
          { value: 'another', label: 'Another available' },
          { value: 'disabled2', label: 'Also disabled', disabled: true },
        ]}
        selectedValue={value}
        onChange={(newValue) => setValue(newValue)}
      />
    );
  },
};
