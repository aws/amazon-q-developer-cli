import React from 'react';
import { Box } from 'ink';
import { Alert } from './Alert.js';
import { Card } from '../card/Card.js';

const meta = {
  component: Alert,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const AlertTypes = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <Alert message="Operation completed successfully" status="success" />
      </Card>
      <Card active={true}>
        <Alert message="New update available" status="info" />
      </Card>
      <Card active={true}>
        <Alert message="Disk space running low" status="warning" />
      </Card>
      <Card active={true}>
        <Alert message="Connection to server failed" status="error" />
      </Card>
    </Box>
  ),
};
