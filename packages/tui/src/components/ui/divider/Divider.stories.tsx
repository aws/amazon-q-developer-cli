import React from 'react';
import { Divider } from './Divider.js';
import type { DividerProps } from './Divider.js';

const meta = {
  component: Divider,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Default', 'CustomColor', 'AccentColor'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Default = {
  parameters: {
    docs: {
      storyDescription: 'Default horizontal divider using Box border',
    },
  },
  args: {} as DividerProps,
};

export const CustomColor = {
  parameters: {
    docs: {
      storyDescription: 'Divider with custom theme color',
    },
  },
  args: {
    color: 'primary',
  } as DividerProps,
};

export const AccentColor = {
  parameters: {
    docs: {
      storyDescription: 'Divider with accent theme color',
    },
  },
  args: {
    color: 'accent',
  } as DividerProps,
};
