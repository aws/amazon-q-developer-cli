import React from 'react';
import { Box } from 'ink';
import { Shell } from './Shell.js';
import { Card } from '../../ui/card/Card.js';
import { Text } from '../../ui/text/Text.js';

const meta = {
  component: Shell,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const ListDirectory = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <Shell name="Bash" command="ls -la" status="success" />
        <Box flexDirection="column" marginTop={1}>
          <Text>drwxr-xr-x 12 user staff 384 Jan 14 10:30 .</Text>
          <Text>drwxr-xr-x 8 user staff 256 Jan 13 15:22 ..</Text>
          <Text>-rw-r--r-- 1 user staff 1024 Jan 14 09:15 README.md</Text>
          <Text>drwxr-xr-x 5 user staff 160 Jan 12 14:30 src</Text>
          <Text>-rw-r--r-- 1 user staff 512 Jan 11 16:45 package.json</Text>
          <Text>drwxr-xr-x 15 user staff 480 Jan 14 10:28 node_modules</Text>
        </Box>
      </Card>
      <Card active={true}>
        <Shell name="Bash" command="ls -la" status="error" />
        <Box flexDirection="column" marginTop={1}>
          <Text>Could not list any files or folders in this directory</Text>
        </Box>
      </Card>
    </Box>
  ),
};
