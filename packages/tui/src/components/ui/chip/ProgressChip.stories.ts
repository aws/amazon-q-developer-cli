import React from 'react';
import { ProgressChip } from './ProgressChip.js';

const meta = {
  component: ProgressChip,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Zero',
      'Low',
      'Quarter',
      'Half',
      'AboveThreshold',
      'High',
      'WithLabel',
      'NoPercentage',
      'CustomThreshold',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Zero = {
  parameters: {
    docs: {
      storyDescription: 'Progress at 0% - shows ◷ in green',
    },
  },
  args: {
    value: 0,
  },
};

export const Low = {
  parameters: {
    docs: {
      storyDescription: 'Progress at 10% - shows ◔ in green',
    },
  },
  args: {
    value: 10,
  },
};

export const Quarter = {
  parameters: {
    docs: {
      storyDescription: 'Progress at 25% - shows ◔ in green',
    },
  },
  args: {
    value: 25,
  },
};

export const Half = {
  parameters: {
    docs: {
      storyDescription: 'Progress at 50% - shows ◑ in green',
    },
  },
  args: {
    value: 50,
  },
};

export const AboveThreshold = {
  parameters: {
    docs: {
      storyDescription:
        'Progress at 55% - shows ◑ in yellow (above default 60% threshold)',
    },
  },
  args: {
    value: 55,
  },
};

export const High = {
  parameters: {
    docs: {
      storyDescription: 'Progress at 75% - shows ◕ in yellow',
    },
  },
  args: {
    value: 75,
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
    label: 'context used',
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
    label: 'context used',
  },
};

export const CustomThreshold = {
  parameters: {
    docs: {
      storyDescription: 'Progress chip with custom warning threshold at 80%',
    },
  },
  args: {
    value: 70,
    warningThreshold: 80,
    label: 'context used',
  },
};
