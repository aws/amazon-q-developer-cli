import React, { useState, useCallback, useEffect } from 'react';
import { Box, Text, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useQueueState,
  useQueueActions,
  useCommandState,
  useProcessingState,
} from '../../../stores/selectors.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';

const MAX_VISIBLE_LINES = 6;

type ActiveTab = 'tasks' | 'queue';

interface ActivityTrayExpandedProps {
  queueCount: number;
}

export const ActivityTrayExpanded = React.memo(function ActivityTrayExpanded({
  queueCount,
}: ActivityTrayExpandedProps) {
  const { tasks } = useTaskState();
  const { queuedMessages, editingQueueIndex } = useQueueState();
  const { removeQueuedMessage, startEditingQueue } = useQueueActions();
  const { commandInputValue } = useCommandState();
  const { pendingApproval } = useProcessingState();
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();

  const hasTasks = tasks.length > 0;
  const hasQueue = queueCount > 0;

  const [activeTab, setActiveTab] = useState<ActiveTab>(
    hasTasks ? 'tasks' : 'queue'
  );
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Auto-switch tab when the active tab's items disappear
  useEffect(() => {
    if (activeTab === 'queue' && !hasQueue && hasTasks) {
      setActiveTab('tasks');
      setSelectedIndex(0);
    } else if (activeTab === 'tasks' && !hasTasks && hasQueue) {
      setActiveTab('queue');
      setSelectedIndex(0);
    }
  }, [activeTab, hasTasks, hasQueue]);

  const itemCount =
    activeTab === 'tasks' ? tasks.length : queuedMessages.length;

  // Clamp selected index when items change
  const clampedIndex = Math.min(selectedIndex, Math.max(0, itemCount - 1));

  // Sync local selectedIndex when clamping changes it
  useEffect(() => {
    if (clampedIndex !== selectedIndex) {
      setSelectedIndex(clampedIndex);
    }
  }, [clampedIndex, selectedIndex]);

  const handleRemoveQueued = useCallback(() => {
    if (activeTab !== 'queue' || queuedMessages.length === 0) return;
    removeQueuedMessage(clampedIndex);
    // Cursor adjustment happens automatically via clampedIndex on re-render
  }, [activeTab, queuedMessages.length, clampedIndex, removeQueuedMessage]);

  const handleEditQueued = useCallback(() => {
    if (activeTab !== 'queue' || queuedMessages.length === 0) return;
    if (editingQueueIndex != null) return; // already editing
    startEditingQueue(clampedIndex);
  }, [
    activeTab,
    queuedMessages.length,
    editingQueueIndex,
    clampedIndex,
    startEditingQueue,
  ]);

  // Disable tray keyboard handling when editing a queue item or when an
  // approval is pending (approval UI owns Tab/Enter/arrow keys).
  const isNavigable = editingQueueIndex == null && !pendingApproval;

  useInput(
    (_input, key) => {
      if ((key.shift || key.meta) && key.upArrow) {
        setSelectedIndex((prev) => Math.max(0, prev - 1));
      } else if ((key.shift || key.meta) && key.downArrow) {
        setSelectedIndex((prev) => Math.min(itemCount - 1, prev + 1));
      } else if (_input === 'p' && key.ctrl) {
        // ctrl+p — alternative up navigation for terminals that
        // don't send shift modifier with arrow keys (e.g. Terminal.app)
        setSelectedIndex((prev) => Math.max(0, prev - 1));
      } else if (_input === 'n' && key.ctrl) {
        // ctrl+n — alternative down navigation
        setSelectedIndex((prev) => Math.min(itemCount - 1, prev + 1));
      } else if (key.tab && !key.shift) {
        if (hasTasks && hasQueue) {
          const next = activeTab === 'tasks' ? 'queue' : 'tasks';
          setActiveTab(next);
          setSelectedIndex(0);
        }
      } else if (activeTab === 'queue' && !commandInputValue) {
        // Only capture enter/delete when the prompt input is empty,
        // otherwise these keys belong to PromptInput
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

  // Scroll offset keeps the visible window positioned correctly
  const scrollOffset = (() => {
    if (activeTab === 'tasks') {
      // Tasks: auto-follow the next pending task
      const total = tasks.length;
      if (total <= MAX_VISIBLE_LINES) return 0;
      const nextIndex = tasks.findIndex((t) => t.status !== 'completed');
      const target = nextIndex === -1 ? total - 1 : nextIndex;
      const maxScroll = total - MAX_VISIBLE_LINES;
      return Math.min(maxScroll, Math.max(0, target - 1));
    }
    // Queue: follow the cursor
    if (itemCount <= MAX_VISIBLE_LINES) return 0;
    const maxScroll = itemCount - MAX_VISIBLE_LINES;
    return Math.min(maxScroll, Math.max(0, clampedIndex - 1));
  })();

  // Build contextual action hints
  const hints: string[] = [];
  if (editingQueueIndex != null) {
    hints.push('esc to cancel');
  } else if (activeTab === 'queue' && queuedMessages.length > 1) {
    hints.push('shift+↑↓ or ctrl+p/n to navigate');
  }
  if (
    activeTab === 'queue' &&
    queuedMessages.length > 0 &&
    editingQueueIndex == null
  ) {
    hints.push('enter to edit');
    hints.push('del to remove');
  }
  if (hasTasks && hasQueue && editingQueueIndex == null) {
    hints.push(`tab to view ${activeTab === 'tasks' ? 'queue' : 'tasks'}`);
  }
  const hintText = hints.join(' · ');

  return (
    <Box flexDirection="column" width={termWidth} backgroundColor={bg}>
      {/* Header with tabs */}
      <Box width={termWidth} backgroundColor={bg} paddingX={1}>
        <Box flexGrow={1}>
          {hasTasks && (
            <Text
              backgroundColor={bg}
              color={activeTab === 'tasks' ? fg : mutedHex}
              bold={activeTab === 'tasks'}
            >
              ◐ Tasks ({tasks.length})
            </Text>
          )}
          {hasTasks && hasQueue && (
            <Text backgroundColor={bg} color={mutedHex}>
              {'  '}
            </Text>
          )}
          {hasQueue && (
            <Text
              backgroundColor={bg}
              color={activeTab === 'queue' ? fg : mutedHex}
              bold={activeTab === 'queue'}
            >
              ◇ Queue ({queueCount})
            </Text>
          )}
        </Box>
        <Text backgroundColor={bg} color={fg} dimColor italic>
          ctrl+x to collapse
        </Text>
      </Box>

      {/* Item list */}
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
      {activeTab === 'queue' && (
        <QueueList
          messages={queuedMessages}
          scrollOffset={scrollOffset}
          maxVisible={MAX_VISIBLE_LINES}
          selectedIndex={clampedIndex}
          editingIndex={editingQueueIndex}
          bg={bg}
          fg={fg}
          mutedHex={mutedHex}
          brandHex={brandHex}
          termWidth={termWidth}
        />
      )}

      {/* Action hints */}
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
  const nextIndex = tasks.findIndex((t) => t.status !== 'completed');
  const visible = tasks.slice(scrollOffset, scrollOffset + maxVisible);

  return (
    <>
      {visible.map((task, i) => {
        const globalIndex = scrollOffset + i;
        const isLast = globalIndex === tasks.length - 1;
        const isNext = globalIndex === nextIndex;
        const { icon, color } = getStatusIcon(task.status, isNext, {
          successHex,
          infoHex,
          mutedHex,
        });
        const connector = isLast ? '└──' : '├──';

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
              <Text backgroundColor={bg} color={brandHex}>
                ✎{' '}
              </Text>
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
  colors: { successHex: string; infoHex: string; mutedHex: string | undefined }
): { icon: string; color: string | undefined } {
  if (status === 'completed') {
    return { icon: '●', color: colors.successHex };
  }
  if (isNext) {
    return { icon: '◐', color: colors.infoHex };
  }
  return { icon: '○', color: colors.mutedHex };
}
