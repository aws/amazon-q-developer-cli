import React, { useMemo } from 'react';
import { useStore, type StoreApi } from 'zustand';
import { Box, Text } from '../../../renderer.js';
import { useTaskState } from '../../../stores/selectors.js';
import {
  workflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useAllowIcons, useGlyphs } from '../../../hooks/useGlyphs.js';
import { truncateToWidth } from '../../../utils/text-width.js';
import {
  RUN_STATUS_COLOR_TOKEN,
  runStatusGlyph,
} from '../../layout/workflow-monitor/run-status-style.js';
import { isWorkflowTrayActive } from './tray-tabs.js';
import { collapsedWorkflowSummary } from './tray-view-model.js';

const MAX_COLLAPSED_WORKFLOWS = 3;
const WORKFLOW_NAME_MAX = 18;

export interface ActivityTrayCollapsedProps {
  hasTasks: boolean;
  queuedMessageCount: number;
  store?: StoreApi<WorkflowStoreState>;
}

export const ActivityTrayCollapsed = React.memo(function ActivityTrayCollapsed({
  hasTasks,
  queuedMessageCount,
  store = workflowStore,
}: ActivityTrayCollapsedProps) {
  const { tasks } = useTaskState();
  const workflows = useStore(store, (state) => state.workflows);
  const liveWorkflows = useMemo(
    () =>
      [...workflows.values()].filter((workflow) =>
        isWorkflowTrayActive(workflow.status)
      ),
    [workflows]
  );
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const workflowSummary = useMemo(
    () => collapsedWorkflowSummary(liveWorkflows, MAX_COLLAPSED_WORKFLOWS),
    [liveWorkflows]
  );

  const surface = getColor('surface').hex;
  const backgroundColor = surface === 'inherit' ? undefined : surface;
  const primary = getColor('primary').hex;
  const foregroundColor = primary === 'inherit' ? undefined : primary;
  const muted = getColor('muted').hex;
  const mutedColor = muted === 'inherit' ? undefined : muted;

  const completedTasks = tasks.filter(
    (task) => task.status === 'completed'
  ).length;
  const remainingTasks = tasks.length - completedTasks;
  const segments: React.ReactNode[] = [];

  workflowSummary.entries.forEach((workflow) => {
    const statusColor = getColor(RUN_STATUS_COLOR_TOKEN[workflow.status]).hex;
    segments.push(
      <React.Fragment key={workflow.workflowId}>
        {allowIcons && (
          <Text backgroundColor={backgroundColor} color={statusColor}>
            {runStatusGlyph(workflow.status, glyphs)}{' '}
          </Text>
        )}
        <Text backgroundColor={backgroundColor} color={foregroundColor} bold>
          {truncateToWidth(workflow.name, WORKFLOW_NAME_MAX)}
        </Text>
        <Text backgroundColor={backgroundColor} color={statusColor}>
          {` ${workflow.status}`}
        </Text>
        <Text backgroundColor={backgroundColor} color={mutedColor}>
          {` ${workflow.completedSteps}/${workflow.totalSteps}`}
        </Text>
      </React.Fragment>
    );
  });

  const hiddenWorkflowCount = workflowSummary.hiddenCount;
  if (hiddenWorkflowCount > 0) {
    segments.push(
      <Text
        key="additional-workflows"
        backgroundColor={backgroundColor}
        color={mutedColor}
      >
        +{hiddenWorkflowCount} other{hiddenWorkflowCount === 1 ? '' : 's'}
      </Text>
    );
  }

  if (queuedMessageCount > 0) {
    segments.push(
      <Text
        key="messages"
        backgroundColor={backgroundColor}
        color={foregroundColor}
        bold
      >
        {allowIcons ? `${glyphs.diamond} ` : ''}
        {queuedMessageCount} message{queuedMessageCount === 1 ? '' : 's'} queued
      </Text>
    );
  }

  if (hasTasks) {
    segments.push(
      <Text
        key="tasks"
        backgroundColor={backgroundColor}
        color={foregroundColor}
        bold
      >
        {allowIcons ? `${glyphs.executing} ` : ''}
        {remainingTasks > 0
          ? `${remainingTasks} task${remainingTasks === 1 ? '' : 's'} remaining`
          : `${completedTasks} task${completedTasks === 1 ? '' : 's'} done`}
      </Text>
    );
  }

  return (
    <Box width={termWidth} backgroundColor={backgroundColor} paddingX={1}>
      <Box flexGrow={1} overflow="hidden">
        <Text
          backgroundColor={backgroundColor}
          color={foregroundColor}
          wrap="truncate-end"
        >
          {segments.map((segment, index) => (
            <React.Fragment key={index}>
              {index > 0 && ` ${glyphs.smallDot} `}
              {segment}
            </React.Fragment>
          ))}
          {` ${glyphs.smallDot} ctrl+x expand`}
          {/* N13: surface ctrl+g monitor while a workflow is active (the
              workflow-start chip in WorkflowTool.tsx already advertises it
              transiently; this keeps it always-visible in the tray). */}
          {liveWorkflows.length > 0 && ` ${glyphs.smallDot} ctrl+g monitor`}
        </Text>
      </Box>
    </Box>
  );
});
