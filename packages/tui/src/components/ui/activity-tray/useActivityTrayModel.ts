import { useCallback, useContext } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
import {
  AppStoreContext,
  useAppStore,
  type AppStoreApi,
  type AppState,
} from '../../../stores/app-store.js';
import { selectVisibleSlashCommands } from '../../../stores/visible-slash-commands.js';
import {
  selectLiveWorkflowCount,
  workflowStore,
} from '../../../stores/workflow-store.js';
import { buildUnifiedQueueEntries } from '../../../utils/queue-navigation.js';
import { isPromptMenuOpenForState } from '../command-menu-utils.js';
import {
  availableActivityTrayTabs,
  resolveActivityTrayTab,
} from './tray-tabs.js';
import { isActivityTrayVisible } from './tray-view-model.js';
import type { ActivityTrayInputOwnership } from './input-ownership.js';

function selectActivityTrayAppInputGate(state: AppState) {
  const promptMenuOpen = isPromptMenuOpenForState(
    state,
    selectVisibleSlashCommands(state)
  );
  return {
    appInputEnabled:
      state.editingQueueIndex == null &&
      state.editingSteerLineIndex == null &&
      state.pendingApproval == null &&
      state.pendingQuestion == null &&
      !promptMenuOpen,
    promptMenuOpen,
  };
}

export function readActivityTrayInputGate(store: AppStoreApi) {
  const { appInputEnabled, promptMenuOpen } = selectActivityTrayAppInputGate(
    store.getState()
  );
  const historyOpen = workflowStore.getState().history.isOpen;
  return {
    historyOpen,
    inputEnabled: appInputEnabled && !historyOpen,
    promptMenuOpen,
  };
}

export function useActivityTrayInputGateReader() {
  const store = useContext(AppStoreContext);
  if (!store) throw new Error('Missing StoreContext.Provider in the tree');
  return useCallback(() => readActivityTrayInputGate(store), [store]);
}

export function useActivityTrayModel() {
  const visibility = useActivityTrayVisibility();
  const requestedTab = useAppStore((state) => state.activityTrayTab);
  const { historyOpen, inputEnabled } = useActivityTrayInputGate();
  const hasActiveWorkflow = useStore(
    workflowStore,
    (state) =>
      state.activeWorkflowId !== null &&
      state.workflows.has(state.activeWorkflowId)
  );
  const tabs = availableActivityTrayTabs({
    hasTasks: visibility.hasTasks,
    hasMessages: visibility.queuedMessageCount > 0,
    hasWorkflow: hasActiveWorkflow,
  });
  const activeTab = resolveActivityTrayTab(tabs, requestedTab);
  const navigationActive = visibility.open && inputEnabled;
  const inputOwnership: ActivityTrayInputOwnership = {
    rowNavigationActive:
      navigationActive && (activeTab === 'tasks' || activeTab === 'queue'),
    tabNavigationActive: navigationActive && tabs.length > 1,
    workflowNavigationActive: navigationActive && activeTab === 'workflow',
  };

  return {
    ...visibility,
    activeTab,
    historyOpen,
    inputOwnership,
    navigationActive,
    tabs,
  };
}

export function useActivityTrayInputGate() {
  const { appInputEnabled, promptMenuOpen } = useAppStore(
    useShallow(selectActivityTrayAppInputGate)
  );
  const historyOpen = useStore(workflowStore, (state) => state.history.isOpen);

  return {
    historyOpen,
    inputEnabled: appInputEnabled && !historyOpen,
    promptMenuOpen,
  };
}

export function useActivityTrayVisibility() {
  const { expanded, hasAnyTasks, queuedMessageCount } = useAppStore(
    useShallow((state) => ({
      expanded: state.activityTrayExpanded,
      hasAnyTasks: state.tasks.length > 0,
      queuedMessageCount: buildUnifiedQueueEntries(
        state.pendingSteerContent,
        state.queuedMessages
      ).length,
    }))
  );
  const { liveWorkflowCount, workflowCount } = useStore(
    workflowStore,
    useShallow((state) => ({
      liveWorkflowCount: selectLiveWorkflowCount(state),
      workflowCount: state.workflows.size,
    }))
  );
  const { showTasks } = useVerboseDisplay();
  const hasTasks = showTasks && hasAnyTasks;
  const visible = isActivityTrayVisible({
    hasTasks,
    hasMessages: queuedMessageCount > 0,
    liveWorkflowCount,
    workflowCount,
    expanded,
  });

  return {
    expanded,
    hasTasks,
    open: visible && expanded,
    queuedMessageCount,
    visible,
  };
}
