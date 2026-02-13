import { RadioGroup, type RadioGroupProps } from './RadioGroup.js';

const meta = {
  component: RadioGroup,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'BasicVertical',
      'BasicHorizontal',
      'WithDisabledOptions',
      'LargeList',
    ],
  },
  tags: ['autodocs'],
  argTypes: {
    onChange: { action: 'changed' },
  },
};

export default meta;

export const BasicVertical = {
  parameters: {
    docs: {
      storyDescription:
        'Basic vertical radio group with label (use ↑/↓ arrows to navigate and select)',
    },
  },
  args: {
    label: 'Choose an option:',
    options: [
      { value: 'option1', label: 'First option' },
      { value: 'option2', label: 'Second option' },
      { value: 'option3', label: 'Third option' },
    ],
    selectedValue: 'option2',
    direction: 'vertical',
  } as RadioGroupProps,
};

export const BasicHorizontal = {
  parameters: {
    docs: {
      storyDescription:
        'Basic horizontal radio group with label (use ←/→ arrows to navigate and select)',
    },
  },
  args: {
    label: 'Select size:',
    options: [
      { value: 'small', label: 'Small' },
      { value: 'medium', label: 'Medium' },
      { value: 'large', label: 'Large' },
    ],
    selectedValue: 'medium',
    direction: 'horizontal',
  } as RadioGroupProps,
};

export const WithDisabledOptions = {
  parameters: {
    docs: {
      storyDescription:
        'Radio group with some disabled options (skips disabled options during navigation)',
    },
  },
  args: {
    label: 'Select availability:',
    options: [
      { value: 'available', label: 'Available option' },
      { value: 'disabled1', label: 'Disabled option', disabled: true },
      { value: 'another', label: 'Another available' },
      { value: 'disabled2', label: 'Also disabled', disabled: true },
      { value: 'final', label: 'Final option' },
    ],
    selectedValue: 'available',
  } as RadioGroupProps,
};

export const LargeList = {
  parameters: {
    docs: {
      storyDescription:
        'Radio group with many options to test navigation behavior',
    },
  },
  args: {
    options: Array.from({ length: 15 }, (_, i) => ({
      value: `item${i + 1}`,
      label: `Option ${i + 1}`,
      disabled: i === 3 || i === 7 || i === 11, // Disable some options
    })),
    selectedValue: 'item5',
  } as RadioGroupProps,
};
