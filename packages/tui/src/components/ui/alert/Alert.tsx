import React, { useEffect } from 'react';
import { StatusBar } from '../../chat/status-bar/StatusBar.js';
import { StatusInfo } from '../status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { getStatusColor } from '../../../utils/colorUtils.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { ShimmerText } from '../shimmer/ShimmerText.js';

export interface AlertProps {
  /** The alert message/title */
  message: string;

  /** Alert status type */
  status: StatusType;

  /** Auto-dismiss after ms (optional) */
  autoHideMs?: number;

  /** Callback when auto-dismissed */
  onDismiss?: () => void;
}

export const Alert = React.memo(function Alert({
  message,
  status,
  autoHideMs,
  onDismiss,
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
      <StatusInfo title={message} useStatusColor={true} />
    </StatusBar>
  );
});
