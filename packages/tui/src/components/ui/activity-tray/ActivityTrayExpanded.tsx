import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTaskState } from '../../../stores/selectors.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';

const MAX_VISIBLE_LINES = 6;

export const ActivityTrayExpanded = React.memo(function ActivityTrayExpanded() {
  const { tasks } = useTaskState();
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();

  const bg = getColor('surface').hex;
  const fg = getColor('primary').hex;
  const successHex = getColor('success').hex;
  const infoHex = getColor('info').hex;
  const mutedHex = getColor('muted').hex;

  const total = tasks.length;
  const nextIndex = tasks.findIndex((t) => t.status !== 'completed');

  // Auto-follow: keep the next pending task visible with one row of
  // completed context above it. When all done, show the tail.
  const scrollOffset = (() => {
    if (total <= MAX_VISIBLE_LINES) return 0;
    const target = nextIndex === -1 ? total - 1 : nextIndex;
    const maxScroll = total - MAX_VISIBLE_LINES;
    return Math.min(maxScroll, Math.max(0, target - 1));
  })();

  const visibleTasks = tasks.slice(
    scrollOffset,
    scrollOffset + MAX_VISIBLE_LINES
  );

  return (
    <Box flexDirection="column" width={termWidth} backgroundColor={bg}>
      {/* Header */}
      <Box width={termWidth} backgroundColor={bg} paddingX={1}>
        <Box flexGrow={1}>
          <Text backgroundColor={bg} color={fg} bold>
            ◐ Tasks ({total})
          </Text>
        </Box>
        <Text backgroundColor={bg} color={fg} dimColor italic>
          ctrl+x to collapse
        </Text>
      </Box>

      {/* Task list */}
      {visibleTasks.map((task, i) => {
        const globalIndex = scrollOffset + i;
        const isLast = globalIndex === tasks.length - 1;
        const isNext = globalIndex === nextIndex;
        const { icon, color } = getStatusIcon(task.status, isNext, {
          successHex,
          infoHex,
          mutedHex,
        });
        const connector = isLast ? '└──' : '├──';

        return (
          <Box
            key={task.id}
            width={termWidth}
            backgroundColor={bg}
            paddingX={1}
          >
            <Text backgroundColor={bg} dimColor>
              {connector}
            </Text>
            <Text backgroundColor={bg} color={color}>
              {' '}
              {icon}{' '}
            </Text>
            <Text backgroundColor={bg} color={fg}>
              {task.id}.{' '}
            </Text>
            <Text
              backgroundColor={bg}
              color={task.status === 'completed' ? mutedHex : fg}
              strikethrough={task.status === 'completed'}
            >
              {task.subject}
            </Text>
          </Box>
        );
      })}
    </Box>
  );
});

function getStatusIcon(
  status: 'pending' | 'completed',
  isNext: boolean,
  colors: { successHex: string; infoHex: string; mutedHex: string }
): { icon: string; color: string } {
  if (status === 'completed') {
    return { icon: '●', color: colors.successHex };
  }
  if (isNext) {
    return { icon: '◐', color: colors.infoHex };
  }
  return { icon: '○', color: colors.mutedHex };
}
