import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTaskState } from '../../../stores/selectors.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';

interface ActivityTrayCollapsedProps {
  hasTasks: boolean;
  hasSteer: boolean;
  hasQueue: boolean;
  queueCount: number;
}

export const ActivityTrayCollapsed = React.memo(function ActivityTrayCollapsed({
  hasTasks,
  hasSteer,
  hasQueue,
  queueCount,
}: ActivityTrayCollapsedProps) {
  const { tasks } = useTaskState();
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();

  const rawBg = getColor('surface').hex;
  const bg = rawBg === 'inherit' ? undefined : rawBg;
  const rawFg = getColor('primary').hex;
  const fg = rawFg === 'inherit' ? undefined : rawFg;
  const success = getColor('success').hex;
  const rawMuted = getColor('muted').hex;
  const muted = rawMuted === 'inherit' ? undefined : rawMuted;

  const done = tasks.filter((t) => t.status === 'completed').length;
  const remaining = tasks.length - done;

  // No tasks: show steer/queue status only
  if (!hasTasks) {
    return (
      <Box width={termWidth} backgroundColor={bg} paddingX={1}>
        <Box flexGrow={1} overflow="hidden">
          <Text backgroundColor={bg} color={fg} wrap="truncate-end">
            {hasSteer && (
              <>
                {allowIcons && (
                  <Text backgroundColor={bg} color={muted}>
                    {glyphs.executing}{' '}
                  </Text>
                )}
                <Text backgroundColor={bg} color={fg} bold>
                  Steer {glyphs.smallDot} pending
                </Text>
              </>
            )}
            {hasSteer && hasQueue && (
              <Text backgroundColor={bg} color={fg}>
                {` ${glyphs.smallDot} `}
              </Text>
            )}
            {hasQueue && (
              <>
                {allowIcons && (
                  <Text backgroundColor={bg} color={muted}>
                    {glyphs.diamond}{' '}
                  </Text>
                )}
                <Text backgroundColor={bg} color={fg} bold>
                  Queue {glyphs.smallDot} {queueCount} pending
                </Text>
              </>
            )}
            <Text backgroundColor={bg} color={fg}>
              {' '}
              {glyphs.smallDot} ctrl+x to view and manage
            </Text>
          </Text>
        </Box>
      </Box>
    );
  }

  // Tasks mode: show task status, append steer/queue badges if present
  return (
    <Box width={termWidth} backgroundColor={bg} paddingX={1}>
      <Box flexGrow={1} overflow="hidden">
        <Text backgroundColor={bg} color={fg} wrap="truncate-end">
          <Text backgroundColor={bg} color={fg} bold>
            {!allowIcons ? '' : glyphs.executing} Tasks
          </Text>
          {done > 0 && (
            <Text backgroundColor={bg}>
              {' '}
              {glyphs.smallDot}{' '}
              <Text backgroundColor={bg} color={success}>
                {done} done
              </Text>
            </Text>
          )}
          {remaining > 0 && (
            <Text backgroundColor={bg} color={fg}>
              {' '}
              {glyphs.smallDot} {remaining} remaining
            </Text>
          )}
          {hasSteer && (
            <Text backgroundColor={bg} color={muted}>
              {' '}
              {glyphs.smallDot} +1 steer
            </Text>
          )}
          {hasQueue && (
            <Text backgroundColor={bg} color={muted}>
              {' '}
              {glyphs.smallDot} +{queueCount} queued
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
