import { truncateToWidth, visibleWidth } from '../../../utils/text-width.js';

export interface LiteActivityCounts {
  runningWorkflows: number;
  pausedWorkflows: number;
  completedSteps: number;
  totalSteps: number;
  queuedMessages: number;
  remainingTasks: number;
  completedTasks: number;
}

function plural(count: number, singular: string): string {
  return `${count} ${singular}${count === 1 ? '' : 's'}`;
}

function workflowLabels(
  running: number,
  paused: number
): { full: string; compact: string; terse: string } | null {
  const total = running + paused;
  if (total === 0) return null;

  if (running === 0) {
    return {
      full: `${plural(paused, 'workflow')} paused`,
      compact: `${paused} paused`,
      terse: plural(total, 'workflow'),
    };
  }
  if (paused === 0) {
    return {
      full: `${plural(running, 'workflow')} running`,
      compact: `${running} running`,
      terse: plural(total, 'workflow'),
    };
  }
  return {
    full: `${plural(running, 'workflow')} running, ${paused} paused`,
    compact: `${running} running, ${paused} paused`,
    terse: plural(total, 'workflow'),
  };
}

function join(segments: readonly string[], separator: string): string {
  return segments.filter(Boolean).join(separator);
}

/**
 * Builds one non-wrapping Lite activity row. Detail is reduced before
 * the expand control, so narrow terminals retain Ctrl+X.
 */
export function formatLiteActivitySummary(
  counts: LiteActivityCounts,
  maxWidth: number,
  separator: string
): string | null {
  const width = Math.max(1, Math.floor(maxWidth));
  const workflows = workflowLabels(
    counts.runningWorkflows,
    counts.pausedWorkflows
  );
  const stepProgress =
    workflows && counts.totalSteps > 0
      ? `steps ${counts.completedSteps}/${counts.totalSteps}`
      : '';
  const queueFull =
    counts.queuedMessages > 0
      ? `${plural(counts.queuedMessages, 'message')} queued`
      : '';
  const queueCompact =
    counts.queuedMessages > 0 ? `${counts.queuedMessages} queued` : '';
  const tasksFull =
    counts.remainingTasks > 0
      ? `${plural(counts.remainingTasks, 'task')} remaining`
      : '';
  const tasksCompact =
    counts.remainingTasks > 0 ? plural(counts.remainingTasks, 'task') : '';
  const completedTasksFull =
    counts.remainingTasks === 0 && counts.completedTasks > 0
      ? `${plural(counts.completedTasks, 'task')} done`
      : '';
  const completedTasksCompact =
    counts.remainingTasks === 0 && counts.completedTasks > 0
      ? plural(counts.completedTasks, 'task')
      : '';
  const hasExpandableActivity =
    workflows !== null ||
    counts.remainingTasks > 0 ||
    counts.completedTasks > 0;
  const expandFull = hasExpandableActivity ? 'ctrl+x expand' : '';
  const expandCompact = hasExpandableActivity ? 'ctrl+x' : '';
  const taskFull = tasksFull || completedTasksFull;
  const taskCompact = tasksCompact || completedTasksCompact;

  const candidates = workflows
    ? [
        [workflows.full, stepProgress, queueFull, taskFull, expandFull],
        [workflows.full, stepProgress, queueCompact, taskCompact, expandFull],
        [workflows.full, stepProgress, queueCompact, expandFull],
        [workflows.full, stepProgress, expandFull],
        [workflows.compact, stepProgress, expandFull],
        [workflows.compact, stepProgress, expandCompact],
        [workflows.terse, stepProgress, expandCompact],
        [workflows.terse, stepProgress],
        ...(stepProgress
          ? [[stepProgress, expandCompact], [stepProgress]]
          : []),
      ]
    : [
        [queueFull, taskFull, expandFull],
        [queueCompact, taskCompact, expandFull],
        [taskFull, expandFull],
        [taskCompact, expandCompact],
        [queueFull],
        [queueCompact],
      ];

  const rows = candidates
    .map((segments) => join(segments, separator))
    .filter((row, index, all) => row.length > 0 && all.indexOf(row) === index);
  if (rows.length === 0) return null;

  const fitting = rows.find((row) => visibleWidth(row) <= width);
  return fitting ?? truncateToWidth(rows.at(-1)!, width, '');
}

export interface LiteActivityRowAllocation {
  workflows: number;
  tasks: number;
}

/**
 * Shares a bounded six-row lite tray between workflows and tasks. Both
 * sections receive at least one row when present, then workflows take up to
 * three rows before spare capacity flows to whichever section still needs it.
 */
export function allocateLiteActivityRows(
  workflowCount: number,
  taskCount: number,
  maxRows: number
): LiteActivityRowAllocation {
  const workflows = Math.max(0, Math.floor(workflowCount));
  const tasks = Math.max(0, Math.floor(taskCount));
  const capacity = Math.max(0, Math.floor(maxRows));

  if (capacity === 0 || (workflows === 0 && tasks === 0)) {
    return { workflows: 0, tasks: 0 };
  }
  if (workflows === 0) {
    return { workflows: 0, tasks: Math.min(tasks, capacity) };
  }
  if (tasks === 0) {
    return { workflows: Math.min(workflows, capacity), tasks: 0 };
  }

  let visibleWorkflows = Math.min(workflows, Math.min(3, capacity - 1));
  const visibleTasks = Math.min(tasks, capacity - visibleWorkflows);
  const spare = capacity - visibleWorkflows - visibleTasks;
  if (spare > 0) {
    visibleWorkflows += Math.min(workflows - visibleWorkflows, spare);
  }

  return { workflows: visibleWorkflows, tasks: visibleTasks };
}
