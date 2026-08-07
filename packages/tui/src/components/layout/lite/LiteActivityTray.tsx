import React, { useEffect, useMemo } from 'react';
import type { StoreApi } from 'zustand';
import { useStore } from 'zustand';
import { Box, Text } from '../../../renderer.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
import { wrapAtWords } from '../../../lite/render.js';
import { useAppStore } from '../../../stores/app-store.js';
import {
  workflowActivitySummary,
  workflowProgress,
} from '../../../stores/workflow-view-model.js';
import {
  workflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import { isLiveWorkflowStatus } from '../../../types/workflow-status.js';
import type { TaskItem } from '../../../types/tasks.js';
import type { WorkflowRunView } from '../../../types/workflow-monitor.js';
import { buildUnifiedQueueEntries } from '../../../utils/queue-navigation.js';
import { truncateToWidth, visibleWidth } from '../../../utils/text-width.js';
import {
  RUN_STATUS_COLOR_TOKEN,
  runStatusGlyph,
} from '../workflow-monitor/run-status-style.js';
import {
  allocateLiteActivityRows,
  formatLiteActivitySummary,
} from './activity-summary.js';

const MAX_VISIBLE_ROWS = 6;
const MAX_WORKFLOW_NAME_WIDTH = 32;

interface LiteActivityTrayProps {
  store?: StoreApi<WorkflowStoreState>;
}

export const LiteActivityTray = React.memo(function LiteActivityTray({
  store = workflowStore,
}: LiteActivityTrayProps): React.ReactElement | null {
  const workflows = useStore(store, (state) => state.workflows);
  const setWorkflowSurfaceOpen = useStore(
    store,
    (state) => state.setWorkflowSurfaceOpen
  );
  const pendingSteerContent = useAppStore((state) => state.pendingSteerContent);
  const queuedMessages = useAppStore((state) => state.queuedMessages);
  const tasks = useAppStore((state) => state.tasks);
  const expanded = useAppStore((state) => state.activityTrayExpanded);
  const { width } = useTerminalSize();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const { getColor } = useTheme();

  const { showTasks } = useVerboseDisplay();
  const visibleTasks = showTasks ? tasks : [];
  const liveWorkflows = useMemo(
    () =>
      Array.from(workflows.values()).filter((workflow) =>
        isLiveWorkflowStatus(workflow.status)
      ),
    [workflows]
  );
  const displayedWorkflows = expanded
    ? Array.from(workflows.values())
    : liveWorkflows;
  const workflowSummary = useMemo(
    () => workflowActivitySummary(liveWorkflows),
    [liveWorkflows]
  );
  const queuedMessageCount = useMemo(
    () => buildUnifiedQueueEntries(pendingSteerContent, queuedMessages).length,
    [pendingSteerContent, queuedMessages]
  );
  const remainingTaskCount = visibleTasks.filter(
    (task) => task.status === 'pending'
  ).length;
  const completedTaskCount = visibleTasks.length - remainingTaskCount;
  const traySurfaceOpen = expanded && workflows.size > 0;

  useEffect(() => {
    setWorkflowSurfaceOpen('tray', traySurfaceOpen);
    return () => {
      if (traySurfaceOpen) setWorkflowSurfaceOpen('tray', false);
    };
  }, [setWorkflowSurfaceOpen, traySurfaceOpen]);

  const prefix = `  ${glyphs.dotFilled} `;
  const summary = formatLiteActivitySummary(
    {
      runningWorkflows: workflowSummary.running,
      pausedWorkflows: workflowSummary.paused,
      completedSteps: workflowSummary.completedSteps,
      totalSteps: workflowSummary.totalSteps,
      queuedMessages: queuedMessageCount,
      remainingTasks: remainingTaskCount,
      completedTasks: completedTaskCount,
    },
    width - visibleWidth(prefix),
    ` ${glyphs.smallDot} `,
    `${glyphs.arrowUp} to edit`
  );

  if (!expanded) {
    if (summary === null) return null;
    const actionMarker = ` ${glyphs.smallDot} ctrl+x`;
    const actionIndex = summary.indexOf(actionMarker);
    const activity =
      actionIndex === -1 ? summary : summary.slice(0, actionIndex);
    const actions = actionIndex === -1 ? '' : summary.slice(actionIndex);
    const indicatorColor =
      workflowSummary.running > 0
        ? getColor('info')
        : workflowSummary.paused > 0
          ? getColor('warning')
          : getColor('secondary');

    return (
      <Text wrap="truncate-end">
        {'  '}
        {indicatorColor(glyphs.dotFilled)} {getColor('primary')(activity)}
        {actions && getColor('secondary')(actions)}
      </Text>
    );
  }

  if (displayedWorkflows.length === 0 && visibleTasks.length === 0) {
    return summary === null ? null : <Text>{`  ${summary}`}</Text>;
  }

  const allocation = allocateLiteActivityRows(
    displayedWorkflows.length,
    visibleTasks.length,
    MAX_VISIBLE_ROWS
  );
  const workflowRows = displayedWorkflows.slice(0, allocation.workflows);
  const taskWindow = selectTaskWindow(visibleTasks, allocation.tasks);
  const taskRows = visibleTasks.slice(
    taskWindow.offset,
    taskWindow.offset + allocation.tasks
  );
  const separator = ` ${glyphs.smallDot} `;

  return (
    <Box flexDirection="column">
      <Text>
        {'  '}
        {getColor('primary').bold('activity')}
        {getColor('secondary')(`${separator}ctrl+x collapse`)}
      </Text>
      {workflowRows.length > 0 && (
        <>
          <Text>
            {'  '}
            {getColor('primary').bold(
              `workflows (${displayedWorkflows.length})`
            )}
          </Text>
          {workflowRows.map((workflow, index) => (
            <WorkflowRow
              key={workflow.workflowId}
              workflow={workflow}
              isLast={
                index === workflowRows.length - 1 &&
                displayedWorkflows.length === workflowRows.length
              }
              width={width}
              allowIcons={allowIcons}
            />
          ))}
          {displayedWorkflows.length > workflowRows.length && (
            <Text>
              {getColor('secondary')(
                `  ${glyphs.smallDot} +${displayedWorkflows.length - workflowRows.length} workflows`
              )}
            </Text>
          )}
        </>
      )}
      {taskRows.length > 0 && (
        <>
          <Text>
            {'  '}
            {getColor('primary').bold(`tasks (${visibleTasks.length})`)}
          </Text>
          {taskWindow.offset > 0 && (
            <Text>
              {getColor('secondary')(
                `  ${glyphs.smallDot} ${taskWindow.offset} earlier`
              )}
            </Text>
          )}
          {taskRows.map((task, index) => (
            <TaskRow
              key={task.id}
              task={task}
              isNext={
                taskWindow.offset + index ===
                visibleTasks.findIndex(
                  (candidate) => candidate.status !== 'completed'
                )
              }
              isLast={
                taskWindow.offset + index === visibleTasks.length - 1 &&
                taskWindow.offset + taskRows.length === visibleTasks.length
              }
              width={width}
              allowIcons={allowIcons}
            />
          ))}
          {taskWindow.offset + taskRows.length < visibleTasks.length && (
            <Text>
              {getColor('secondary')(
                `  ${glyphs.smallDot} +${visibleTasks.length - taskWindow.offset - taskRows.length} tasks`
              )}
            </Text>
          )}
        </>
      )}
      {displayedWorkflows.length > 0 && (
        <Text>
          {getColor('secondary')(`  ctrl+g monitor${separator}/workflow list`)}
        </Text>
      )}
    </Box>
  );
});

interface WorkflowRowProps {
  workflow: WorkflowRunView;
  isLast: boolean;
  width: number;
  allowIcons: boolean;
}

function WorkflowRow({
  workflow,
  isLast,
  width,
  allowIcons,
}: WorkflowRowProps): React.ReactElement {
  const glyphs = useGlyphs();
  const { getColor } = useTheme();
  const progress = workflowProgress(workflow.nodes);
  const connector = isLast ? glyphs.treeCorner : glyphs.treeBranch;
  const icon = allowIcons ? `${runStatusGlyph(workflow.status, glyphs)} ` : '';
  const statusColor = getColor(RUN_STATUS_COLOR_TOKEN[workflow.status]);
  const suffix = ` ${glyphs.smallDot} ${workflow.status} ${glyphs.smallDot} steps ${progress.completed}/${progress.total}`;
  const nameWidth = Math.max(
    8,
    Math.min(MAX_WORKFLOW_NAME_WIDTH, width - visibleWidth(suffix) - 9)
  );
  const name = truncateToWidth(workflow.name, nameWidth, glyphs.ellipsis);

  return (
    <Text wrap="truncate-end">
      {'  '}
      {getColor('secondary')(connector)} {statusColor(icon)}
      {getColor('primary').bold(name)}
      {getColor('secondary')(suffix)}
    </Text>
  );
}

interface TaskRowProps {
  task: TaskItem;
  isNext: boolean;
  isLast: boolean;
  width: number;
  allowIcons: boolean;
}

function TaskRow({
  task,
  isNext,
  isLast,
  width,
  allowIcons,
}: TaskRowProps): React.ReactElement {
  const glyphs = useGlyphs();
  const { getColor } = useTheme();
  const connector = isLast ? glyphs.treeCorner : glyphs.treeBranch;
  const icon = !allowIcons
    ? ' '
    : task.status === 'completed'
      ? getColor('success')(glyphs.checkmark)
      : isNext
        ? getColor('info')(glyphs.arrowRight)
        : getColor('secondary')(glyphs.dotEmpty);
  const styleLine = (line: string): string => {
    if (task.status === 'completed') {
      return getColor('secondary').strikethrough(line);
    }
    return isNext ? getColor('primary').bold(line) : getColor('primary')(line);
  };
  const prefixWidth = 9 + visibleWidth(task.id);
  const availableWidth = Math.max(20, width - prefixWidth);
  const wrapped = wrapAtWords(task.subject, availableWidth, availableWidth);
  const indent = ' '.repeat(prefixWidth);
  const head = `  ${getColor('secondary')(connector)} ${icon} ${getColor('secondary')(`${task.id}.`)} ${styleLine(wrapped[0] ?? '')}`;
  const tail = wrapped.slice(1).map((line) => `${indent}${styleLine(line)}`);

  return (
    <Text wrap="overflow">
      {tail.length === 0 ? head : `${head}\n${tail.join('\n')}`}
    </Text>
  );
}

function selectTaskWindow(
  tasks: readonly TaskItem[],
  maxVisible: number
): { offset: number } {
  if (maxVisible <= 0 || tasks.length <= maxVisible) return { offset: 0 };
  const nextIndex = tasks.findIndex((task) => task.status !== 'completed');
  const target = nextIndex === -1 ? tasks.length - 1 : nextIndex;
  return {
    offset: Math.min(tasks.length - maxVisible, Math.max(0, target - 1)),
  };
}
