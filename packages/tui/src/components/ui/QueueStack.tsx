import React from 'react';
import { Box, Text } from 'ink';
import { useQueueState, useConversationState } from '../../stores/selectors.js';
import { useUIState } from '../../stores/selectors.js';
import { useTheme } from '../../hooks/useThemeContext.js';

function QueueRow({
  index,
  total,
  message,
  showHint,
}: {
  index: number;
  total: number;
  message: string;
  showHint?: boolean;
}) {
  const { getColor } = useTheme();
  const primaryHex = getColor('primary').hex;
  const mutedHex = getColor('muted').hex;

  return (
    <Box width="100%" backgroundColor={mutedHex} paddingX={1}>
      <Box flexGrow={1} overflow="hidden">
        <Text color={primaryHex} wrap="truncate-end">
          <Text bold>
            [{index + 1}/{total}] Queued:
          </Text>{' '}
          {message}
        </Text>
      </Box>
      {showHint && (
        <Text color={primaryHex} dimColor italic>
          {' '}
          ctrl+o to expand
        </Text>
      )}
    </Box>
  );
}

/**
 * Compact stacked queue indicator shown above the active conversation turn.
 *
 * Collapsed (default): shows only the next queued message with expand hint.
 * Expanded (Ctrl+O): shows all queued messages.
 *
 * Uses the unified toolOutputsExpanded state so queue and tool outputs
 * share a single expand/collapse toggle.
 */
export const QueueStack = React.memo(function QueueStack() {
  const { queuedMessages } = useQueueState();
  const { toolOutputsExpanded } = useUIState();
  const { isProcessing } = useConversationState();

  if (queuedMessages.length === 0) return null;

  const total = queuedMessages.length;

  // Force collapsed view while streaming to avoid layout thrashing / flicker
  if (!toolOutputsExpanded || isProcessing) {
    return (
      <Box marginTop={1}>
        <QueueRow
          index={0}
          total={total}
          message={queuedMessages[0]!}
          showHint={total > 1 && !isProcessing}
        />
      </Box>
    );
  }

  return (
    <Box flexDirection="column" marginTop={1}>
      {queuedMessages.map((msg, i) => (
        <QueueRow key={`${i}-${msg}`} index={i} total={total} message={msg} />
      ))}
    </Box>
  );
});
