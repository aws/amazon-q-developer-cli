import React from 'react';
import { SnackBar, SnackBarProps, Action } from './SnackBar.js';

const meta = {
  component: SnackBar,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Basic', 'WithDescriptions', 'SlideIn', 'ManyActions', 'LongTitle'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Basic = {
  parameters: {
    docs: {
      storyDescription: 'Basic action bar with simple yes/no actions',
    },
  },
  args: {
    title: 'Apply Changes',
    actions: [
      { key: 'y', label: 'yes' },
      { key: 'n', label: 'no' },
    ],
  } as SnackBarProps,
};

export const WithDescriptions = {
  parameters: {
    docs: {
      storyDescription: 'Action bar with actions that include descriptions',
    },
  },
  args: {
    title: 'Apply Changes',
    actions: [
      { key: 'y', label: 'yes' },
      { key: 'n', label: 'no' },
      { key: 't', label: 'trust', description: 'all future code changes' },
    ],
  } as SnackBarProps,
};

export const SlideIn = {
  parameters: {
    docs: {
      storyDescription: 'Action bar with slide-in animation from top',
    },
  },
  args: {
    title: 'Apply Changes',
    actions: [
      { key: 'y', label: 'yes' },
      { key: 'n', label: 'no' },
      { key: 't', label: 'trust', description: 'all future code changes' },
    ],
    slideIn: true,
  } as SnackBarProps,
};

export const ManyActions = {
  parameters: {
    docs: {
      storyDescription: 'Action bar with multiple actions and mixed descriptions',
    },
  },
  args: {
    title: 'Code Review',
    actions: [
      { key: 'a', label: 'approve' },
      { key: 'r', label: 'reject' },
      { key: 'c', label: 'comment' },
      { key: 's', label: 'suggest', description: 'changes' },
      { key: 'd', label: 'defer', description: 'for later review' },
    ],
  } as SnackBarProps,
};

export const LongTitle = {
  parameters: {
    docs: {
      storyDescription: 'Action bar with a longer title to test layout',
    },
  },
  args: {
    title: 'Confirm Dangerous Operation',
    actions: [
      { key: 'y', label: 'yes', description: 'I understand the risks' },
      { key: 'n', label: 'no' },
      { key: 'b', label: 'backup', description: 'first then proceed' },
    ],
  } as SnackBarProps,
};

export const KeyNotInLabel = {
  parameters: {
    docs: {
      storyDescription: 'Action bar where key characters are not found in labels',
    },
  },
  args: {
    title: 'File Operations',
    actions: [
      { key: 'x', label: 'delete' },
      { key: 'z', label: 'move' },
      { key: 'q', label: 'backup', description: 'before operation' },
    ],
  } as SnackBarProps,
};
