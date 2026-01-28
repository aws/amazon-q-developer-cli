import React from 'react';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import type { StatusType } from '../../../types/componentTypes.js';

export interface ReadProps {
  /** The tool name */
  name: string;
  
  /** Optional file path or target */
  target?: string;
  
  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;
}

export const Read = React.memo(function Read({ 
  name,
  target,
  status,
  noStatusBar = false,
}: ReadProps) {
  const content = <StatusInfo title={name} target={target} />;

  if (noStatusBar) {
    return content;
  }

  return (
    <StatusBar status={status}>
      {content}
    </StatusBar>
  );
});
