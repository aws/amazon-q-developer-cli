import React from 'react';
import { Box, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useTaskActions,
  useQueueState,
} from '../../../stores/selectors.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';

export const ActivityTray = React.memo(function ActivityTray() {
  const { tasks, activityTrayExpanded } = useTaskState();
  const { queuedMessages } = useQueueState();
  const toggleActivityTray = useTaskActions();

  const hasTasks = tasks.length > 0;
  const hasQueue = queuedMessages.length > 0;
  const visible = hasTasks || hasQueue;

  useInput(
    (input, key) => {
      if (key.ctrl && input === 'x') {
        toggleActivityTray();
      }
    },
    { isActive: visible }
  );

  if (!visible) return null;

  if (activityTrayExpanded) {
    return (
      <Box flexDirection="column">
        <ActivityTrayExpanded queueCount={queuedMessages.length} />
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <ActivityTrayCollapsed queueCount={queuedMessages.length} />
    </Box>
  );
});
