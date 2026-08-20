import { ImageRead } from './ImageRead.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 100, rows: 12 };

const meta = {
  component: ImageRead,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      single: { label: 'Single image target' },
      loading: { label: 'Image read in progress without completion marker' },
      'completed-indicator': { label: 'Image read completed indicator' },
      multiple: { label: 'Multiple image targets' },
      standalone: { label: 'Image read embedded without status chrome' },
      'windows-path': { label: 'Windows image path displays its basename' },
      error: {
        label: 'Image read error',
        gapType: 'integration-only',
        description:
          'ToolUseMessage routes image failures through the shared error renderer.',
      },
    },
    storyOrder: [
      'SingleImage',
      'WindowsImage',
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
  parameters: certifyVisualStory(
    'logo.png',
    {
      visible: ['● ImageRead logo.png'],
      hidden: ['output:', 'screenshot.jpg'],
    },
    ['single', 'completed-indicator'],
    viewport
  ),
};

export const WindowsImage = {
  args: {
    content: JSON.stringify({
      paths: ['C:\\workspace\\assets\\diagram.png'],
    }),
    isFinished: true,
    status: 'success',
  },
  parameters: certifyVisualStory(
    'diagram.png',
    {
      visible: ['ImageRead diagram.png'],
      hidden: ['C:\\workspace\\assets\\diagram.png'],
    },
    ['single', 'windows-path'],
    viewport
  ),
};

// Single image — loading
export const SingleImageLoading = {
  args: {
    content: JSON.stringify({ paths: ['src/assets/logo.png'] }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'logo.png',
    {
      visible: ['ImageRead logo.png'],
      hidden: ['● ImageRead', 'output:', 'screenshot.jpg'],
    },
    ['single', 'loading'],
    viewport
  ),
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
  parameters: certifyVisualStory(
    '3 images',
    {
      visible: ['ImageRead (3 images)'],
      hidden: ['logo.png', 'screenshot.jpg', 'banner.webp', 'output:'],
    },
    ['multiple'],
    viewport
  ),
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    content: JSON.stringify({ paths: ['design/mockup.png'] }),
    noStatusBar: true,
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'mockup.png',
    {
      visible: ['ImageRead mockup.png'],
      hidden: ['output:'],
    },
    ['single', 'standalone'],
    viewport
  ),
};
