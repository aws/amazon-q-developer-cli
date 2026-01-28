import React from 'react';
import { Box } from 'ink';
import { Text } from '../ui/text/Text.js';
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
  // const { getColor } = useTheme();

  return (
    <Box flexDirection="column" width="100%" alignItems="center">
      <Wordmark animate={animate} />

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
