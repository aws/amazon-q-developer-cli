import type { ActivityTrayTab } from './tray-tabs.js';
import type { WorkflowRunView } from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import { workflowProgress } from '../../../stores/workflow-view-model.js';

export interface ActivityTrayVisibility {
  hasTasks: boolean;
  hasMessages: boolean;
  liveWorkflowCount: number;
  workflowCount: number;
  expanded: boolean;
}

export function isActivityTrayVisible({
  hasTasks,
  hasMessages,
  liveWorkflowCount,
  workflowCount,
  expanded,
}: ActivityTrayVisibility): boolean {
  return (
    hasTasks ||
    hasMessages ||
    liveWorkflowCount > 0 ||
    (expanded && workflowCount > 0)
  );
}

export interface CollapsedWorkflowEntry {
  workflowId: string;
  name: string;
  status: WorkflowStatus;
  completedSteps: number;
  totalSteps: number;
}

export interface CollapsedWorkflowSummary {
  entries: CollapsedWorkflowEntry[];
  hiddenCount: number;
}

export function collapsedWorkflowSummary(
  workflows: readonly WorkflowRunView[],
  maxVisible = 3
): CollapsedWorkflowSummary {
  const visibleCount = Math.max(0, Math.floor(maxVisible));
  return {
    entries: workflows.slice(0, visibleCount).map((workflow) => {
      const progress = workflowProgress(workflow.nodes);
      return {
        workflowId: workflow.workflowId,
        name: workflow.name,
        status: workflow.status,
        completedSteps: progress.completed,
        totalSteps: progress.total,
      };
    }),
    hiddenCount: Math.max(0, workflows.length - visibleCount),
  };
}

export function workflowActivityLabel(
  running: number,
  paused: number
): string | null {
  if (running + paused < 2) return null;
  if (paused === 0) return `${running} workflows running`;
  if (running === 0) return `${paused} workflows paused`;
  return `${running} workflow${running === 1 ? '' : 's'} running, ${paused} paused`;
}

export interface TrayScrollInput {
  activeTab: ActivityTrayTab | null;
  itemCount: number;
  taskStatuses: readonly string[];
  selectedIndex: number;
  maxVisible: number;
}

export function activityTrayScrollOffset({
  activeTab,
  itemCount,
  taskStatuses,
  selectedIndex,
  maxVisible,
}: TrayScrollInput): number {
  if (activeTab === 'tasks') {
    if (taskStatuses.length <= maxVisible) return 0;
    const nextIndex = taskStatuses.findIndex(
      (status) => status !== 'completed'
    );
    const target =
      nextIndex === -1 ? taskStatuses.length - 1 : Math.max(0, nextIndex - 1);
    return Math.min(taskStatuses.length - maxVisible, target);
  }

  if (itemCount <= maxVisible) return 0;
  return Math.min(itemCount - maxVisible, Math.max(0, selectedIndex - 1));
}

export interface TrayHintInput {
  activeTab: ActivityTrayTab | null;
  queueCount: number;
  hasSteer: boolean;
  editing: boolean;
  workflowNodeCount: number;
  workflowCount: number;
  tabCount: number;
  arrows: string;
}

export function activityTrayHints({
  activeTab,
  queueCount,
  hasSteer,
  editing,
  workflowNodeCount,
  workflowCount,
  tabCount,
  arrows,
}: TrayHintInput): string[] {
  if (editing) return ['esc to cancel'];

  const hints: string[] = [];
  if (activeTab === 'queue' && queueCount > 1) {
    hints.push(`shift+${arrows} or ctrl+p/n navigate`);
  }
  if (activeTab === 'queue' && queueCount > 0) {
    hints.push('enter edit', 'del remove');
  } else if (activeTab === 'queue' && hasSteer) {
    hints.push('del remove');
  }
  if (activeTab === 'workflow' && workflowNodeCount > 1) {
    hints.push(`shift+${arrows} select`, 'ctrl+g monitor');
  }
  if (activeTab === 'workflow' && workflowCount > 1) {
    hints.push('shift+left/right workflows');
  }
  if (tabCount > 1) hints.push('tab switch');
  hints.push('ctrl+x collapse');
  return hints;
}
