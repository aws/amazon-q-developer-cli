import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Wordmark /*useTheme*/ } from '../brand/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Text } from '../ui/text/Text.js';
import { useAppStore } from '../../stores/app-store.js';

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
  const agentEngine = useAppStore((s) => s.agentEngine);

  return (
    <Box flexDirection="column" width="100%" alignItems="center">
      <Wordmark animate={animate} />
      {process.env.NODE_ENV !== 'production' && (
        <InkText dimColor>{'Development Mode · Twinki'}</InkText>
      )}

      <Box
        flexDirection="column"
        alignItems="center"
        marginTop={1}
        paddingX={2}
      >
        {agentEngine !== 'kas' ? (
          <>
            <Text>
              {primary('An early release of ')}
              {brand('Kiro CLI V3')}
              {primary(' is now available! Try it out: ')}
              {brand('kiro-cli --v3')}
            </Text>
            <Text>{primary(' ')}</Text>
            <Text>
              {primary(
                "What's new: Specs, expanded hooks, and an improved trust model."
              )}
            </Text>
            <Text>
              {primary(
                'Migration tooling to bring your V2 configurations to V3 is coming soon.'
              )}
            </Text>
            <Text>{brand('https://kiro.dev/changelog/cli')}</Text>
            <Text>{primary(' ')}</Text>
            <Text>
              {primary('Share feedback anytime with ')}
              {brand('/feedback')}
              {primary('.')}
            </Text>
          </>
        ) : (
          <>
            <Text>
              {primary('Welcome to the new Kiro CLI UX! ')}
              {brand('/feedback')}
              {primary(' for thoughts.')}
            </Text>
            <Text>
              {primary('Prefer the classic experience? Relaunch with ')}
              {brand('kiro-cli --classic')}
              {primary('.')}
            </Text>
          </>
        )}
      </Box>
    </Box>
  );
});
