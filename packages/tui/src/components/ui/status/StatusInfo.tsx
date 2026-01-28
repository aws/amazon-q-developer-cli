import React from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { Text } from '../text/Text.js';
import { getStatusColor } from '../../../utils/colorUtils.js';
import { useStatusBar } from '../../chat/status-bar/StatusBar.js';
import type { StatusType } from '../../../types/componentTypes.js';

export interface StatusInfoProps {
  /** The main heading/identifier (e.g., tool name, alert message) */
  title: string;
  
  /** Optional target/context shown in parentheses (e.g., file path, environment) */
  target?: string;
  
  /** Optional status type for color theming - if provided, overrides StatusBar context */
  status?: StatusType;
  
  /** Whether to color the title based on status. Defaults to false. */
  useStatusColor?: boolean;
}

export const StatusInfo = React.memo(function StatusInfo({ 
  title, 
  target, 
  status: statusProp,
  useStatusColor = false,
}: StatusInfoProps) {
  const { getColor } = useTheme();

  // Try to get status from StatusBar context
  let contextStatus: StatusType | undefined;
  try {
    const statusBar = useStatusBar();
    contextStatus = statusBar.status;
  } catch {
    // Not inside a StatusBar, that's okay
  }

  // Use prop if provided, otherwise use context, otherwise undefined
  const status = statusProp ?? contextStatus;

  const titleColor = (useStatusColor && status) ? getStatusColor(status, getColor) : getColor('primary');
  const targetColor = getColor('secondary');

  return (
    <Text>
      {titleColor(title)}
      {target && targetColor(`(${target})`)}
    </Text>
  );
});
