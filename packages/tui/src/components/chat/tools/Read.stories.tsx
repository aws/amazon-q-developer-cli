import React from 'react';
import { Box } from 'ink';
import { Read } from './Read.js';
import { Card } from '../../ui/card/Card.js';
import { Text } from '../../ui/text/Text.js';

const meta = {
  component: Read,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const ReadFile = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Card active={true}>
        <Read name="Read" target="src/components/Button.tsx" status="success" />
        <Box flexDirection="column" marginTop={1}>
          <Text>import React from 'react';</Text>
          <Text>import {'{ Box }'} from 'ink';</Text>
          <Text> </Text>
          <Text>export const Button = () =&gt; {'{'}</Text>
          <Text>  return &lt;Box&gt;Click me&lt;/Box&gt;;</Text>
          <Text>{'};'}</Text>
        </Box>
      </Card>
      <Card active={true}>
        <Read name="Read" target="missing-file.txt" status="error" />
        <Box flexDirection="column" marginTop={1}>
          <Text>Error: File not found</Text>
        </Box>
      </Card>
    </Box>
  ),
};
