import React from 'react';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import type { StatusType } from '../../../types/componentTypes.js';

export interface ShellProps {
  /** The tool name */
  name: string;
  
  /** Optional bash command */
  command?: string;
  
  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;
}

export const Shell = React.memo(function Shell({ 
  name,
  command,
  status,
  noStatusBar = false,
}: ShellProps) {
  const content = <StatusInfo title={name} target={command} />;

  if (noStatusBar) {
    return content;
  }

  return (
    <StatusBar status={status}>
      {content}
    </StatusBar>
  );
});
