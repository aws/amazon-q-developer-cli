import React, { useState, useCallback, useEffect } from 'react';
import { Box, Text, useInput } from '../../../renderer.js';
import {
  useTaskState,
  useQueueState,
  useQueueActions,
  useCommandState,
  useProcessingState,
} from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Icon, IconType } from '../icon/Icon.js';

const MAX_VISIBLE_LINES = 6;

type ActiveTab = 'tasks' | 'queue';

export const ActivityTrayExpanded = React.memo(function ActivityTrayExpanded({
  hasTasks,
}: {
  hasTasks: boolean;
}) {
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

  const hasSteer = pendingSteerContent != null;
  const hasQueue = queuedMessages.length > 0;
  const queueCount = queuedMessages.length;
  // The "queue" tab hosts both the steer message (if any) and the queued
  // messages list — show it whenever either is present.
  const hasMessages = hasSteer || hasQueue;

  const [activeTab, setActiveTab] = useState<ActiveTab>(
    hasTasks ? 'tasks' : 'queue'
  );
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Auto-switch tab when the active tab's items disappear
  useEffect(() => {
    if (activeTab === 'queue' && !hasMessages && hasTasks) {
      setActiveTab('tasks');
      setSelectedIndex(0);
    } else if (activeTab === 'tasks' && !hasTasks && hasMessages) {
      setActiveTab('queue');
      setSelectedIndex(0);
    }
  }, [activeTab, hasTasks, hasMessages]);

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
    if (activeTab !== 'queue') return;
    if (queuedMessages.length > 0) {
      removeQueuedMessage(clampedIndex);
      // Cursor adjustment happens automatically via clampedIndex on re-render
    } else if (hasSteer) {
      // No queued messages — delete clears the pending steer instead.
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
        if (hasTasks && hasMessages) {
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
    hints.push(
      `shift+${glyphs.arrowUp}${glyphs.arrowDown} or ctrl+p/n to navigate`
    );
  }
  if (
    activeTab === 'queue' &&
    queuedMessages.length > 0 &&
    editingQueueIndex == null
  ) {
    hints.push('enter to edit');
    hints.push('del to remove');
  } else if (
    activeTab === 'queue' &&
    queuedMessages.length === 0 &&
    hasSteer &&
    editingQueueIndex == null
  ) {
    hints.push('del to remove');
  }
  if (hasTasks && hasMessages && editingQueueIndex == null) {
    hints.push(`tab to view ${activeTab === 'tasks' ? 'messages' : 'tasks'}`);
  }
  const hintText = hints.join(` ${glyphs.smallDot} `);

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
              {hasSteer && `${!allowIcons ? '' : glyphs.executing} Steer`}
              {hasSteer && hasQueue && ` ${glyphs.smallDot} `}
              {hasQueue &&
                `${!allowIcons ? '' : glyphs.diamond} Queue (${queueCount})`}
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

      {/* Steer message (if any) shown above the queued messages list. */}
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
