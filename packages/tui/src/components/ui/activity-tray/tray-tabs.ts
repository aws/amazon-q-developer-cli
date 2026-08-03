import type { WorkflowStatus } from '../../../types/workflow.js';
import { isLiveWorkflowStatus } from '../../../types/workflow-status.js';
import type { ActivityTrayTab } from '../../../types/activity-tray.js';

export type { ActivityTrayTab } from '../../../types/activity-tray.js';

export function isWorkflowTrayActive(
  status: WorkflowStatus | null | undefined
): boolean {
  return (
    status !== null && status !== undefined && isLiveWorkflowStatus(status)
  );
}

export interface ActivityTrayTabPresence {
  hasTasks: boolean;
  hasMessages: boolean;
  hasWorkflow: boolean;
}

export function availableActivityTrayTabs({
  hasTasks,
  hasMessages,
  hasWorkflow,
}: ActivityTrayTabPresence): ActivityTrayTab[] {
  const tabs: ActivityTrayTab[] = [];
  if (hasTasks) tabs.push('tasks');
  if (hasMessages) tabs.push('queue');
  if (hasWorkflow) tabs.push('workflow');
  return tabs;
}

export function nextActivityTrayTab(
  tabs: readonly ActivityTrayTab[],
  activeTab: ActivityTrayTab
): ActivityTrayTab {
  if (tabs.length <= 1) return activeTab;
  const activeIndex = tabs.indexOf(activeTab);
  if (activeIndex === -1) return tabs[0] ?? activeTab;
  return tabs[(activeIndex + 1) % tabs.length] ?? activeTab;
}

export function resolveActivityTrayTab(
  tabs: readonly ActivityTrayTab[],
  requestedTab: ActivityTrayTab | null
): ActivityTrayTab | null {
  if (requestedTab && tabs.includes(requestedTab)) return requestedTab;
  if (tabs.includes('workflow')) return 'workflow';
  return tabs[0] ?? null;
}
