import React from 'react';
import { Box } from './../../../renderer.js';
import { FileList } from './FileList.js';
import { Text } from '../../ui/text/Text.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 100, rows: 12 };
const manyFiles = [
  'Button.tsx',
  'Card.tsx',
  'Modal.tsx',
  'Sidebar.tsx',
  'Header.tsx',
  'Footer.tsx',
  'Nav.tsx',
  'Table.tsx',
  'Form.tsx',
  'Input.tsx',
  'Select.tsx',
  'Checkbox.tsx',
];

const meta = {
  component: FileList,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      short: { label: 'Short list' },
      collapsed: { label: 'Collapsed list with expansion hint' },
      expanded: { label: 'Expanded list rendering' },
      static: { label: 'Static list with hidden-item suffix' },
      wrapped: { label: 'Items wrap at narrow terminal width' },
      clipped: { label: 'Long item names respect the character cap' },
      empty: { label: 'Empty list renders no rows' },
      'toggle-interaction': {
        label: 'Ctrl+O changes collapsed output to expanded output',
        gapType: 'integration-only',
        description:
          'The global tool-output controller owns the keyboard transition.',
      },
    },
    storyOrder: [
      'Short',
      'Collapsed',
      'Expanded',
      'StaticSuffix',
      'WrappedNarrow',
      'CharacterClipped',
      'Empty',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Short = {
  args: {
    items: ['Button.tsx', 'Card.tsx', 'Modal.tsx'],
    previewCount: 5,
    expanded: false,
  },
  parameters: certifyVisualStory(
    'Button.tsx',
    {
      visible: ['Button.tsx, Card.tsx, Modal.tsx'],
      hidden: ['ctrl+o', '+1 more'],
    },
    ['short'],
    viewport
  ),
};

export const Collapsed = {
  args: {
    items: manyFiles,
    previewCount: 5,
    expanded: false,
    expandHint: '...+7 files (ctrl+o to toggle)',
    hiddenCount: 7,
  },
  parameters: certifyVisualStory(
    'Button.tsx',
    {
      visible: [
        'Button.tsx, Card.tsx, Modal.tsx, Sidebar.tsx, Header.tsx',
        '...+7 files (ctrl+o to toggle)',
      ],
      hidden: ['Footer.tsx', 'Checkbox.tsx'],
    },
    ['collapsed'],
    viewport
  ),
};

export const Expanded = {
  args: {
    items: manyFiles,
    previewCount: 5,
    expanded: true,
  },
  parameters: certifyVisualStory(
    'Checkbox.tsx',
    {
      visible: ['Button.tsx', 'Footer.tsx', 'Checkbox.tsx'],
      hidden: ['ctrl+o', '+7 files'],
      ordered: ['Button.tsx', 'Footer.tsx', 'Checkbox.tsx'],
    },
    ['expanded'],
    viewport
  ),
};

export const StaticSuffix = {
  args: {
    items: manyFiles,
    previewCount: 5,
    expanded: false,
    hiddenCount: 7,
  },
  parameters: certifyVisualStory(
    '+7 more',
    {
      visible: ['Header.tsx +7 more'],
      hidden: ['Footer.tsx', 'Checkbox.tsx', 'ctrl+o'],
    },
    ['static'],
    viewport
  ),
};

export const WrappedNarrow = {
  args: {
    items: ['AlphaComponent.tsx', 'BetaComponent.tsx', 'GammaComponent.tsx'],
    previewCount: 5,
    expanded: false,
  },
  parameters: certifyVisualStory(
    'AlphaComponent.tsx',
    {
      visible: [
        'AlphaComponent.tsx',
        'BetaComponent.tsx',
        'GammaComponent.tsx',
      ],
      ordered: [
        'AlphaComponent.tsx',
        'BetaComponent.tsx',
        'GammaComponent.tsx',
      ],
    },
    ['wrapped'],
    { columns: 36, rows: 8 }
  ),
};

export const CharacterClipped = {
  args: {
    items: ['VeryLongComponentName.tsx', 'Short.tsx'],
    previewCount: 5,
    expanded: false,
    maxChars: 8,
  },
  parameters: certifyVisualStory(
    'VeryLon…',
    {
      visible: ['VeryLon…, Short.t…'],
      hidden: ['VeryLongComponentName.tsx', 'Short.tsx'],
    },
    ['clipped'],
    viewport
  ),
};

export const Empty = {
  render: () => (
    <Box flexDirection="column">
      <Text>Empty file list</Text>
      <FileList items={[]} previewCount={5} expanded={false} />
    </Box>
  ),
  parameters: certifyVisualStory(
    'Empty file list',
    {
      visible: ['Empty file list'],
      hidden: ['Button.tsx', 'ctrl+o', '+1 more'],
    },
    ['empty'],
    viewport
  ),
};
