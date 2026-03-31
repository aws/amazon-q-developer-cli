import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Wordmark /*useTheme*/ } from '../brand/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Text } from '../ui/text/Text.js';

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
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const brand = getColor('brand');

  return (
    <Box flexDirection="column" width="100%" alignItems="center">
      <Wordmark animate={animate} />
      {process.env.NODE_ENV !== 'production' && (
        <InkText dimColor>
          {'Development Mode · '}
          {process.env.KIRO_RENDERER === 'ink' ? 'Ink' : 'Twinki'}
        </InkText>
      )}

      <Box
        flexDirection="column"
        alignItems="center"
        marginTop={1}
        paddingX={2}
      >
        <Text>
          {primary('Welcome to the new Kiro CLI UX! Use ')}
          {brand('/feedback')}
          {primary(' to share your thoughts.')}
        </Text>
        <Text>
          {primary('Prefer the classic experience? Relaunch with ')}
          {brand('kiro-cli --classic')}
          {primary('.')}
        </Text>
      </Box>
    </Box>
  );
});
