import React, { useEffect } from 'react';
import { useStore } from 'zustand';
import { Box, useInput } from '../../../renderer.js';
import { useTaskActions } from '../../../stores/selectors.js';
import { workflowStore } from '../../../stores/workflow-store.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';
import {
  useActivityTrayInputGateReader,
  useActivityTrayModel,
} from './useActivityTrayModel.js';
import { useAppStore } from '../../../stores/app-store.js';

export const ActivityTray = React.memo(function ActivityTray() {
  const {
    activeTab,
    expanded,
    hasTasks,
    historyOpen,
    inputOwnership,
    navigationActive,
    open,
    queuedMessageCount,
    tabs,
    visible,
  } = useActivityTrayModel();
  const toggleActivityTray = useTaskActions();
  const setWorkflowSurfaceOpen = useStore(
    workflowStore,
    (state) => state.setWorkflowSurfaceOpen
  );
  const readInputGate = useActivityTrayInputGateReader();

  useEffect(() => {
    setWorkflowSurfaceOpen('tray', open);
    return () => {
      if (open) setWorkflowSurfaceOpen('tray', false);
    };
  }, [setWorkflowSurfaceOpen, open]);

  const specCheckpointOpen = useAppStore(
    (s) => s.specPhaseCheckpoint !== null && s.pendingQuestion !== null
  );

  useInput(
    (input, key) => {
      if (!readInputGate().inputEnabled) return;
      if (key.ctrl && input === 'x') {
        // A spec checkpoint claims ctrl+X to open its document for review. This
        // yields on the store alone, while the checkpoint also waits for the
        // question to be on screen — so the key is briefly inert rather than
        // opening the tray a moment before the review shortcut is advertised.
        if (specCheckpointOpen) return;
        toggleActivityTray();
      }
    },
    { isActive: visible && !historyOpen }
  );

  if (!visible) return null;

  if (expanded) {
    return (
      <Box flexDirection="column">
        <ActivityTrayExpanded
          activeTab={activeTab}
          hasTasks={hasTasks}
          inputOwnership={inputOwnership}
          navigationActive={navigationActive}
          tabs={tabs}
        />
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
