import type { Tab } from '../../../renderer.js';
import type { WorkflowRunView } from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import type { Glyphs } from '../../../utils/glyphs.js';
import { workflowProgress } from '../../../stores/workflow-view-model.js';
import { RUN_STATUS_COLOR_TOKEN, runStatusGlyph } from './run-status-style.js';

export interface WorkflowTabDescriptor extends Tab {
  status: WorkflowStatus;
}

export function buildWorkflowTabs(
  workflows: readonly WorkflowRunView[],
  glyphs: Glyphs,
  resolveColor: (token: string) => string
): WorkflowTabDescriptor[] {
  return workflows.map((workflow) => {
    const { completed, total } = workflowProgress(workflow.nodes);
    return {
      id: workflow.workflowId,
      title: `${workflow.name} ${completed}/${total}`,
      icon: runStatusGlyph(workflow.status, glyphs),
      iconColor: resolveColor(RUN_STATUS_COLOR_TOKEN[workflow.status]),
      status: workflow.status,
    };
  });
}

export interface WorkflowRollupCounts {
  running: number;
  paused: number;
  completed: number;
  failed: number;
}

export function rollupWorkflowCounts(
  workflows: Iterable<WorkflowRunView>
): WorkflowRollupCounts {
  const counts: WorkflowRollupCounts = {
    running: 0,
    paused: 0,
    completed: 0,
    failed: 0,
  };
  for (const workflow of workflows) {
    switch (workflow.status) {
      case 'running':
        counts.running += 1;
        break;
      case 'paused':
        counts.paused += 1;
        break;
      case 'completed':
        counts.completed += 1;
        break;
      case 'failed':
      case 'aborted':
        counts.failed += 1;
        break;
    }
  }
  return counts;
}

export function workflowDigitToIndex(
  input: string,
  tabCount: number
): number | null {
  if (input < '1' || input > '9') return null;
  const index = Number.parseInt(input, 10) - 1;
  return index < tabCount ? index : null;
}

export function adjacentWorkflowId(
  workflowIds: readonly string[],
  activeWorkflowId: string | null,
  direction: 1 | -1
): string | null {
  if (workflowIds.length < 2) return null;
  const activeIndex = activeWorkflowId
    ? workflowIds.indexOf(activeWorkflowId)
    : -1;
  const currentIndex = activeIndex >= 0 ? activeIndex : 0;
  const nextIndex =
    (currentIndex + direction + workflowIds.length) % workflowIds.length;
  return workflowIds[nextIndex] ?? null;
}
