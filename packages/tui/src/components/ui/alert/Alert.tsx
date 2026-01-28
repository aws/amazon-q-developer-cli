import React, { useEffect } from 'react';
import { StatusBar } from '../../chat/status-bar/StatusBar.js';
import { StatusInfo } from '../status/StatusInfo.js';
import type { StatusType } from '../../../types/componentTypes.js';

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
  useEffect(() => {
    if (autoHideMs && onDismiss) {
      const timer = setTimeout(onDismiss, autoHideMs);
      return () => clearTimeout(timer);
    }
  }, [autoHideMs, onDismiss]);

  return (
    <StatusBar status={status}>
      <StatusInfo title={message} useStatusColor={true} />
    </StatusBar>
  );
});
