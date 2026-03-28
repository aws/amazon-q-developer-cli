import React, { useState } from 'react';
import { Box, Text } from 'ink';
import { useKeypress } from '../../hooks/useKeypress';
import { useTheme } from '../../hooks/useThemeContext';

export interface MessageInputProps {
  targetSessionId: string;
  targetSessionName: string;
  onSend: (message: string) => void;
  onCancel: () => void;
}

export const MessageInput: React.FC<MessageInputProps> = ({
  targetSessionId: _targetSessionId,
  targetSessionName,
  onSend,
  onCancel,
}) => {
  const [message, setMessage] = useState('');
  const { getColor } = useTheme();

  useKeypress((input, key) => {
    if (key.escape) {
      onCancel();
    } else if (key.return && message.trim()) {
      onSend(message.trim());
      setMessage('');
    } else if (key.backspace) {
      setMessage((prev) => prev.slice(0, -1));
    } else if (input && input.length === 1 && input >= ' ') {
      setMessage((prev) => prev + input);
    }
  });

  return (
    <Box flexDirection="column" borderStyle="single" padding={1}>
      <Text>Send message to {targetSessionName}:</Text>
      <Box marginTop={1}>
        <Text>
          {message}
          <Text color={getColor('primary')}>█</Text>
        </Text>
      </Box>
      <Box marginTop={1}>
        <Text color={getColor('secondary')}>
          Press Enter to send, Esc to cancel
        </Text>
      </Box>
    </Box>
  );
};
