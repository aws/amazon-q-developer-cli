import React from 'react';
import { Box } from 'ink';
import { NotificationBar } from './NotificationBar.js';

const meta = {
  component: NotificationBar,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const Success = {
  args: {
    message: 'Model changed to claude-sonnet-4',
    status: 'success',
  },
};

export const Error = {
  args: {
    message: 'Command failed - backend unavailable',
    status: 'error',
  },
};

export const Info = {
  args: {
    message: 'Processing your request...',
    status: 'info',
  },
};

export const Warning = {
  args: {
    message: 'Rate limit approaching',
    status: 'warning',
  },
};

export const AllTypes = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <NotificationBar
        message="Operation completed successfully"
        status="success"
      />
      <NotificationBar message="New update available" status="info" />
      <NotificationBar message="Disk space running low" status="warning" />
      <NotificationBar message="Connection failed" status="error" />
    </Box>
  ),
};
