import React, { useState, useEffect } from 'react';
import { Box, Text } from '../../renderer.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import {
  useGlyphs,
  useSpinners,
  useAllowIcons,
} from '../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../contexts/AnimationPausedContext.js';
import type { AgentSession } from '../../types/multi-session.js';

export interface SessionListProps {
  sessions: AgentSession[];
  selectedId?: string;
  onSelect: (sessionId: string) => void;
  width?: number;
}

const getStatusIcon = (
  status: AgentSession['status'],
  spinnerIndex: number,
  spinnerChars: string[],
  glyphs: { dotEmpty: string; checkmark: string; cross: string },
  allowIcons: boolean
) => {
  if (!allowIcons) return '';
  switch (status) {
    case 'idle':
      return glyphs.dotEmpty;
    case 'busy':
      return spinnerChars[spinnerIndex];
    case 'terminated':
      return glyphs.checkmark;
    case 'failed':
      return glyphs.cross;
    default:
      return glyphs.dotEmpty;
  }
};

const getSummaryIndicator = (session: AgentSession, clipboard: string) => {
  if (session.status === 'terminated' && session.summary) {
    return `${clipboard} `;
  }
  return '';
};

export const SessionList: React.FC<SessionListProps> = React.memo(
  ({ sessions, selectedId, onSelect, width = 30 }) => {
    const [focusedIndex, setFocusedIndex] = useState(0);
    const [spinnerIndex, setSpinnerIndex] = useState(0);
    const { getColor } = useTheme();
    const glyphs = useGlyphs();
    const spinners = useSpinners();
    const spinnerChars = spinners.brailleRotate;
    const paused = useAnimationPaused();
    const { allowIcons } = useAllowIcons();

    // Animate spinner only when at least one session is busy
    const hasBusy = sessions.some((s) => s.status === 'busy');
    useEffect(() => {
      if (!hasBusy) return;
      if (paused) return;
      const interval = setInterval(() => {
        setSpinnerIndex((prev) => (prev + 1) % spinnerChars.length);
      }, 100);
      return () => clearInterval(interval);
    }, [hasBusy, spinnerChars.length, paused]);

    // Update focused index when selectedId changes
    useEffect(() => {
      if (selectedId) {
        const index = sessions.findIndex((s) => s.id === selectedId);
        if (index >= 0) {
          setFocusedIndex(index);
        }
      }
    }, [selectedId, sessions]);

    useKeypress((input, key) => {
      if (key.upArrow) {
        setFocusedIndex((prev) => Math.max(0, prev - 1));
      } else if (key.downArrow) {
        setFocusedIndex((prev) => Math.min(sessions.length - 1, prev + 1));
      } else if (key.return && sessions[focusedIndex]) {
        onSelect(sessions[focusedIndex].id);
      } else if (key.home) {
        setFocusedIndex(0);
      } else if (key.end) {
        setFocusedIndex(sessions.length - 1);
      }
    });

    if (sessions.length === 0) {
      return (
        <Box width={width} paddingX={1}>
          <Text>{getColor('secondary')('No active sessions')}</Text>
        </Box>
      );
    }

    return (
      <Box flexDirection="column" width={width}>
        {sessions.map((session, index) => {
          const _isSelected = session.id === selectedId;
          const isFocused = index === focusedIndex;
          const agentColor = getAgentColor(session.name, getColor);

          const getStatusText = () => {
            const icon = getStatusIcon(
              session.status,
              spinnerIndex,
              spinnerChars,
              glyphs,
              allowIcons
            );
            switch (session.status) {
              case 'idle':
                return getColor('secondary')(`${icon} `);
              case 'busy':
                return getColor('success')(`${icon} `);
              case 'terminated':
                return getColor('success')(`${icon} `);
              case 'failed':
                return getColor('error')(`${icon} `);
              default:
                return getColor('secondary')(`${icon} `);
            }
          };

          const getNameText = () => {
            if (session.status === 'terminated') {
              return agentColor.strikethrough(session.name);
            } else if (session.status === 'idle') {
              return agentColor.dim(session.name);
            } else {
              return agentColor(session.name);
            }
          };

          return (
            <Box key={session.id} paddingX={1}>
              <Box
                width={width - 2}
                backgroundColor={isFocused ? 'blue' : undefined}
              >
                <Text>{getSummaryIndicator(session, glyphs.clipboard)}</Text>
                <Text>{getStatusText()}</Text>
                <Text>{getNameText()}</Text>
                <Text>{getColor('secondary')(` (${session.status})`)}</Text>
              </Box>
            </Box>
          );
        })}
      </Box>
    );
  }
);

SessionList.displayName = 'SessionList';
