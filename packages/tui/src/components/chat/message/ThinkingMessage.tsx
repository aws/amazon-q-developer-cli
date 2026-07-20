import React, { useState, useEffect, useRef } from 'react';
import { Box } from './../../../renderer.js';
import {
  StatusBar,
  STATUS_BAR_CONTENT_OFFSET,
} from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useAppStore } from '../../../stores/app-store.js';
import { useThinkingMode, useGlyphs } from '../../../hooks/useGlyphs.js';
import { getComfortMessage } from './comfort-messages.js';
import { useThinkingTip } from './useThinkingTip.js';

interface ThinkingMessageProps {
  barColor?: string;
  /**
   * Whether to show a tip below the spinner. Defaults to false so that
   * secondary mount sites (subagent panels in SessionOutput) don't display
   * tips or schedule timers. Only the main ConversationView mount passes true.
   */
  showTip?: boolean;
}

/**
 * Inline thinking indicator with tiered comfort messaging.
 *
 * When `showTip` is true, a tip line appears below the spinner after a short
 * delay, reusing the shared tips engine. One tip per mount, no rotation.
 * When false (default), no timer is created and no tip logic runs.
 *
 * When an HTTP retry is in progress, a line is appended with the attempt
 * counter and countdown text.
 */
export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({
  barColor,
  showTip = false,
}) => {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');
  const dim = getColor('muted');
  const warning = getColor('warning');
  const keybindings = useKeybindings();
  const retryStatus = useAppStore((s) => s.retryStatus);
  const { thinkingMode } = useThinkingMode();
  const thinkingEnabled = thinkingMode !== 'off';
  const glyphs = useGlyphs();

  const agentEngine = useAppStore((s) => s.agentEngine);

  const mountedAt = useRef(Date.now());
  const [elapsed, setElapsed] = useState(0);

  // Tip only activates when showTip is true. When false, no timer is scheduled.
  const tipText = useThinkingTip(
    {
      surface: 'tui',
      engine: agentEngine,
      recommendLiteUi: false, // Never show launch-only tips during thinking.
    },
    showTip
  );

  useEffect(() => {
    const id = setInterval(() => {
      setElapsed(Date.now() - mountedAt.current);
    }, 1000);
    return () => clearInterval(id);
  }, []);

  const message = getComfortMessage(elapsed, thinkingEnabled);

  // The tip connector aligns with the start of the StatusBar content zone
  // (bar width + margin = STATUS_BAR_CONTENT_OFFSET characters).
  const tipIndent = ' '.repeat(STATUS_BAR_CONTENT_OFFSET);

  return (
    <Box flexDirection="column">
      <StatusBar status="thinking" barColor={barColor}>
        <Text>
          {secondaryColor(message)}
          {dim(` (${keybindings.label('cancelStream')} to cancel)`)}
          {retryStatus && (
            <>
              {'\n'}
              {warning(retryStatus.message)}
            </>
          )}
        </Text>
      </StatusBar>
      {tipText && (
        <Text>
          {dim(`${tipIndent}${glyphs.cornerBottomLeftRound} Tip: ${tipText}`)}
        </Text>
      )}
    </Box>
  );
};
