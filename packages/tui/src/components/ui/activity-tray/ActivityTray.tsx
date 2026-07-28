import React, { useEffect } from 'react';
import { useStore } from 'zustand';
import { Box, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useTaskActions,
  useQueueState,
} from '../../../stores/selectors.js';
import {
  selectLiveWorkflowCount,
  workflowStore,
} from '../../../stores/workflow-store.js';
import { buildUnifiedQueueEntries } from '../../../utils/queue-navigation.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';

export const ActivityTray = React.memo(function ActivityTray() {
  const { tasks, activityTrayExpanded } = useTaskState();
  const { pendingSteerContent, queuedMessages } = useQueueState();
  const toggleActivityTray = useTaskActions();
  const historyOpen = useStore(workflowStore, (state) => state.history.isOpen);
  const workflowCount = useStore(
    workflowStore,
    (state) => state.workflows.size
  );
  const liveWorkflowCount = useStore(workflowStore, selectLiveWorkflowCount);
  const setWorkflowSurfaceOpen = useStore(
    workflowStore,
    (state) => state.setWorkflowSurfaceOpen
  );

  const hasTasks = tasks.length > 0;
  const queuedMessageCount = buildUnifiedQueueEntries(
    pendingSteerContent,
    queuedMessages
  ).length;
  const visible =
    hasTasks ||
    queuedMessageCount > 0 ||
    liveWorkflowCount > 0 ||
    (activityTrayExpanded && workflowCount > 0);
  const traySurfaceOpen = visible && activityTrayExpanded;

  useEffect(() => {
    setWorkflowSurfaceOpen('tray', traySurfaceOpen);
    return () => {
      if (traySurfaceOpen) setWorkflowSurfaceOpen('tray', false);
    };
  }, [setWorkflowSurfaceOpen, traySurfaceOpen]);

  useInput(
    (input, key) => {
      if (key.ctrl && input === 'x') {
        toggleActivityTray();
      }
    },
    { isActive: visible && !historyOpen }
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
        queuedMessageCount={queuedMessageCount}
      />
    </Box>
  );
});
