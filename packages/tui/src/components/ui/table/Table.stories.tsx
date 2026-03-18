import React from 'react';
import { Table } from './Table.js';
import type { TableProps } from './Table.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

const meta = {
  component: Table,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Basic', 'NoHeaders', 'WithColors', 'Empty'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Basic = {
  render: () => {
    const { getColor } = useTheme();
    const primary = getColor('primary');
    const dim = getColor('secondary');
    const brand = getColor('brand');

    const props: TableProps = {
      columns: [
        { label: 'Name', width: 20 },
        { label: 'Status', width: 12 },
        { label: 'Description' },
      ],
      rows: [
        [
          { text: 'my-project', color: primary },
          { text: '3 items', color: dim },
          { text: '/Users/dev/my-project', color: dim },
        ],
        [
          { text: 'docs', color: primary },
          { text: '12 items', color: dim },
          { text: '/Users/dev/docs', color: dim },
        ],
        [
          { text: 'config', color: brand },
          { text: '1 item', color: dim },
          { text: '/Users/dev/.config', color: dim },
        ],
      ],
    };

    return <Table {...props} />;
  },
  parameters: {
    docs: { storyDescription: 'Basic table with headers and colored cells' },
  },
};

export const NoHeaders = {
  render: () => {
    const { getColor } = useTheme();
    const primary = getColor('primary');
    const dim = getColor('secondary');

    return (
      <Table
        columns={[{ width: 15 }, { width: 10 }, {}]}
        rows={[
          [
            { text: 'alpha', color: primary },
            { text: 'active', color: dim },
            { text: 'First entry', color: dim },
          ],
          [
            { text: 'beta', color: primary },
            { text: 'idle', color: dim },
            { text: 'Second entry', color: dim },
          ],
        ]}
        showHeaders={false}
      />
    );
  },
  parameters: {
    docs: { storyDescription: 'Table without header row' },
  },
};

export const WithColors = {
  render: () => {
    const { getColor } = useTheme();
    const primary = getColor('primary');
    const dim = getColor('secondary');
    const brand = getColor('brand');

    return (
      <Table
        columns={[
          { label: 'Name', width: 18 },
          { label: 'ID', width: 12 },
          { label: 'Status', width: 14 },
          { label: 'Path' },
        ]}
        rows={[
          [
            { text: 'kiro-cli', color: primary },
            { text: 'a1b2c3d4', color: brand },
            { text: '142 items', color: dim },
            { text: '~/Desktop/Project/kiro-cli', color: dim },
          ],
          [
            { text: 'docs', color: dim },
            { text: 'e5f6g7h8', color: dim },
            { text: '37% · ETA 5s', color: dim },
            { text: '~/docs', color: dim },
          ],
        ]}
      />
    );
  },
  parameters: {
    docs: {
      storyDescription: 'Table with four columns and mixed cell colors',
    },
  },
};

export const Empty = {
  args: {
    columns: [{ label: 'Name', width: 20 }, { label: 'Value' }],
    rows: [],
  } as TableProps,
  parameters: {
    docs: { storyDescription: 'Table with no rows (headers only)' },
  },
};
