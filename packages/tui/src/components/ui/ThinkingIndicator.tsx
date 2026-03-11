import { Box, Text } from './../../renderer.js';
import { StatusBar } from '../chat/status-bar/StatusBar.js';

export const ThinkingIndicator: React.FC = () => {
  return (
    <StatusBar>
      <Box paddingX={1}>
        <Text dimColor>Thinking...</Text>
      </Box>
    </StatusBar>
  );
};
