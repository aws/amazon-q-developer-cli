import { Box } from '../../renderer.js';
import React, { useMemo } from 'react';
import { Text } from '../ui/text/Text.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../hooks/useGlyphs.js';
import type { AgentSession } from '../../types/multi-session.js';

export interface SessionStatusBarProps {
  session: AgentSession;
  messageCount?: number;
  unreadCount?: number;
}

function formatElapsedTime(created: Date): string {
  const elapsed = Date.now() - created.getTime();
  const seconds = Math.floor(elapsed / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);

  if (hours > 0) return `${hours}h${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m${seconds % 60}s`;
  return `${seconds}s`;
}

export const SessionStatusBar = React.memo(function SessionStatusBar({
  session,
  messageCount = 0,
  unreadCount = 0,
}: SessionStatusBarProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const agentColor = getAgentColor(session.name, getColor);
  const statusIcons: Record<string, string> = useMemo(
    () => ({
      idle: !allowIcons ? '' : glyphs.dotEmpty,
      busy: !allowIcons ? '' : glyphs.dotFilled,
      terminated: !allowIcons ? '' : glyphs.checkmark,
      failed: !allowIcons ? '' : glyphs.cross,
      pending: !allowIcons ? '' : glyphs.dotLoading,
    }),
    [glyphs, allowIcons]
  );
  const statusIcon = statusIcons[session.status];
  const elapsed = formatElapsedTime(session.created);

  return (
    <Box flexDirection="row" width="100%" justifyContent="space-between">
      <Box flexDirection="row" gap={1}>
        <Text>{agentColor(session.name)}</Text>
        <Text>{statusIcon}</Text>
        <Text>{session.status}</Text>
      </Box>
      <Box flexDirection="row" gap={2}>
        <Text>{elapsed}</Text>
        {messageCount > 0 && (
          <Text>
            {messageCount}msg{unreadCount > 0 && ` (${unreadCount} unread)`}
          </Text>
        )}
      </Box>
    </Box>
  );
});
