import { ImageRead } from './ImageRead.js';

const meta = {
  component: ImageRead,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'SingleImage',
      'SingleImageLoading',
      'MultipleImages',
      'Standalone',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Single image — finished
export const SingleImage = {
  args: {
    content: JSON.stringify({ paths: ['src/assets/logo.png'] }),
    isFinished: true,
    status: 'success',
  },
};

// Single image — loading
export const SingleImageLoading = {
  args: {
    content: JSON.stringify({ paths: ['src/assets/logo.png'] }),
    isFinished: false,
  },
};

// Multiple images
export const MultipleImages = {
  args: {
    content: JSON.stringify({
      paths: [
        'src/assets/logo.png',
        'docs/screenshot.jpg',
        'images/banner.webp',
      ],
    }),
    isFinished: true,
    status: 'success',
  },
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    content: JSON.stringify({ paths: ['design/mockup.png'] }),
    noStatusBar: true,
    isFinished: true,
  },
};
