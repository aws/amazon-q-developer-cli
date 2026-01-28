import React from 'react';
import { ProgressChip } from './ProgressChip.js';

const meta = {
  component: ProgressChip,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Default',
      'Empty',
      'Quarter',
      'Half',
      'ThreeQuarters',
      'Complete',
      'WithLabel',
      'NoPercentage',
      'CustomWidth',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Progress percentage examples
export const Default = {
  parameters: {
    docs: {
      storyDescription: 'Default progress chip at 75% with primary color',
    },
  },
  args: {
    value: 75,
    label: 'context used',
  },
};

export const Empty = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip at 0% used (100% remaining)',
    },
  },
  args: {
    value: 0,
    barColor: 'success',
    label: 'context remaining',
    showRemaining: true,
  },
};

export const Quarter = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip at 25% used (75% remaining)',
    },
  },
  args: {
    value: 25,
    barColor: 'success',
    label: 'context remaining',
    showRemaining: true,
  },
};

export const Half = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip at 50% used (50% remaining)',
    },
  },
  args: {
    value: 50,
    barColor: 'success',
    label: 'context remaining',
    showRemaining: true,
  },
};

export const ThreeQuarters = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip at 75% used (25% remaining)',
    },
  },
  args: {
    value: 75,
    barColor: 'success',
    label: 'context remaining',
    showRemaining: true,
  },
};

export const Complete = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip at 100% used (0% remaining)',
    },
  },
  args: {
    value: 100,
    barColor: 'success',
    label: 'context remaining',
    showRemaining: true,
  },
};

export const WithLabel = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip with a text label',
    },
  },
  args: {
    value: 45,
    label: 'context remaining',
  },
};

export const NoPercentage = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip without percentage display',
    },
  },
  args: {
    value: 65,
    showPercentage: false,
    label: 'context remaining',
  },
};

export const CustomWidth = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip with custom bar width',
    },
  },
  args: {
    value: 70,
    barWidth: 20,
    label: 'context remaining',
  },
};
