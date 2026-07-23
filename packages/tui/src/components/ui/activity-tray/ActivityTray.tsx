import React from 'react';
import { Box, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useTaskActions,
  useQueueState,
} from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';

export const ActivityTray = React.memo(function ActivityTray() {
  const { tasks, activityTrayExpanded } = useTaskState();
  const { pendingSteerContent } = useQueueState();
  const queuedMessages = useAppStore((s) => s.queuedMessages);
  const toggleActivityTray = useTaskActions();
  // showTasks off → hide the task rows (parity with lite's LiteTaskTray gate);
  // steer/queue are not tasks, so they remain visible.
  const { showTasks } = useVerboseDisplay();

  const hasTasks = showTasks && tasks.length > 0;

  // Both steer and queue can be visible simultaneously
  const hasSteer = pendingSteerContent != null;
  const hasQueue = queuedMessages.length > 0;
  const visible = hasTasks || hasSteer || hasQueue;

  const queueCount = queuedMessages.length;

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
        <ActivityTrayExpanded hasTasks={hasTasks} />
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <ActivityTrayCollapsed
        hasTasks={hasTasks}
        hasSteer={hasSteer}
        hasQueue={hasQueue}
        queueCount={queueCount}
      />
    </Box>
  );
});
