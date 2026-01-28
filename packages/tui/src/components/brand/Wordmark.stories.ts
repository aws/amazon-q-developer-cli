import React from 'react';
import Wordmark from './Wordmark.js';

const meta = {
  component: Wordmark,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Static', 'Animated'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Static = {
  parameters: {
    docs: {
      storyDescription: 'Static Kiro wordmark without animation',
    },
  },
  args: {
    animate: false,
  },
};

export const Animated = {
  parameters: {
    docs: {
      storyDescription: 'Animated Kiro wordmark with typing effect',
    },
  },
  args: {
    animate: true,
  },
};
