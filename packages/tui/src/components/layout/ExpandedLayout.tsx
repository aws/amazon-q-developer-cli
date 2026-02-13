import React from 'react';
import { Box, Text } from 'ink';
import {
  useAppStore,
  type AppState,
  type AppActions,
} from '../../stores/app-store';

export const ExpandedLayout: React.FC = () => {
  const setMode = useAppStore((s: AppState & AppActions) => s.setMode);
  const messages = useAppStore((s: AppState & AppActions) => s.messages);
  const currentMessage = null; // Not in store - placeholder for future

  return (
    <Box flexDirection="column" height="100%">
      <Box padding={1}>
        <Text bold color="cyan">
          Kiro CLI Chat - Expanded Mode
        </Text>
        <Text dimColor> (Press Escape to return to inline mode)</Text>
      </Box>

      <Box flexGrow={1} flexDirection="column">
        <Text>Expanded mode - TODO: Implement full-screen chat interface</Text>
        <Text dimColor>Messages: {messages.length}</Text>
        <Text dimColor>Current message: {currentMessage ? 'Yes' : 'No'}</Text>
      </Box>

      <Box padding={1}>
        <Text dimColor>Press Escape to return to inline mode</Text>
      </Box>
    </Box>
  );
};
