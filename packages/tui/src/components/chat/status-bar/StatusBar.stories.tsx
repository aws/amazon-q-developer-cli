import React from 'react';
import { Text, Box } from 'ink';
import { StatusBar } from './StatusBar.js';
import { Card } from '../../ui/card/Card.js';

const meta = {
  component: StatusBar,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const Active = {
  render: () => (
    <Card active={true}>
      <StatusBar status="active">
        <Text>Active status (default brand color)</Text>
        <Text>Multiple lines of content</Text>
        <Text>No status dot shown</Text>
      </StatusBar>
    </Card>
  ),
};

export const Inactive = {
  render: () => (
    <Card active={false}>
      <StatusBar status="active">
        <Text>Inactive card - no bar shown</Text>
        <Text>Even with active status</Text>
      </StatusBar>
    </Card>
  ),
};

export const Success = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Text>Success status</Text>
        <Text>Shows green color with dot icon</Text>
      </StatusBar>
    </Card>
  ),
};

export const Info = {
  render: () => (
    <Card active={true}>
      <StatusBar status="info">
        <Text>Info status</Text>
        <Text>Shows info color with dot icon</Text>
      </StatusBar>
    </Card>
  ),
};

export const Warning = {
  render: () => (
    <Card active={true}>
      <StatusBar status="warning">
        <Text>Warning status</Text>
        <Text>Shows warning color with dot icon</Text>
      </StatusBar>
    </Card>
  ),
};

export const Error = {
  render: () => (
    <Card active={true}>
      <StatusBar status="error">
        <Text>Error status</Text>
        <Text>Shows error color with dot icon</Text>
      </StatusBar>
    </Card>
  ),
};

export const AllStatuses = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="active">
          <Text>Active (no dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Text>Success (with dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="info">
          <Text>Info (with dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="warning">
          <Text>Warning (with dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="error">
          <Text>Error (with dot)</Text>
        </StatusBar>
      </Card>
    </Box>
  ),
};
