import React from 'react';
import { Box } from './../../../renderer.js';
import { ToolMeta } from './ToolMeta.js';
import { Card } from '../../ui/card/Card.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';

const meta = {
  component: ToolMeta,
  parameters: { layout: 'fullscreen' },
  tags: ['autodocs'],
};

export default meta;

export const Variants = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <StatusBar status="success">
          <Box flexDirection="column">
            <StatusInfo title="Grep" target={'"useTheme"'} />
            <ToolMeta
              params={['caseSensitive=true', 'includePattern=**/*.tsx']}
            />
          </Box>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Box flexDirection="column">
            <StatusInfo title="Shell" target="npm run build" />
            <ToolMeta params={['timeout=30', 'cwd=/home/user/project']} />
          </Box>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Box flexDirection="column">
            <StatusInfo title="Read" target="config.ts" />
            <ToolMeta params={null} />
          </Box>
        </StatusBar>
      </Card>
      <Card active={true}>
        <StatusBar status="success">
          <Box flexDirection="column">
            <StatusInfo title="Glob" target={'"**/*.test.ts"'} />
            <ToolMeta
              params={[
                'max_depth=10',
                'exclude=node_modules',
                'include=src',
                'followSymlinks=true',
              ]}
            />
          </Box>
        </StatusBar>
      </Card>
    </Box>
  ),
};
