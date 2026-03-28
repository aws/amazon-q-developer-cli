import React, { useEffect } from 'react';
import { Box } from 'ink';
import { StatusBar } from '../../chat/status-bar/StatusBar.js';
import { StatusInfo } from '../status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { getStatusColor } from '../../../utils/colorUtils.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { ShimmerText } from '../shimmer/ShimmerText.js';
import { Text } from '../text/Text.js';

export interface AlertProps {
  message: string;
  status: StatusType;
  autoHideMs?: number;
  onDismiss?: () => void;
  /** Optional action hint shown inline, e.g. "r: read" */
  actionHint?: string;
}

export const Alert = React.memo(function Alert({
  message,
  status,
  autoHideMs,
  onDismiss,
  actionHint,
}: AlertProps) {
  const { getColor } = useTheme();

  useEffect(() => {
    if (autoHideMs && onDismiss) {
      const timer = setTimeout(onDismiss, autoHideMs);
      return () => clearTimeout(timer);
    }
  }, [autoHideMs, onDismiss, message]);

  if (status === 'loading') {
    const color = getStatusColor(status, getColor).hex;
    return (
      <StatusBar status={status}>
        <ShimmerText text={message} color={color} />
      </StatusBar>
    );
  }

  return (
    <StatusBar status={status}>
      <Box>
        <StatusInfo title={message} useStatusColor={true} />
        {actionHint && <Text> {getColor('muted')(actionHint)}</Text>}
      </Box>
    </StatusBar>
  );
});
