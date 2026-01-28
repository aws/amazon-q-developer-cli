import React from 'react';
import { Text } from './Text.js';

const meta = {
  component: Text,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const Default = {
  args: {
    children: 'Default text (no styling)',
  },
};
