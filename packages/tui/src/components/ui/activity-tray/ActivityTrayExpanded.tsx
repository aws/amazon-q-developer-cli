import React, { useMemo } from 'react';
import { useStore, type StoreApi } from 'zustand';
import { Box, Tabs, Text, useInput } from '../../../renderer.js';
import { TASK_DONE_MARKER } from '../../../constants/tasks.js';
import { wrapAtWords } from '../../../lite/render.js';
import { visibleWidth } from '../../../utils/text-width.js';
import { useTaskState, useQueueState } from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import {
  selectActiveWorkflow,
  selectWorkflowNodeIndex,
  workflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import { workflowProgress } from '../../../stores/workflow-view-model.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { buildUnifiedQueueEntries } from '../../../utils/queue-navigation.js';
import { WorkflowDagView } from '../../layout/workflow-monitor/WorkflowDagView.js';
import {
  adjacentWorkflowId,
  buildWorkflowTabs,
} from '../../layout/workflow-monitor/workflow-tabs.js';
import { Icon, IconType } from '../icon/Icon.js';
import { nextActivityTrayTab, type ActivityTrayTab } from './tray-tabs.js';
import {
  activityTrayHints,
  activityTrayScrollOffset,
} from './tray-view-model.js';
import {
  resolveActivityTrayInputAction,
  type ActivityTrayInputOwnership,
} from './input-ownership.js';
import { useActivityTrayInputGateReader } from './useActivityTrayModel.js';

const MAX_VISIBLE_LINES = 6;

export interface ActivityTrayExpandedProps {
  activeTab: ActivityTrayTab | null;
  hasTasks: boolean;
  inputOwnership: ActivityTrayInputOwnership;
  navigationActive: boolean;
  store?: StoreApi<WorkflowStoreState>;
  tabs: readonly ActivityTrayTab[];
}

export const ActivityTrayExpanded = React.memo(function ActivityTrayExpanded({
  activeTab,
  hasTasks,
  inputOwnership,
  navigationActive,
  store = workflowStore,
  tabs,
}: ActivityTrayExpandedProps) {
  const { tasks } = useTaskState();
  const { pendingSteerContent, queuedMessages, editingQueueIndex } =
    useQueueState();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const { width: termWidth } = useTerminalSize();
  const workflow = useStore(store, selectActiveWorkflow);
  const workflowSelectedIndex = useStore(store, selectWorkflowNodeIndex);
  const workflows = useStore(store, (state) => state.workflows);
  const activeWorkflowId = useStore(store, (state) => state.activeWorkflowId);
  const setActivityTrayTab = useAppStore((state) => state.setActivityTrayTab);
  const selectedIndex = useAppStore((state) => state.activityTraySelectedIndex);
  const setSelectedIndex = useAppStore(
    (state) => state.setActivityTraySelectedIndex
  );
  const setActiveWorkflow = useStore(store, (state) => state.setActiveWorkflow);
  const setWorkflowSelectedNode = useStore(
    store,
    (state) => state.setSelectedNode
  );
  const readInputGate = useActivityTrayInputGateReader();

  const hasSteer = pendingSteerContent != null;
  const hasQueue = queuedMessages.length > 0;
  const queueCount = queuedMessages.length;
  const queuedMessageCount = buildUnifiedQueueEntries(
    pendingSteerContent,
    queuedMessages
  ).length;
  const hasMessages = tabs.includes('queue');
  const hasWorkflow = tabs.includes('workflow');
  const workflowList = useMemo(() => [...workflows.values()], [workflows]);
  const workflowIds = useMemo(
    () => workflowList.map((item) => item.workflowId),
    [workflowList]
  );
  const itemCount =
    activeTab === 'tasks'
      ? tasks.length
      : activeTab === 'queue'
        ? queuedMessages.length
        : 0;
  const maxItemIndex = Math.max(0, itemCount - 1);
  const clampedIndex = Math.max(0, Math.min(selectedIndex, maxItemIndex));

  useInput(
    (input, key) => {
      if (!readInputGate().inputEnabled) return;

      const action = resolveActivityTrayInputAction(input, key, inputOwnership);
      switch (action) {
        case 'previous-workflow':
        case 'next-workflow': {
          const adjacentId = adjacentWorkflowId(
            workflowIds,
            activeWorkflowId,
            action === 'next-workflow' ? 1 : -1
          );
          if (adjacentId) setActiveWorkflow(adjacentId);
          return;
        }
        case 'previous-workflow-node':
          setWorkflowSelectedNode(workflowSelectedIndex - 1);
          return;
        case 'next-workflow-node':
          setWorkflowSelectedNode(workflowSelectedIndex + 1);
          return;
        case 'previous-row':
          setSelectedIndex(Math.max(0, clampedIndex - 1));
          return;
        case 'next-row':
          setSelectedIndex(Math.min(maxItemIndex, clampedIndex + 1));
          return;
        case 'next-tab':
          if (activeTab) {
            setActivityTrayTab(nextActivityTrayTab(tabs, activeTab));
          }
          setSelectedIndex(0);
          return;
      }
    },
    { isActive: navigationActive }
  );

  const rawBg = getColor('surface').hex;
  const bg = rawBg === 'inherit' ? undefined : rawBg;
  // Guard against 'inherit' from named:'default' — when backgroundColor is
  // explicitly set, Ink needs a real color value or undefined (terminal default).
  const rawFg = getColor('primary').hex;
  const fg = rawFg === 'inherit' ? undefined : rawFg;
  const successHex = getColor('success').hex;
  const infoHex = getColor('info').hex;
  const rawMuted = getColor('muted').hex;
  const mutedHex = rawMuted === 'inherit' ? undefined : rawMuted;
  const brandHex = getColor('brand').hex;

  const scrollOffset = activityTrayScrollOffset({
    activeTab,
    itemCount,
    taskStatuses: tasks.map((task) => task.status),
    selectedIndex: clampedIndex,
    maxVisible: MAX_VISIBLE_LINES,
  });
  const hintText = activityTrayHints({
    activeTab,
    queueCount,
    hasSteer,
    editing: editingQueueIndex != null,
    workflowNodeCount: workflow?.nodes.length ?? 0,
    workflowCount: workflowList.length,
    tabCount: tabs.length,
    arrows: `${glyphs.arrowUp}${glyphs.arrowDown}`,
  }).join(` ${glyphs.smallDot} `);
  const workflowTabDescriptors = useMemo(
    () =>
      buildWorkflowTabs(workflowList, glyphs, (token) => getColor(token).hex),
    [getColor, glyphs, workflowList]
  );
  const progress = workflowProgress(workflow?.nodes ?? []);

  return (
    <Box flexDirection="column" width={termWidth} backgroundColor={bg}>
      {activeTab === 'workflow' &&
        workflowTabDescriptors.length > 1 &&
        activeWorkflowId && (
          <Tabs
            tabs={workflowTabDescriptors}
            activeId={activeWorkflowId}
            onActivate={setActiveWorkflow}
            width={termWidth}
            activeColor={getColor('brand').hex}
            inactiveColor={getColor('secondary').hex}
            borderColor={getColor('secondary').hex}
          />
        )}

      <Box width={termWidth} backgroundColor={bg} paddingX={1}>
        <Box flexGrow={1}>
          {hasTasks && (
            <Text
              backgroundColor={bg}
              color={activeTab === 'tasks' ? fg : mutedHex}
              bold={activeTab === 'tasks'}
            >
              {!allowIcons ? '' : glyphs.executing} Tasks ({tasks.length})
            </Text>
          )}
          {hasTasks && hasMessages && (
            <Text backgroundColor={bg} color={mutedHex}>
              {'  '}
            </Text>
          )}
          {hasMessages && (
            <Text
              backgroundColor={bg}
              color={activeTab === 'queue' ? fg : mutedHex}
              bold={activeTab === 'queue'}
            >
              {!allowIcons ? '' : glyphs.diamond} Messages ({queuedMessageCount}
              )
            </Text>
          )}
          {hasWorkflow && (hasTasks || hasMessages) && (
            <Text backgroundColor={bg} color={mutedHex}>
              {'  '}
            </Text>
          )}
          {workflow && (
            <Text
              backgroundColor={bg}
              color={activeTab === 'workflow' ? fg : mutedHex}
              bold={activeTab === 'workflow'}
            >
              {!allowIcons ? '' : glyphs.executing}{' '}
              {workflow.name?.trim() || 'Workflow'} ({progress.completed}/
              {progress.total})
            </Text>
          )}
        </Box>
      </Box>

      {activeTab === 'tasks' && (
        <TaskList
          tasks={tasks}
          scrollOffset={scrollOffset}
          maxVisible={MAX_VISIBLE_LINES}
          bg={bg}
          fg={fg}
          successHex={successHex}
          infoHex={infoHex}
          mutedHex={mutedHex}
          termWidth={termWidth}
        />
      )}

      {activeTab === 'queue' && pendingSteerContent && (
        <Box width={termWidth} backgroundColor={bg} paddingX={1}>
          <Text backgroundColor={bg} color={brandHex}>
            {allowIcons ? `${glyphs.executing} ` : '> '}
          </Text>
          <Text backgroundColor={bg} color={fg} wrap="truncate-end">
            {pendingSteerContent}
          </Text>
        </Box>
      )}
      {activeTab === 'queue' && hasQueue && (
        <QueueList
          messages={queuedMessages}
          scrollOffset={scrollOffset}
          maxVisible={MAX_VISIBLE_LINES}
          selectedIndex={clampedIndex}
          editingIndex={editingQueueIndex}
          bg={bg}
          fg={fg}
          mutedHex={mutedHex ?? ''}
          brandHex={brandHex}
          termWidth={termWidth}
        />
      )}

      {activeTab === 'workflow' && workflow && (
        <Box paddingX={1}>
          <WorkflowDagView
            nodes={workflow.nodes}
            selectedIndex={workflowSelectedIndex}
            width={Math.max(1, termWidth - 2)}
            height={MAX_VISIBLE_LINES + 2}
          />
        </Box>
      )}

      {hintText && (
        <Box width={termWidth} backgroundColor={bg} paddingX={1}>
          <Text backgroundColor={bg} color={mutedHex} italic>
            {hintText}
          </Text>
        </Box>
      )}
    </Box>
  );
});

// --- Task list sub-component ---

interface TaskListProps {
  tasks: Array<{
    id: string;
    subject: string;
    status: 'pending' | 'completed';
  }>;
  scrollOffset: number;
  maxVisible: number;
  bg: string | undefined;
  fg: string | undefined;
  successHex: string;
  infoHex: string;
  mutedHex: string | undefined;
  termWidth: number;
}

function TaskList({
  tasks,
  scrollOffset,
  maxVisible,
  bg,
  fg,
  successHex,
  infoHex,
  mutedHex,
  termWidth,
}: TaskListProps) {
  const glyphs = useGlyphs();
  const { getColor } = useTheme();
  const { allowIcons } = useAllowIcons();
  const nextIndex = tasks.findIndex((t) => t.status !== 'completed');
  const visible = tasks.slice(scrollOffset, scrollOffset + maxVisible);

  return (
    <>
      {visible.map((task, i) => {
        const globalIndex = scrollOffset + i;
        const isLast = globalIndex === tasks.length - 1;
        const isNext = globalIndex === nextIndex;
        const { icon, color } = getStatusIcon(
          task.status,
          isNext,
          {
            successHex,
            infoHex,
            mutedHex,
          },
          glyphs,
          allowIcons
        );
        const connector = isLast ? glyphs.treeCorner : glyphs.treeBranch;
        const subject =
          task.status === 'completed'
            ? doneSubject(
                task.subject,
                termWidth -
                  2 -
                  visibleWidth(connector) -
                  visibleWidth(icon) -
                  2 -
                  visibleWidth(task.id) -
                  2,
                getColor('muted')
              )
            : getColor('primary')(task.subject);

        return (
          <Box
            key={task.id}
            width={termWidth}
            backgroundColor={bg}
            paddingX={1}
          >
            <Text backgroundColor={bg} dimColor>
              {connector}
            </Text>
            <Text backgroundColor={bg} color={color}>
              {' '}
              {icon}{' '}
            </Text>
            <Text backgroundColor={bg} color={fg}>
              {task.id}.{' '}
            </Text>
            <Text backgroundColor={bg}>{subject}</Text>
          </Box>
        );
      })}
    </>
  );
}

// --- Queue list sub-component ---

interface QueueListProps {
  messages: string[];
  scrollOffset: number;
  maxVisible: number;
  selectedIndex: number;
  editingIndex: number | null;
  bg: string | undefined;
  fg: string | undefined;
  mutedHex: string;
  brandHex: string;
  termWidth: number;
}

function QueueList({
  messages,
  scrollOffset,
  maxVisible,
  selectedIndex,
  editingIndex,
  bg,
  fg,
  mutedHex,
  brandHex,
  termWidth,
}: QueueListProps) {
  const { getColor } = useTheme();
  const visible = messages.slice(scrollOffset, scrollOffset + maxVisible);

  return (
    <>
      {visible.map((msg, i) => {
        const globalIndex = scrollOffset + i;
        const isSelected = globalIndex === selectedIndex;
        const isEditing = globalIndex === editingIndex;

        return (
          <Box
            key={`q-${globalIndex}`}
            width={termWidth}
            backgroundColor={bg}
            paddingX={1}
          >
            <Text backgroundColor={bg} color={isSelected ? brandHex : mutedHex}>
              {isSelected ? '>' : ' '}{' '}
            </Text>
            {isEditing && (
              <>
                <Icon type={IconType.PENCIL} color={getColor('brand')} />
                <Text backgroundColor={bg}> </Text>
              </>
            )}
            <Text
              backgroundColor={bg}
              color={isEditing ? mutedHex : fg}
              wrap="truncate-end"
            >
              {globalIndex + 1}. {msg}
            </Text>
          </Box>
        );
      })}
    </>
  );
}

// --- Helpers ---

/**
 * Strike the subject through and trail it with the completion marker, wrapped
 * so the marker shares the subject's last row: its width comes out of every
 * row's budget, or the renderer floats it onto a row of its own. The marker
 * itself stays out of the strikethrough — struck through, the word reads as
 * retracted rather than as the state it reports.
 *
 * The floor keeps a pane too narrow for the marker from wrapping the subject
 * one column at a time, which would grow the row without bound; the row
 * overflows instead.
 */
function doneSubject(
  subject: string,
  width: number,
  muted: { (s: string): string; strikethrough(s: string): string }
): string {
  const budget = Math.max(20, width - TASK_DONE_MARKER.length - 1);
  const lines = wrapAtWords(subject, budget, budget);
  return lines
    .map(
      (line, index) =>
        muted.strikethrough(line) +
        (index === lines.length - 1 ? ` ${muted(TASK_DONE_MARKER)}` : '')
    )
    .join('\n');
}

function getStatusIcon(
  status: 'pending' | 'completed',
  isNext: boolean,
  colors: { successHex: string; infoHex: string; mutedHex: string | undefined },
  icons: { dotFilled: string; executing: string; dotEmpty: string },
  allowIcons: boolean
): { icon: string; color: string | undefined } {
  if (!allowIcons) return { icon: '', color: undefined };
  if (status === 'completed') {
    return { icon: icons.dotFilled, color: colors.successHex };
  }
  if (isNext) {
    return { icon: icons.executing, color: colors.infoHex };
  }
  return { icon: icons.dotEmpty, color: colors.mutedHex };
}
