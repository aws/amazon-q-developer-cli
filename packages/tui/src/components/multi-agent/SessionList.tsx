import React, { useState, useEffect } from 'react';
import { Box, Text } from 'ink';
import { useKeypress } from '../../hooks/useKeypress.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import type { AgentSession } from '../../types/multi-session.js';

export interface SessionListProps {
  sessions: AgentSession[];
  selectedId?: string;
  onSelect: (sessionId: string) => void;
  width?: number;
}

const SPINNER_CHARS = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const getStatusIcon = (
  status: AgentSession['status'],
  spinnerIndex: number
) => {
  switch (status) {
    case 'idle':
      return '○';
    case 'busy':
      return SPINNER_CHARS[spinnerIndex];
    case 'terminated':
      return '✓';
    case 'failed':
      return '✗';
    default:
      return '○';
  }
};

const getSummaryIndicator = (session: AgentSession) => {
  if (session.status === 'terminated' && session.summary) {
    return '📋 ';
  }
  return '';
};

export const SessionList: React.FC<SessionListProps> = React.memo(
  ({ sessions, selectedId, onSelect, width = 30 }) => {
    const [focusedIndex, setFocusedIndex] = useState(0);
    const [spinnerIndex, setSpinnerIndex] = useState(0);
    const { getColor } = useTheme();

    // Animate spinner for busy sessions
    useEffect(() => {
      const interval = setInterval(() => {
        setSpinnerIndex((prev) => (prev + 1) % SPINNER_CHARS.length);
      }, 100);

      return () => clearInterval(interval);
    }, []);

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
            const icon = getStatusIcon(session.status, spinnerIndex);
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
                <Text>{getSummaryIndicator(session)}</Text>
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
