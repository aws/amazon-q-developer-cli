import type { WorkflowStatus } from '../../../types/workflow.js';
import { isLiveWorkflowStatus } from '../../../types/workflow-status.js';

export type ActivityTrayTab = 'tasks' | 'queue' | 'workflow';

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
