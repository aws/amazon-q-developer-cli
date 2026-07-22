import React, { useEffect } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { chalk } from '../../utils/color.js';

interface SurveyPromptBarProps {
  message: string;
  shortcutKey?: string;
  shortcutLabel?: string;
  /** Auto-dismiss after this many ms. Omit to persist until user action. */
  autoHideMs?: number;
  onDismiss?: () => void;
}

/**
 * Full-width colored bar for survey prompts. Positioned between the
 * conversation view and the prompt bar (same slot as NotificationBar).
 *
 * Uses a hardcoded blue background (the dark theme's highlight/emphasis
 * color) so it stands out consistently regardless of theme.
 */
export const SurveyPromptBar = React.memo(function SurveyPromptBar({
  message,
  shortcutKey = 'ctrl+y',
  shortcutLabel = 'to rate',
  autoHideMs,
  onDismiss,
}: SurveyPromptBarProps) {
  const { width: termWidth } = useTerminalSize();

  useEffect(() => {
    if (autoHideMs && onDismiss) {
      const timer = setTimeout(onDismiss, autoHideMs);
      return () => clearTimeout(timer);
    }
  }, [autoHideMs, onDismiss]);

  // Hardcoded blue from kiroDark highlight — consistent across themes.
  const BG = '#0087FF';

  return (
    <Box
      width={termWidth}
      backgroundColor={BG}
      paddingX={1}
      justifyContent="space-between"
    >
      <Box flexShrink={1} overflow="hidden">
        <Text wrap="truncate-end">{chalk.white.bold(message)}</Text>
      </Box>
      <Box flexShrink={0}>
        <Text>
          {chalk.white.bold(shortcutKey)}
          {chalk.white(` ${shortcutLabel}`)}
        </Text>
      </Box>
    </Box>
  );
});
