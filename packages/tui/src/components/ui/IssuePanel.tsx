import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';

interface IssuePanelProps {
  url: string;
  message: string;
  onClose: () => void;
}

export const IssuePanel: React.FC<IssuePanelProps> = ({
  url,
  message,
  onClose,
}) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');

  return (
    <Panel title="/issue" onClose={onClose}>
      <Box flexDirection="column" gap={1}>
        <Text>{message}</Text>
        <Text>{url}</Text>
        <Text>{dim('Press ESC to close')}</Text>
      </Box>
    </Panel>
  );
};
