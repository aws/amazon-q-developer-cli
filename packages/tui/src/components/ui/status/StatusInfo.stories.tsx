import React from 'react';
import { Box } from 'ink';
import type { StatusInfoProps } from './StatusInfo.js';
import { StatusInfo } from './StatusInfo.js';
import { StatusBar } from '../../chat/status-bar/StatusBar.js';
import { Card } from '../card/Card.js';

const meta = {
  component: StatusInfo,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const ToolExamples = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="success">
          <StatusInfo title="Read" target="../../docs" />
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <StatusInfo title="Write" target="helloworld.ts" />
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <StatusInfo title="Bash" target="ls -la" />
        </StatusBar>
      </Card>
    </Box>
  ),
};

export const AlertExamples = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="success">
          <StatusInfo title="Operation successful" useStatusColor={true} />
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="info">
          <StatusInfo title="System update available" useStatusColor={true} />
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="warning">
          <StatusInfo title="Disk space low" useStatusColor={true} />
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="error">
          <StatusInfo title="Connection failed" useStatusColor={true} />
        </StatusBar>
      </Card>
    </Box>
  ),
};
