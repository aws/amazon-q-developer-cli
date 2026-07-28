import React, { useCallback, useMemo, useState } from 'react';
import { useStore, type StoreApi } from 'zustand';
import { Box, Tabs, Text, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useQueueState,
  useQueueActions,
  useCommandState,
  useProcessingState,
} from '../../../stores/selectors.js';
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
  workflowDigitToIndex,
} from '../../layout/workflow-monitor/workflow-tabs.js';
import { Icon, IconType } from '../icon/Icon.js';
import {
  availableActivityTrayTabs,
  nextActivityTrayTab,
  type ActivityTrayTab,
} from './tray-tabs.js';
import {
  activityTrayHints,
  activityTrayScrollOffset,
} from './tray-view-model.js';

const MAX_VISIBLE_LINES = 6;

export interface ActivityTrayExpandedProps {
  hasTasks: boolean;
  store?: StoreApi<WorkflowStoreState>;
}

export const ActivityTrayExpanded = React.memo(function ActivityTrayExpanded({
  hasTasks,
  store = workflowStore,
}: ActivityTrayExpandedProps) {
  const { tasks } = useTaskState();
  const { pendingSteerContent, queuedMessages, editingQueueIndex } =
    useQueueState();
  const { removeQueuedMessage, startEditingQueue } = useQueueActions();
  const { commandInputValue } = useCommandState();
  const { pendingApproval } = useProcessingState();
  const clearSteerMessage = useAppStore((s) => s.clearSteerMessage);
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const { width: termWidth } = useTerminalSize();
  const workflow = useStore(store, selectActiveWorkflow);
  const workflowSelectedIndex = useStore(store, selectWorkflowNodeIndex);
  const workflows = useStore(store, (state) => state.workflows);
  const activeWorkflowId = useStore(store, (state) => state.activeWorkflowId);
  const historyOpen = useStore(store, (state) => state.history.isOpen);
  const setActiveWorkflow = useStore(store, (state) => state.setActiveWorkflow);
  const setWorkflowSelectedNode = useStore(
    store,
    (state) => state.setSelectedNode
  );

  const hasSteer = pendingSteerContent != null;
  const hasQueue = queuedMessages.length > 0;
  const queueCount = queuedMessages.length;
  const queuedMessageCount = buildUnifiedQueueEntries(
    pendingSteerContent,
    queuedMessages
  ).length;
  const hasMessages = queuedMessageCount > 0;
  const hasWorkflow = workflow !== null;
  const workflowList = useMemo(() => [...workflows.values()], [workflows]);
  const workflowIds = useMemo(
    () => workflowList.map((item) => item.workflowId),
    [workflowList]
  );
  const tabs = useMemo(
    () => availableActivityTrayTabs({ hasTasks, hasMessages, hasWorkflow }),
    [hasMessages, hasTasks, hasWorkflow]
  );
  const [requestedTab, setActiveTab] = useState<ActivityTrayTab>(
    hasWorkflow ? 'workflow' : (tabs[0] ?? 'tasks')
  );
  const activeTab = tabs.includes(requestedTab)
    ? requestedTab
    : hasWorkflow
      ? 'workflow'
      : (tabs[0] ?? 'tasks');
  const [selectedIndex, setSelectedIndex] = useState(0);

  const itemCount =
    activeTab === 'tasks'
      ? tasks.length
      : activeTab === 'queue'
        ? queuedMessages.length
        : 0;
  const maxItemIndex = Math.max(0, itemCount - 1);
  const clampedIndex = Math.max(0, Math.min(selectedIndex, maxItemIndex));

  const handleRemoveQueued = useCallback(() => {
    if (activeTab !== 'queue') return;
    if (queuedMessages.length > 0) {
      removeQueuedMessage(clampedIndex);
    } else if (hasSteer) {
      clearSteerMessage();
    }
  }, [
    activeTab,
    queuedMessages.length,
    clampedIndex,
    removeQueuedMessage,
    hasSteer,
    clearSteerMessage,
  ]);

  const handleEditQueued = useCallback(() => {
    if (activeTab !== 'queue' || queuedMessages.length === 0) return;
    if (editingQueueIndex != null) return;
    startEditingQueue(clampedIndex);
  }, [
    activeTab,
    queuedMessages.length,
    editingQueueIndex,
    clampedIndex,
    startEditingQueue,
  ]);

  const isNavigable =
    editingQueueIndex == null && !pendingApproval && !historyOpen;

  useInput(
    (input, key) => {
      if (activeTab === 'workflow' && workflow) {
        if (key.leftArrow || key.rightArrow) {
          const adjacentId = adjacentWorkflowId(
            workflowIds,
            activeWorkflowId,
            key.rightArrow ? 1 : -1
          );
          if (adjacentId) setActiveWorkflow(adjacentId);
          return;
        }
        const workflowIndex = workflowDigitToIndex(input, workflowIds.length);
        if (!key.ctrl && !key.meta && workflowIndex !== null) {
          const workflowId = workflowIds[workflowIndex];
          if (workflowId) setActiveWorkflow(workflowId);
          return;
        }
        if (key.upArrow || (!key.ctrl && !key.meta && input === 'k')) {
          setWorkflowSelectedNode(workflowSelectedIndex - 1);
          return;
        }
        if (key.downArrow || (!key.ctrl && !key.meta && input === 'j')) {
          setWorkflowSelectedNode(workflowSelectedIndex + 1);
          return;
        }
      }

      if ((key.shift || key.meta) && key.upArrow) {
        setSelectedIndex(Math.max(0, clampedIndex - 1));
      } else if ((key.shift || key.meta) && key.downArrow) {
        setSelectedIndex(Math.min(maxItemIndex, clampedIndex + 1));
      } else if (input === 'p' && key.ctrl) {
        setSelectedIndex(Math.max(0, clampedIndex - 1));
      } else if (input === 'n' && key.ctrl) {
        setSelectedIndex(Math.min(maxItemIndex, clampedIndex + 1));
      } else if (key.tab && !key.shift && !key.ctrl) {
        if (tabs.length > 1) {
          setActiveTab(nextActivityTrayTab(tabs, activeTab));
          setSelectedIndex(0);
        }
      } else if (activeTab === 'queue' && !commandInputValue) {
        if (key.delete || key.backspace) {
          handleRemoveQueued();
        } else if (key.return) {
          handleEditQueued();
        }
      }
    },
    { isActive: isNavigable }
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
            showIndexes
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
              {!allowIcons ? '' : glyphs.executing} Workflow (
              {progress.completed}/{progress.total})
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
            <Text
              backgroundColor={bg}
              color={task.status === 'completed' ? mutedHex : fg}
              strikethrough={task.status === 'completed'}
            >
              {task.subject}
            </Text>
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
