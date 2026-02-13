import React from 'react';
import { Box, useInput } from 'ink';
import { Text } from './text/Text';
import { Divider } from './divider/Divider';
import { useTheme } from '../../hooks/useThemeContext';

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

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    }
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      <Box justifyContent="space-between" marginBottom={0}>
        <Text>{getColor('primary')('/help')}</Text>
        <Text>
          {getColor('secondary')('(')}
          {getColor('brand')('ESC')}
          {getColor('secondary')(' to close)')}
        </Text>
      </Box>
      <Divider />
      {commands.map((cmd) => (
        <Box key={cmd.name} flexDirection="column">
          <Text>
            {getColor('primary')(cmd.name)}{' '}
            {getColor('secondary')(`- ${cmd.description}`)}
          </Text>
          <Text> {getColor('secondary')(cmd.usage)}</Text>
        </Box>
      ))}
    </Box>
  );
};
