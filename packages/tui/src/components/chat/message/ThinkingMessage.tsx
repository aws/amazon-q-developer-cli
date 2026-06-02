import React, { useState, useEffect, useRef } from 'react';
import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useAppStore } from '../../../stores/app-store.js';
import { useShowThinking } from '../../../hooks/useGlyphs.js';
import { getComfortMessage } from './comfort-messages.js';

interface ThinkingMessageProps {
  barColor?: string;
}

/**
 * Inline thinking indicator with tiered comfort messaging.
 *
 * When an HTTP retry is in progress (the SDK is backing off between attempts
 * on a transient error), a second line is appended with the attempt counter
 * and countdown text.
 */
export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({
  barColor,
}) => {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');
  const dim = getColor('muted');
  const warning = getColor('warning');
  const keybindings = useKeybindings();
  const retryStatus = useAppStore((s) => s.retryStatus);
  const { showThinking: thinkingEnabled } = useShowThinking();

  const mountedAt = useRef(Date.now());
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const id = setInterval(() => {
      setElapsed(Date.now() - mountedAt.current);
    }, 1000);
    return () => clearInterval(id);
  }, []);

  const message = getComfortMessage(elapsed, thinkingEnabled);

  return (
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
  );
};
