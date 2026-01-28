import React from 'react';
import Chip, { ChipColor } from './Chip.js';

const meta = {
  component: Chip,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Basic',
      'WithBackground',
      'WithPrefix',
      'WithWrap',
      'GitStyle',
      'PathPrefix',
      'Primary',
      'Secondary',
      'Brand',
      'Success',
      'Warning',
      'Error',
      'WithPathShortening',
      'ComplexExample',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Basic examples
export const Basic = {
  parameters: {
    docs: {
      storyDescription: 'Basic chip with just a value',
    },
  },
  args: {
    value: 'claude-3.5-sonnet',
    color: ChipColor.PRIMARY,
  },
};

export const WithBackground = {
  parameters: {
    docs: {
      storyDescription: 'Chip with muted background color',
    },
  },
  args: {
    value: 'with background',
    color: ChipColor.BRAND,
    background: true,
  },
};

export const WithPrefix = {
  parameters: {
    docs: {
      storyDescription: 'Chip with prefix text',
    },
  },
  args: {
    value: 'connected',
    color: ChipColor.SUCCESS,
    prefix: 'status: ',
  },
};

export const WithWrap = {
  parameters: {
    docs: {
      storyDescription: 'Chip with value wrapped in parentheses',
    },
  },
  args: {
    value: 'main',
    color: ChipColor.PRIMARY,
    wrap: true,
  },
};

export const GitStyle = {
  parameters: {
    docs: {
      storyDescription: 'Git-style chip with prefix and wrap (git:(branch))',
    },
  },
  args: {
    value: 'feature/new-component',
    color: ChipColor.PRIMARY,
    prefix: 'git:',
    wrap: true,
  },
};

export const PathPrefix = {
  parameters: {
    docs: {
      storyDescription: 'Path-style chip with tilde prefix',
    },
  },
  args: {
    value: 'developer/project',
    color: ChipColor.BRAND,
    prefix: '~/',
  },
};

// Color examples
export const Primary = {
  parameters: {
    docs: {
      storyDescription: 'Chip with primary color (default)',
    },
  },
  args: {
    value: 'primary-value',
    color: ChipColor.PRIMARY,
  },
};

export const Secondary = {
  parameters: {
    docs: {
      storyDescription: 'Chip with secondary color',
    },
  },
  args: {
    value: 'secondary-value',
    color: ChipColor.SECONDARY,
  },
};

export const Brand = {
  parameters: {
    docs: {
      storyDescription: 'Chip with brand color',
    },
  },
  args: {
    value: 'brand-value',
    color: ChipColor.BRAND,
  },
};

export const Success = {
  parameters: {
    docs: {
      storyDescription: 'Chip with success color',
    },
  },
  args: {
    value: 'success-value',
    color: ChipColor.SUCCESS,
  },
};

export const Warning = {
  parameters: {
    docs: {
      storyDescription: 'Chip with warning color',
    },
  },
  args: {
    value: 'warning-value',
    color: ChipColor.WARNING,
  },
};

export const Error = {
  parameters: {
    docs: {
      storyDescription: 'Chip with error color',
    },
  },
  args: {
    value: 'error-value',
    color: ChipColor.ERROR,
  },
};

// Feature examples
export const WithPathShortening = {
  parameters: {
    docs: {
      storyDescription: 'Chip automatically shortens paths (replaces home directory with ~)',
    },
  },
  args: {
    value: '/Users/developer/projects/my-app',
    color: ChipColor.BRAND,
  },
};

export const ComplexExample = {
  parameters: {
    docs: {
      storyDescription: 'Complex example with prefix, wrap, and custom color',
    },
  },
  args: {
    value: 'running',
    color: ChipColor.WARNING,
    prefix: '⟳ ',
    wrap: true,
  },
};
