import React from 'react';
import { Box, useInput } from '../../../renderer.js';
import { useTaskState, useTaskActions } from '../../../stores/selectors.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';

export const ActivityTray = React.memo(function ActivityTray() {
  const { tasks, activityTrayExpanded } = useTaskState();
  const toggleActivityTray = useTaskActions();

  const visible = tasks.length > 0;

  useInput(
    (input, key) => {
      if (key.ctrl && input === 'x') {
        toggleActivityTray();
      }
    },
    { isActive: visible }
  );

  if (!visible) return null;

  return (
    <Box flexDirection="column">
      {activityTrayExpanded ? (
        <ActivityTrayExpanded />
      ) : (
        <ActivityTrayCollapsed />
      )}
    </Box>
  );
});
