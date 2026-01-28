import React from 'react';
import { Divider, DividerProps } from './Divider.js';

const meta = {
  component: Divider,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Default',
      'CustomCharacter',
      'CustomColor',
      'CustomWidth',
      'Thick',
      'Dotted',
      'Double',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Default = {
  parameters: {
    docs: {
      storyDescription: 'Default horizontal divider with standard character',
    },
  },
  args: {} as DividerProps,
};

export const CustomCharacter = {
  parameters: {
    docs: {
      storyDescription: 'Divider with custom character',
    },
  },
  args: {
    character: '═',
  } as DividerProps,
};

export const CustomColor = {
  parameters: {
    docs: {
      storyDescription: 'Divider with custom theme color',
    },
  },
  args: {
    color: 'accent',
  } as DividerProps,
};

export const CustomWidth = {
  parameters: {
    docs: {
      storyDescription: 'Divider with custom width',
    },
  },
  args: {
    width: 40,
  } as DividerProps,
};

export const Thick = {
  parameters: {
    docs: {
      storyDescription: 'Thick divider using heavy line character',
    },
  },
  args: {
    character: '━',
    color: 'primary',
  } as DividerProps,
};

export const Dotted = {
  parameters: {
    docs: {
      storyDescription: 'Dotted divider using dot character',
    },
  },
  args: {
    character: '·',
    color: 'muted',
  } as DividerProps,
};

export const Double = {
  parameters: {
    docs: {
      storyDescription: 'Double line divider',
    },
  },
  args: {
    character: '═',
    color: 'secondary',
    width: 60,
  } as DividerProps,
};
