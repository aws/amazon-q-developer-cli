import React from 'react';
import { PastedChip, PastedChipProps } from './PastedChip.js';

const meta = {
  component: PastedChip,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['TextFewLines', 'TextManyLines', 'TextFewChars', 'Image', 'ImageWithDetails'],
  },
  tags: ['autodocs'],
};

export default meta;

export const TextFewLines = {
  parameters: {
    docs: {
      storyDescription: 'Pasted text with few lines',
    },
  },
  args: {
    type: 'text',
    lineCount: 3,
    charCount: 150,
  } as PastedChipProps,
};

export const TextManyLines = {
  parameters: {
    docs: {
      storyDescription: 'Pasted text with many lines',
    },
  },
  args: {
    type: 'text',
    lineCount: 25,
    charCount: 1200,
  } as PastedChipProps,
};

export const TextFewChars = {
  parameters: {
    docs: {
      storyDescription: 'Pasted text with single line and few characters',
    },
  },
  args: {
    type: 'text',
    lineCount: 1,
    charCount: 42,
  } as PastedChipProps,
};

export const Image = {
  parameters: {
    docs: {
      storyDescription: 'Pasted image without details',
    },
  },
  args: {
    type: 'image',
  } as PastedChipProps,
};

export const ImageWithDetails = {
  parameters: {
    docs: {
      storyDescription: 'Pasted image with dimensions and size',
    },
  },
  args: {
    type: 'image',
    imageWidth: 1920,
    imageHeight: 1080,
    imageSizeBytes: 2457600,
  } as PastedChipProps,
};
