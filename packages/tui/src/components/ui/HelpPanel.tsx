import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';

interface Command {
  name: string;
  description: string;
  usage: string;
}

interface HelpPanelProps {
  commands: Command[];
  onClose: () => void;
}

export const HelpPanel: React.FC<HelpPanelProps> = ({ commands, onClose }) => {
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const dim = getColor('secondary');

  return (
    <Panel title="/help" onClose={onClose}>
      {commands.map((cmd) => (
        <Box key={cmd.name} flexDirection="column">
          <Text>
            {primary(cmd.name)} {dim(`- ${cmd.description}`)}
          </Text>
          <Text> {dim(cmd.usage)}</Text>
        </Box>
      ))}
    </Panel>
  );
};
