import React from 'react';
import { Box, Text as InkText } from 'ink';
import { Wordmark /*useTheme*/ } from '../brand/index.js';

export interface WelcomeScreenProps {
  agent: string;
  mcpServers: string[];
  animate?: boolean;
}

export const WelcomeScreen = React.memo(function WelcomeScreen({
  // agent,
  // mcpServers,
  animate = false,
}: WelcomeScreenProps) {
  const isDevelopment = process.env.NODE_ENV !== 'production';
  // const { getColor } = useTheme();

  return (
    <Box flexDirection="column" width="100%" alignItems="center">
      <Wordmark animate={animate} />
      {isDevelopment && <InkText dimColor>Development Mode</InkText>}

      {/* <Box width="100%">
        <Text>{getColor('text')('━'.repeat(120))}</Text>
      </Box> */}

      {/* <Box flexDirection="row" flexWrap="wrap">
        <Text>{getColor('primary')('🤖 Agent: ')}</Text>
        <Text>{getColor('text')(agent)}</Text>
        {mcpServers.length > 0 && (
          <>
            <Text>{getColor('text')(' • MCP: ')}</Text>
            <Text>{getColor('text')(`${mcpServers.length} servers`)}</Text>
          </>
        )}
      </Box> */}
    </Box>
  );
});
