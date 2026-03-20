import React from 'react';
import { Text, Box } from './../../../renderer.js';
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

export const Thinking = {
  render: () => (
    <Card active={true}>
      <StatusBar status="thinking">
        <Text>Thinking status</Text>
        <Text>Shows braille dot spinner</Text>
      </StatusBar>
    </Card>
  ),
};

export const Paused = {
  render: () => (
    <Card active={true}>
      <StatusBar status="paused">
        <Text>Paused status</Text>
        <Text>Shows arrow down icon</Text>
      </StatusBar>
    </Card>
  ),
};

export const Executing = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="executing">
          <Text>Executing tool (pie spinner)</Text>
          <Text>Bar on lines below, spinner on line 0</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Text>Finished tool (solid dot)</Text>
          <Text>Bar on lines below, dot on line 0</Text>
        </StatusBar>
      </Card>
    </Box>
  ),
};

export const AllStatuses = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="active">
          <Text>Active (no icon, bar only)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="executing">
          <Text>Executing (pie spinner)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="thinking">
          <Text>Thinking (braille spinner)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="paused">
          <Text>Paused (arrow down)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Text>Success (dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="info">
          <Text>Info (dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="warning">
          <Text>Warning (dot)</Text>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="error">
          <Text>Error (dot)</Text>
        </StatusBar>
      </Card>
    </Box>
  ),
};
