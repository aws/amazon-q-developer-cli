import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';

interface MemoriesPanelProps {
  onClose: () => void;
}

const MEMORIES_SETTINGS_URL = 'https://app.kiro.dev/settings/memory';

export const MemoriesPanel: React.FC<MemoriesPanelProps> = ({ onClose }) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');

  return (
    <Panel title="/memories" onClose={onClose}>
      <Box flexDirection="column">
        <Box marginBottom={1}>
          <Text>
            {dim(
              'When turned on, memories will be collected at the end of each session and can help the agent to learn your preferences over time.'
            )}
          </Text>
        </Box>
        <Box>
          <Text>{dim('Turn on/off, delete, and view all at ')}</Text>
          <InkText color="#a855f7">{MEMORIES_SETTINGS_URL}</InkText>
        </Box>
      </Box>
    </Panel>
  );
};
