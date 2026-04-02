import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTaskState } from '../../../stores/selectors.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';

interface ActivityTrayCollapsedProps {
  queueCount: number;
}

export const ActivityTrayCollapsed = React.memo(function ActivityTrayCollapsed({
  queueCount,
}: ActivityTrayCollapsedProps) {
  const { tasks } = useTaskState();
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();

  const bg = getColor('surface').hex;
  const fg = getColor('primary').hex;
  const success = getColor('success').hex;
  const muted = getColor('muted').hex;

  const hasTasks = tasks.length > 0;
  const done = tasks.filter((t) => t.status === 'completed').length;
  const remaining = tasks.length - done;

  // Queue-only mode: no tasks, just pending messages
  if (!hasTasks) {
    return (
      <Box width={termWidth} backgroundColor={bg} paddingX={1}>
        <Box flexGrow={1} overflow="hidden">
          <Text backgroundColor={bg} color={fg} wrap="truncate-end">
            <Text backgroundColor={bg} color={muted}>
              ◇
            </Text>
            <Text backgroundColor={bg} color={fg} bold>
              {' '}
              Queue
            </Text>
            <Text backgroundColor={bg} color={fg}>
              {' '}
              · {queueCount} pending
            </Text>
          </Text>
        </Box>
      </Box>
    );
  }

  // Tasks mode: show task status, append queue badge if present
  return (
    <Box width={termWidth} backgroundColor={bg} paddingX={1}>
      <Box flexGrow={1} overflow="hidden">
        <Text backgroundColor={bg} color={fg} wrap="truncate-end">
          <Text backgroundColor={bg} color={fg} bold>
            ◐ Tasks
          </Text>
          {done > 0 && (
            <Text backgroundColor={bg}>
              {' '}
              ·{' '}
              <Text backgroundColor={bg} color={success}>
                {done} done
              </Text>
            </Text>
          )}
          {remaining > 0 && (
            <Text backgroundColor={bg} color={fg}>
              {' '}
              · {remaining} remaining
            </Text>
          )}
          {queueCount > 0 && (
            <Text backgroundColor={bg} color={muted}>
              {' '}
              · +{queueCount} queued
            </Text>
          )}
        </Text>
      </Box>
      <Text backgroundColor={bg} color={fg} dimColor italic>
        {' '}
        ctrl+x to expand
      </Text>
    </Box>
  );
});
