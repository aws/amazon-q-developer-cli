import React from 'react';
import { Box, Text, type ComponentMouseEvent } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';
import type { ConnectionState } from '../32-acp-showcase/types.js';
import type { ManagedSession } from './sessions.js';

function stateColor(state: ConnectionState, theme: ShowcaseTheme): string {
  if (state === 'error') return theme.danger;
  if (state === 'running' || state === 'cancelling') return theme.warning;
  return theme.success;
}

export function SessionPanel({
  sessions,
  selectedId,
  width,
  height,
  theme,
  onSelect,
  onCreate,
  onRename,
  onClose,
  onContext,
}: {
  sessions: ManagedSession[];
  selectedId?: string;
  width: number;
  height: number;
  theme: ShowcaseTheme;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onRename: (session: ManagedSession) => void;
  onClose: (id: string) => void;
  onContext: (session: ManagedSession, event: ComponentMouseEvent) => void;
}): React.ReactElement {
  return (
    <Box flexDirection="column" width={width} height={height} backgroundColor={theme.panel}>
      <Box height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.raised}>
        <Text color={theme.fg} bold>
          SESSIONS
        </Text>
        <Text color={theme.accent} bold onClick={onCreate}>
          + NEW
        </Text>
      </Box>
      {sessions.map((session) => {
        const active = session.id === selectedId;
        return (
          <Box
            key={session.id}
            height={1}
            paddingX={1}
            backgroundColor={active ? theme.raised : theme.panel}
            onClick={() => onSelect(session.id)}
            onMouseDown={(event) => {
              if (event.button === 'right') onContext(session, event);
            }}
          >
            <Box flexGrow={1}>
              <Text color={stateColor(session.state.connection, theme)}>{active ? '>' : '-'}</Text>
              <Text color={active ? theme.accent : theme.fg} bold={active} wrap="truncate">
                {` ${session.title}`}
              </Text>
            </Box>
            <Text color={theme.muted} onClick={() => onRename(session)}>
              {' '}
              RENAME{' '}
            </Text>
            <Text color={theme.danger} onClick={() => onClose(session.id)}>
              {' '}
              CLOSE
            </Text>
          </Box>
        );
      })}
      {sessions.length === 0 && (
        <Box paddingX={1}>
          <Text color={theme.muted}>No sessions. Select + NEW to start one.</Text>
        </Box>
      )}
    </Box>
  );
}
