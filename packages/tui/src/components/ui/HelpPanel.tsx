import React from 'react';
import { Box, useInput } from 'ink';
import { Text } from './text/Text';
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
      <Text>{getColor('brand')('Available Commands')}</Text>
      {commands.map((cmd) => (
        <Box key={cmd.name} flexDirection="column">
          <Text>
            {getColor('primary')(cmd.name)}{' '}
            {getColor('secondary')(`- ${cmd.description}`)}
          </Text>
          <Text> {getColor('secondary')(cmd.usage)}</Text>
        </Box>
      ))}
      <Text>
        {getColor('secondary')('Press ')}
        {getColor('brand')('Esc')}
        {getColor('secondary')(' to close')}
      </Text>
    </Box>
  );
};
