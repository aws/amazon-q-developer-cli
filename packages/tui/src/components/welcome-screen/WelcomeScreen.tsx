import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
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
      {(process.env.NODE_ENV !== 'production' ||
        process.env.KIRO_RENDERER !== 'ink') && (
        <InkText dimColor>
          {process.env.NODE_ENV !== 'production' ? 'Development Mode · ' : ''}
          {process.env.KIRO_RENDERER === 'ink' ? 'Ink' : 'Twinki'}
        </InkText>
      )}

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
