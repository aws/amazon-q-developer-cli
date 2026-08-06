import React from 'react';
import { Box } from '../../renderer.js';
import { Text } from '../ui/text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';

/**
 * #12 / #14: A pending tool approval (or question) is global state, but the
 * full {@link ApprovalRequest} overlay is only rendered inline. While the user
 * is in a monitor view (workflow / crew / session), a permission prompt raised
 * by the main session would otherwise sit invisibly in the inline layout with
 * no indication it needs an answer.
 *
 * This banner is hoisted above the mode switch so it stays pinned across every
 * monitor view — it never scrolls out with the output pane — and tells the user
 * how to reach the prompt (`q`/`Esc` returns to chat where the overlay lives).
 * Inline mode already renders the real overlay, so we render nothing there.
 */
export const MonitorApprovalBanner = React.memo(
  function MonitorApprovalBanner() {
    const { getColor } = useTheme();
    const glyphs = useGlyphs();
    const { allowIcons } = useAllowIcons();
    const mode = useAppStore((state) => state.mode);
    const pendingApproval = useAppStore((state) => state.pendingApproval);
    const pendingQuestion = useAppStore((state) => state.pendingQuestion);

    const waiting = !!pendingApproval || !!pendingQuestion;
    if (!waiting || mode === 'inline') return null;

    const label = pendingQuestion
      ? 'The agent is asking a question'
      : 'The agent needs permission to run a tool';

    return (
      <Box paddingX={1}>
        <Text wrap="truncate">
          {getColor('warning').bold(
            `${allowIcons ? `${glyphs.warning} ` : ''}${label} ${glyphs.lineVertical} press q or Esc to return to chat and respond`
          )}
        </Text>
      </Box>
    );
  }
);
