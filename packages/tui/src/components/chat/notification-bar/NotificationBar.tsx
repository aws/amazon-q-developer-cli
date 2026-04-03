import React from 'react';
import { Box } from './../../../renderer.js';
import { Alert } from '../../ui/alert/Alert.js';
import type { StatusType } from '../../../types/componentTypes.js';

interface NotificationBarProps {
  message?: string;
  status?: StatusType;
  autoHideMs?: number;
  onDismiss?: () => void;
  actionHint?: string;
}

export function NotificationBar({
  message,
  status,
  autoHideMs,
  onDismiss,
  actionHint,
}: NotificationBarProps) {
  if (!message || !status) {
    return <Box height={1} />;
  }

  return (
    <Alert
      message={message}
      status={status}
      autoHideMs={autoHideMs}
      onDismiss={onDismiss}
      actionHint={actionHint}
    />
  );
}
