import React from 'react';
import { Box } from 'ink';
import { Alert } from './Alert.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface BlockingErrorAlertProps {
  /** The error message to display */
  message: string;

  /** Optional guidance text for recovery actions */
  guidance?: string;

  /** Callback when dismissed (for recoverable errors) */
  onDismiss?: () => void;
}

/**
 * BlockingErrorAlert displays blocking errors prominently.
 * Used for errors that prevent the application from continuing normal operation.
 * Wraps the Alert component with error status and optional guidance.
 */
export const BlockingErrorAlert = React.memo(function BlockingErrorAlert({
  message,
  guidance,
  onDismiss,
}: BlockingErrorAlertProps) {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');

  return (
    <Box flexDirection="column" paddingX={1} marginBottom={1}>
      <Alert message={message} status="error" onDismiss={onDismiss} />
      {guidance && (
        <Box marginLeft={2} marginTop={0}>
          <Text>{secondaryColor(guidance)}</Text>
        </Box>
      )}
    </Box>
  );
});
