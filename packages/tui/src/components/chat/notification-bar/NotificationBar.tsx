import React from 'react';
import { Box } from 'ink';
import { Alert } from '../../ui/alert/Alert.js';
import type { StatusType } from '../../../types/componentTypes.js';

interface NotificationBarProps {
  message?: string;
  status?: StatusType;
  autoHideMs?: number;
  onDismiss?: () => void;
}

export function NotificationBar({
  message,
  status,
  autoHideMs,
  onDismiss,
}: NotificationBarProps) {
  if (!message || !status) {
    return <Box height={1} />;
  }

  return (
    <Box height={1}>
      <Alert
        message={message}
        status={status}
        autoHideMs={autoHideMs}
        onDismiss={onDismiss}
      />
    </Box>
  );
}
