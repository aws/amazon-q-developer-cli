import React from 'react';
import { Box } from './../../../renderer.js';
import { FileList } from './FileList.js';
import { Card } from '../../ui/card/Card.js';

const meta = {
  component: FileList,
  parameters: { layout: 'fullscreen' },
  tags: ['autodocs'],
};

export default meta;

const fewFiles = ['Button.tsx', 'Card.tsx', 'Modal.tsx'];
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

export const Variants = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <FileList items={fewFiles} previewCount={5} expanded={false} />
      </Card>
      <Card active={true}>
        <FileList
          items={manyFiles}
          previewCount={5}
          expanded={false}
          expandHint="...+7 files (ctrl+o to toggle)"
          hiddenCount={7}
        />
      </Card>
      <Card active={true}>
        <FileList items={manyFiles} previewCount={5} expanded={true} />
      </Card>
      <Card active={true}>
        <FileList
          items={manyFiles}
          previewCount={5}
          expanded={false}
          hiddenCount={7}
        />
      </Card>
    </Box>
  ),
};
